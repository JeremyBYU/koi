use crate::gpu::Gpu;
use bytemuck::{Pod, Zeroable};
use crate::water::{LOCK_SIZE, lock_table, smoothstep, steps};
use koi_sim::{JOINTS, Koi, Pose, School, Variety, noise2};
use koi_theme::{Outline, PaletteLock, Rgb, Theme};
use std::f32::consts::{PI, TAU};

/// Catmull-Rom samples per chain link when a pose lays the body along the spine.
const SAMPLES: usize = 4;
/// Points in a posed spine: a lead-in past the head, the samples, the tail joint, and a
/// run-out past it for the tail fin. Keep in step with koi.wgsl.
const CURVE: usize = (JOINTS - 1) * SAMPLES + 3;
/// Tail-beat strengths the body is painted at. A pose blends the two nearest.
const BEATS: [f32; 3] = [0.2, 0.55, 1.0];
/// Head tip to the end of the tail fin, in BL.
const TAIL: f32 = 1.3;
/// How far a koi at full dive depth fades toward `deep`, seen through the water over it.
/// Keep in step with koi.wgsl.
const DIVE_DIM: f32 = 0.3;

// Sprite texture extent in BL around the fish centre, head towards +u.
const TEX_U0: f32 = -0.84;
const TEX_U1: f32 = 0.54;
const TEX_V: f32 = 0.36;

/// The spine as a smooth curve in BL relative to the centre: (x, y, distance from the
/// head joint in BL, unused). Straight lead-in and run-out cover the texture's ends.
fn curve(pose: &Pose) -> [[f32; 4]; CURVE] {
    let p: [(f32, f32); JOINTS] = pose.spine.map(|(x, y)| ((x - pose.x) / pose.len, (y - pose.y) / pose.len));
    let (head, tail) = (p[0], p[JOINTS - 1]);
    // Mirrored ghost joints at both ends, so the spline keeps the end links' direction.
    let mut ext = [(0.0, 0.0); JOINTS + 2];
    ext[1..=JOINTS].copy_from_slice(&p);
    ext[0] = (2.0 * head.0 - p[1].0, 2.0 * head.1 - p[1].1);
    ext[JOINTS + 1] = (2.0 * tail.0 - p[JOINTS - 2].0, 2.0 * tail.1 - p[JOINTS - 2].1);
    let reach = |from: (f32, f32), towards: (f32, f32), by: f32| {
        let (dx, dy) = (from.0 - towards.0, from.1 - towards.1);
        let n = dx.hypot(dy).max(1e-6);
        (from.0 + dx / n * by, from.1 + dy / n * by)
    };
    let link = 1.0 / (JOINTS - 1) as f32;
    let mut curve = [[0.0; 4]; CURVE];
    let lead = reach(head, p[1], 0.05);
    curve[0] = [lead.0, lead.1, -0.05, 0.0];
    for span in 0..JOINTS - 1 {
        let [p0, p1, p2, p3] = [ext[span], ext[span + 1], ext[span + 2], ext[span + 3]];
        for n in 0..SAMPLES {
            let s = n as f32 / SAMPLES as f32;
            let cr = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (c - a) * s + (2.0 * a - 5.0 * b + 4.0 * c - d) * s * s + (3.0 * b - a - 3.0 * c + d) * s * s * s);
            curve[1 + span * SAMPLES + n] = [cr(p0.0, p1.0, p2.0, p3.0), cr(p0.1, p1.1, p2.1, p3.1), (span as f32 + s) * link, 0.0];
        }
    }
    curve[CURVE - 2] = [tail.0, tail.1, 1.0, 0.0];
    let run_out = reach(tail, p[JOINTS - 2], 0.35);
    curve[CURVE - 1] = [run_out.0, run_out.1, 1.35, 0.0];
    curve
}

/// Renders koi sprites, one koi at a time, into a reused RGBA buffer.
///
/// Each koi is painted once per tail-beat strength as a straight body texture. A pose lays
/// that texture along a Catmull-Rom curve through the spine joints, adds the travelling tail
/// wave and blends the two nearest beat strengths. The GPU and the CPU do the same resampling.
/// In a pixel theme (`style.pixel_px` above 0) the bodies have hard edges and a pose takes the
/// nearest texel of the nearest beat, so the sprite has no anti-aliasing, and with
/// `palette_lock = "all"` its colours snap to the swatches.
pub struct Poser {
    bodies: Vec<[Body; 3]>,
    scale: f32,
    /// How far any painted fin or body reaches to the side of the spine, in BL.
    reach: f32,
    rgba: Vec<u8>,
    look: Look,
    /// The palette lock table, empty unless the theme locks the koi too.
    lock: Vec<u32>,
    gpu: Option<GpuPoser>,
}

/// What the pose pass needs from the theme to draw outlines.
#[derive(Clone, Copy)]
struct Look {
    mode: u32,
    /// 1 in pixel themes: nearest texels, no anti-aliasing.
    crisp: u32,
    sun: [f32; 2],
    /// outline, shadow, mid (standing in for the water under the fish), deep (the water
    /// over a diving koi) and highlight (a pleased koi's shimmer), in 0..1 sRGB.
    colors: [[f32; 4]; 5],
}

impl Look {
    fn new(theme: &Theme) -> Look {
        let p = &theme.palette;
        let srgb = |c: Rgb| {
            let [r, g, b] = c.map(|v| f32::from(v) / 255.0);
            [r, g, b, 0.0]
        };
        let mode = match theme.style.outline {
            Outline::None => 0,
            Outline::Soft => 1,
            Outline::Dark => 2,
            Outline::Selout => 3,
            Outline::Rim => 4,
        };
        Look { mode, crisp: u32::from(theme.style.pixel_px > 0), sun: theme.light.sun, colors: [srgb(p.outline), srgb(p.shadow), srgb(p.mid), srgb(p.deep), srgb(p.highlight)] }
    }
}

/// The outline colour for an edge pixel of `body` (straight sRGB) in outline `mode`, where
/// `facing` is how much the edge faces the sun, -1 to 1. koi.wgsl has the same.
fn edge_color(body: [f32; 3], mode: u32, facing: f32, look: &Look) -> [f32; 3] {
    let [outline, shadow, water, _, _] = look.colors.map(|[r, g, b, _]| [r, g, b]);
    match mode {
        1 => mix(mix(water, body, 0.3), outline, 0.8),
        3 if facing > 0.25 => mix(body, outline, 0.45),
        4 if facing > 0.0 => mix(outline, body, 0.2),
        4 => mix(body, shadow, 0.6),
        _ => outline,
    }
}

struct GpuPoser {
    gpu: Gpu,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    texels: wgpu::Buffer,
    edges: wgpu::Buffer,
    lock: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PoseParams {
    origin: [f32; 2],
    size: [u32; 2],
    len: f32,
    phase: f32,
    amp: f32,
    blend: f32,
    tex_u0: f32,
    tex_v: f32,
    tex_w: u32,
    tex_h: u32,
    tex_a: u32,
    tex_b: u32,
    mode: u32,
    crisp: u32,
    sun: [f32; 2],
    lock: u32,
    depth: f32,
    joy: f32,
    /// WGSL aligns the colours after `joy` to 16 bytes.
    pad: [f32; 3],
    colors: [[f32; 4]; 5],
    curve: [[f32; 4]; CURVE],
}

/// A koi swimming straight along +u, `l` texels per BL, premultiplied RGBA in 0..1, and
/// per texel how much of the outline colour replaces the body there (0 inside, 1 on the edge).
struct Body {
    w: usize,
    h: usize,
    l: f32,
    texels: Vec<[f32; 4]>,
    edges: Vec<f32>,
    offset: usize,
}

/// The two painted beat strengths around the pose's energy, the blend between them, and the
/// tail wave amplitude in BL: a pleased koi flutters its tail a little even while still.
/// `Poser::largest` counts on the amplitude staying within 0.1.
fn beat(pose: &Pose) -> (usize, f32, f32) {
    let e = pose.energy.clamp(BEATS[0], BEATS[2]);
    let i = if e < BEATS[1] { 0 } else { 1 };
    (i, (e - BEATS[i]) / (BEATS[i + 1] - BEATS[i]), (0.1 * e).max(0.07 * pose.joy))
}

impl Poser {
    /// Paints every koi in `school` in `theme`. `scale` is sprite pixels per water pixel.
    /// With a `gpu`, poses render there; without one, on the CPU. The koi in `school` must
    /// not change afterwards: index `k` in the other calls is `school.fish[k]`.
    pub fn new(gpu: Option<&Gpu>, school: &School, theme: &Theme, scale: f32) -> Poser {
        let bodies = paint_school(school, theme, scale);
        let reach = bodies
            .iter()
            .flatten()
            .flat_map(|b| b.texels.iter().enumerate().filter(|(_, t)| t[3] > 0.5 / 255.0).map(|(i, _)| ((i / b.w) as f32 / b.l - TEX_V).abs()))
            .fold(0.0, f32::max);
        let lock = if theme.style.palette_lock == PaletteLock::All { lock_table(&theme.palette) } else { Vec::new() };
        // An empty pond would need empty GPU buffers, which wgpu refuses.
        let gpu = gpu.filter(|_| !bodies.is_empty()).map(|gpu| GpuPoser::new(gpu, &bodies, &lock));
        Poser { bodies, scale, reach, rgba: Vec::new(), look: Look::new(theme), lock, gpu }
    }

    /// Repaints every koi in `theme`. `school` must be the one this poser was made for. Only
    /// colours change, so every size and bound stays as it was. A switch between a painted
    /// and a pixel theme changes the sprite scale too, so it needs a new `Poser` instead.
    pub fn recolor(&mut self, school: &School, theme: &Theme) {
        self.bodies = paint_school(school, theme, self.scale);
        self.look = Look::new(theme);
        self.lock = if theme.style.palette_lock == PaletteLock::All { lock_table(&theme.palette) } else { Vec::new() };
        if let Some(g) = &self.gpu {
            if !self.lock.is_empty() {
                g.gpu.queue.write_buffer(&g.lock, 0, bytemuck::cast_slice(&self.lock));
            }
            let texels: Vec<[f32; 4]> = self.bodies.iter().flatten().flat_map(|b| b.texels.iter().copied()).collect();
            let edges: Vec<f32> = self.bodies.iter().flatten().flat_map(|b| b.edges.iter().copied()).collect();
            g.gpu.queue.write_buffer(&g.texels, 0, bytemuck::cast_slice(&texels));
            g.gpu.queue.write_buffer(&g.edges, 0, bytemuck::cast_slice(&edges));
        }
    }

    /// Sprite pixels that any pose of koi `k` fits within, across and down: 1.4 BL from
    /// head lead-in to fin run-out, plus the side reach and the tail wave on both sides.
    pub fn largest(&self, k: usize) -> i32 {
        ((1.4 + 2.0 * (self.reach + 0.1)) * self.bodies[k][0].l).ceil() as i32
    }

    /// The pixel box, relative to the fish centre, that holds koi `k` in `pose`:
    /// (left, top, right, bottom), inclusive.
    pub fn bounds(&self, k: usize, pose: &Pose) -> (i32, i32, i32, i32) {
        let l = self.bodies[k][0].l;
        let reach = self.reach + beat(pose).2;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for [x, y, _, _] in curve(pose) {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        (((x0 - reach) * l).floor() as i32 - 1, ((y0 - reach) * l).floor() as i32 - 1, ((x1 + reach) * l).ceil() as i32 + 1, ((y1 + reach) * l).ceil() as i32 + 1)
    }

    /// Koi `k` in `pose`, as straight-alpha RGBA covering `w` x `h` sprite pixels whose
    /// top-left is (`x0`, `y0`) from the fish centre. A fractional origin slides the koi
    /// inside the sprite, for motion finer than the placement grid.
    pub fn pose(&mut self, k: usize, pose: &Pose, x0: f32, y0: f32, w: usize, h: usize) -> &[u8] {
        self.rgba.clear();
        self.rgba.resize(w * h * 4, 0);
        let (i, blend, amp) = beat(pose);
        let (body, next) = (&self.bodies[k][i], &self.bodies[k][i + 1]);
        let l = body.l;
        let curve = curve(pose);
        match &mut self.gpu {
            Some(g) => {
                let params = PoseParams {
                    origin: [x0, y0],
                    size: [w, h].map(|n| u32::try_from(n).expect("sprite size fits u32")),
                    len: l,
                    phase: pose.phase,
                    amp,
                    blend,
                    tex_u0: TEX_U0,
                    tex_v: TEX_V,
                    tex_w: u32::try_from(body.w).expect("texture fits u32"),
                    tex_h: u32::try_from(body.h).expect("texture fits u32"),
                    tex_a: u32::try_from(body.offset).expect("texture offset fits u32"),
                    tex_b: u32::try_from(next.offset).expect("texture offset fits u32"),
                    mode: self.look.mode,
                    crisp: self.look.crisp,
                    sun: self.look.sun,
                    lock: u32::from(!self.lock.is_empty()),
                    depth: pose.depth,
                    joy: pose.joy,
                    pad: [0.0; 3],
                    colors: self.look.colors,
                    curve,
                };
                g.reserve(u64::try_from(self.rgba.len()).expect("sprite bytes fit u64"));
                g.gpu.queue.write_buffer(&g.params, 0, bytemuck::bytes_of(&params));
                let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&g.pipeline);
                    pass.set_bind_group(0, &g.bind_group, &[]);
                    pass.dispatch_workgroups(params.size[0].div_ceil(16), params.size[1].div_ceil(16), 1);
                }
                g.gpu.finish_into(encoder, &g.out, &g.readback, &mut self.rgba);
            }
            None => {
                // Keep in step with koi.wgsl.
                const SEGS: usize = CURVE - 1;
                let (mut qx, mut qy, mut tx, mut ty, mut inv) = ([0.0f32; SEGS], [0.0f32; SEGS], [0.0f32; SEGS], [0.0f32; SEGS], [0.0f32; SEGS]);
                for n in 0..SEGS {
                    (qx[n], qy[n], tx[n], ty[n]) = (curve[n][0], curve[n][1], curve[n + 1][0] - curve[n][0], curve[n + 1][1] - curve[n][1]);
                    inv[n] = 1.0 / (tx[n] * tx[n] + ty[n] * ty[n]);
                }
                let dist2 = |px: f32, py: f32, n: usize| {
                    let s = (((px - qx[n]) * tx[n] + (py - qy[n]) * ty[n]) * inv[n]).clamp(0.0, 1.0);
                    let (dx, dy) = (px - qx[n] - tx[n] * s, py - qy[n] - ty[n] * s);
                    dx * dx + dy * dy
                };
                // Work in 8x8 blocks. A block whose centre is too far from the spine for any
                // pixel to show is skipped. Otherwise only segments that come within a block
                // diagonal of the closest one can be nearest to any of its pixels.
                let (half, far) = (6.0 / l, self.reach + amp + 6.0 / l);
                for (by, bx) in (0..h).step_by(8).flat_map(|by| (0..w).step_by(8).map(move |bx| (by, bx))) {
                    let (cx, cy) = ((x0 + (bx + 4) as f32) / l, (y0 + (by + 4) as f32) / l);
                    let d: [f32; SEGS] = std::array::from_fn(|n| dist2(cx, cy, n).sqrt());
                    let closest = d.iter().copied().fold(f32::MAX, f32::min);
                    if closest > far {
                        continue;
                    }
                    let candidates: Vec<usize> = (0..SEGS).filter(|&n| d[n] <= closest + 2.0 * half).collect();
                    for (row, col) in (by..(by + 8).min(h)).flat_map(|row| (bx..(bx + 8).min(w)).map(move |col| (row, col))) {
                        let (px, py) = ((x0 + col as f32) / l, (y0 + row as f32) / l);
                        let (_, n) = candidates.iter().map(|&n| (dist2(px, py, n), n)).fold((f32::MAX, 0), |best, c| if c.0 < best.0 { c } else { best });
                        let s = (((px - qx[n]) * tx[n] + (py - qy[n]) * ty[n]) * inv[n]).clamp(0.0, 1.0);
                        let (dx, dy) = (px - qx[n] - tx[n] * s, py - qy[n] - ty[n] * s);
                        let (a, vb) = (curve[n][2] + (curve[n + 1][2] - curve[n][2]) * s, (dx * ty[n] - dy * tx[n]) * inv[n].sqrt());
                        let t = (a / TAIL).clamp(0.0, 1.0);
                        let tv = vb - amp * (0.1 + 0.9 * t * t) * (pose.phase - TAU * t).sin();
                        let (seg_x, seg_y) = (tx[n], ty[n]);
                        let (tx, ty) = ((0.5 - a - TEX_U0) * l, (tv + TEX_V) * l);
                        if tx < 0.0 || ty < 0.0 || tx >= (body.w - 1) as f32 || ty >= (body.h - 1) as f32 {
                            continue;
                        }
                        let (ix, iy) = (tx as usize, ty as usize);
                        let (fx, fy) = (tx - ix as f32, ty - iy as f32);
                        let i = iy * body.w + ix;
                        let texel: [f32; 5] = if self.look.crisp == 1 {
                            let j = (ty + 0.5) as usize * body.w + (tx + 0.5) as usize;
                            let b = if blend < 0.5 { body } else { next };
                            let [r, g, bl, a] = b.texels[j];
                            [r, g, bl, a, b.edges[j]]
                        } else {
                            let bilerp = |a: f32, b: f32, c: f32, d: f32| {
                                let top = a + (b - a) * fx;
                                top + (c + (d - c) * fx - top) * fy
                            };
                            let sample = |b: &Body| -> [[f32; 5]; 4] { [i, i + 1, i + body.w, i + body.w + 1].map(|j| { let [r, g, bl, a] = b.texels[j]; [r, g, bl, a, b.edges[j]] }) };
                            let ([t00, t10, t01, t11], [n00, n10, n01, n11]) = (sample(body), sample(next));
                            std::array::from_fn(|ch| {
                                let here = bilerp(t00[ch], t10[ch], t01[ch], t11[ch]);
                                here + (bilerp(n00[ch], n10[ch], n01[ch], n11[ch]) - here) * blend
                            })
                        };
                        if texel[3] < 0.5 / 255.0 {
                            continue;
                        }
                        let straight = [texel[0] / texel[3], texel[1] / texel[3], texel[2] / texel[3]];
                        // A pleased koi's head shimmers, in bands running back from the nose.
                        let glow = 0.25 * pose.joy * (1.0 - smoothstep(0.15, 0.45, a)) * (0.5 + 0.5 * (TAU * 5.0 * a - 2.0 * pose.phase).sin());
                        let [hr, hg, hb, _] = self.look.colors[4];
                        let straight = mix(straight, [hr, hg, hb], glow);
                        // The side of the body this pixel is on, in screen space, against the sun.
                        let side = if vb < 0.0 { -1.0 } else { 1.0 } * inv[n].sqrt();
                        let facing = (seg_y * self.look.sun[0] - seg_x * self.look.sun[1]) * side;
                        let [dr, dg, db, _] = self.look.colors[3];
                        let color = mix(mix(straight, edge_color(straight, self.look.mode, facing, &self.look), texel[4]), [dr, dg, db], DIVE_DIM * pose.depth);
                        let o = (row * w + col) * 4;
                        let [r, g, b] = color.map(|c| (c * 255.0).min(255.0) as u8);
                        self.rgba[o..o + 4].copy_from_slice(&[r, g, b, (texel[3] * 255.0) as u8]);
                        if !self.lock.is_empty() {
                            let [r, g, b] = [r, g, b].map(|c| usize::from(c >> 3));
                            self.rgba[o..o + 3].copy_from_slice(&self.lock[r << 10 | g << 5 | b].to_le_bytes()[..3]);
                        }
                    }
                }
            }
        }
        &self.rgba
    }
}

/// Every koi's body at each beat strength, with each body's offset into the shared texel buffer.
fn paint_school(school: &School, theme: &Theme, scale: f32) -> Vec<[Body; 3]> {
    let mut offset = 0;
    school
        .fish
        .iter()
        .map(|f| {
            BEATS.map(|beat| {
                let body = Body::paint(f, theme, f.len * scale, beat);
                let body = Body { offset, ..body };
                offset += body.texels.len();
                body
            })
        })
        .collect()
}

impl GpuPoser {
    fn new(gpu: &Gpu, bodies: &[[Body; 3]], lock: &[u32]) -> GpuPoser {
        let device = &gpu.device;
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pose params"),
            size: std::mem::size_of::<PoseParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texels: Vec<[f32; 4]> = bodies.iter().flatten().flat_map(|b| b.texels.iter().copied()).collect();
        let texel_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koi bodies"),
            size: u64::try_from(std::mem::size_of_val(texels.as_slice())).expect("texels fit u64"),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&texel_buffer, 0, bytemuck::cast_slice(&texels));
        let edges: Vec<f32> = bodies.iter().flatten().flat_map(|b| b.edges.iter().copied()).collect();
        let edge_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koi edges"),
            size: u64::try_from(std::mem::size_of_val(edges.as_slice())).expect("edges fit u64"),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(&edge_buffer, 0, bytemuck::cast_slice(&edges));
        let lock_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("koi palette lock"),
            size: 4 * LOCK_SIZE as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !lock.is_empty() {
            gpu.queue.write_buffer(&lock_buffer, 0, bytemuck::cast_slice(lock));
        }
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
                entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(3, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(4, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let module = device.create_shader_module(wgpu::include_wgsl!("koi.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pose"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("pose"),
            compilation_options: Default::default(),
            cache: None,
        });
        let (out, readback, bind_group) = Self::buffers(gpu, &layout, [&params, &texel_buffer, &edge_buffer, &lock_buffer], 4);
        GpuPoser { gpu: gpu.clone(), pipeline, layout, params, texels: texel_buffer, edges: edge_buffer, lock: lock_buffer, out, readback, bind_group, capacity: 4 }
    }

    fn buffers(gpu: &Gpu, layout: &wgpu::BindGroupLayout, inputs: [&wgpu::Buffer; 4], size: u64) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        let [params, texels, edges, lock] = inputs;
        let out = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("pose out"), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("pose readback"), size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: out.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: texels.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: edges.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: lock.as_entire_binding() },
            ],
        });
        (out, readback, bind_group)
    }

    /// Grows the output buffers to hold `bytes`. Sprites vary in size with the pose.
    fn reserve(&mut self, bytes: u64) {
        if bytes > self.capacity {
            (self.out, self.readback, self.bind_group) = Self::buffers(&self.gpu, &self.layout, [&self.params, &self.texels, &self.edges, &self.lock], bytes);
            self.capacity = bytes;
        }
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|ch| a[ch] + (b[ch] - a[ch]) * t)
}

impl Body {
    /// Paints koi `f` straight along +u at `l` texels per BL, cel-shaded in `style.koi_tones`
    /// tones, with pectoral fins spread for tail-beat strength `beat` (gliding koi spread
    /// them, bursting koi tuck them). The outline is not painted here: `edges` marks where
    /// the pose pass draws it, since its sun side turns with the fish.
    fn paint(f: &Koi, theme: &Theme, l: f32, beat: f32) -> Body {
        let (palette, light_cfg, style) = (&theme.palette, &theme.light, &theme.style);
        let rgb = |c: Rgb| c.map(|v| f32::from(v) / 255.0);
        let (white, red, sumi, gold, blue, orange) = (rgb(palette.koi_white), rgb(palette.koi_red), rgb(palette.koi_sumi), rgb(palette.ogon), rgb(palette.asagi_blue), rgb(palette.asagi_red));
        let (light, shadow, ink) = (rgb(palette.highlight), rgb(palette.shadow), rgb(palette.outline));
        let tones = f32::from(style.koi_tones.max(2) - 1);
        let (fin, fin_root) = match f.variety {
            Variety::Kohaku | Variety::Sanke => (white, white),
            Variety::Showa => (white, sumi),
            Variety::Ogon => (gold, gold),
            Variety::Asagi => (white, orange),
        };
        let fin = mix(fin, light, 0.15);
        let seed = f.seed;
        let spread = 0.95 - 0.5 * beat;
        // How much of a texel a shape covers, from its signed distance to the shape's edge in
        // texels. Pixel themes cover whole texels or none.
        let cover = |edge: f32| if style.pixel_px > 0 { if edge >= 0.0 { 1.0 } else { 0.0 } } else { (edge + 0.5).clamp(0.0, 1.0) };
        let (w, h) = (((TEX_U1 - TEX_U0) * l).ceil() as usize + 1, (2.0 * TEX_V * l).ceil() as usize + 1);
        let widths = [0.07, 0.108, 0.118, 0.112, 0.095, 0.07, 0.045, 0.026];
        let body_len = 0.9;

        let mut texels = vec![[0.0f32; 4]; w * h];
        for ty in 0..h {
            for tx in 0..w {
                let (u, v) = (TEX_U0 + tx as f32 / l, ty as f32 / l - TEX_V);
                let (s, a) = (0.5 - u, v.abs());
                let mut out = [0.0f32; 4];
                let mut over = |c: [f32; 3], alpha: f32| {
                    if alpha > 0.0 {
                        for ch in 0..3 {
                            out[ch] = c[ch] * alpha + out[ch] * (1.0 - alpha);
                        }
                        out[3] = alpha + out[3] * (1.0 - alpha);
                    }
                };

                // Tail fin: long, forked, flowing, from the narrow peduncle.
                let q = (s - 0.84) / (TAIL - 0.84);
                if (0.0..1.05).contains(&q) {
                    let half = 0.02 + 0.17 * q.max(0.0).powf(0.85);
                    let fork = 1.0 - 0.3 * (1.0 - (a / half).min(1.0)).powi(2) + 0.03 * (a * 30.0 + seed as f32).sin();
                    let edge = ((half - a) * l).min((fork - q) * (TAIL - 0.84) * l);
                    if edge > -0.5 {
                        let rays = 0.82 + 0.18 * (a.atan2(s - 0.78) * 36.0).cos();
                        let c = mix(fin_root, fin, smoothstep(0.1, 0.4, q));
                        over(c, cover(edge) * ((0.7 - 0.3 * q) * rays + smoothstep(1.6, 0.4, edge) * 0.2));
                    }
                }
                // A fin as a translucent blade from `root` along `angle` (from straight back,
                // outwards), `len` long and `wide` at its widest, with faint rays.
                let mut blade = |root_s: f32, root_a: f32, angle: f32, len: f32, wide: f32, alpha: f32| {
                    let (ds, da) = (s - root_s, a - root_a);
                    let along = (ds * angle.cos() + da * angle.sin()) / len;
                    let across = -ds * angle.sin() + da * angle.cos();
                    if !(0.0..1.0).contains(&along) {
                        return;
                    }
                    let half = wide * (PI * along.powf(1.15)).sin();
                    let edge = (half - across.abs()) * l;
                    if edge > -0.5 {
                        let rays = 0.85 + 0.15 * (across.atan2(along * len) * 30.0).cos();
                        let c = mix(if along < 0.4 { fin_root } else { fin }, fin, smoothstep(0.3, 0.45, along));
                        over(c, cover(edge) * (alpha * (1.0 - 0.3 * along) * rays + smoothstep(1.6, 0.4, edge) * 0.2));
                    }
                };

                blade(0.56, 0.045, 0.6, 0.12, 0.03, 0.55);
                blade(0.23, 0.075, spread, 0.25, 0.065, 0.6);

                let t = s / body_len;
                if (0.0..=1.0).contains(&t) {
                    let x = t * (widths.len() - 1) as f32;
                    let i = (x as usize).min(widths.len() - 2);
                    let mut half = widths[i] + (widths[i + 1] - widths[i]) * smoothstep(0.0, 1.0, x - i as f32);
                    if s < 0.07 {
                        half *= (1.0 - ((0.07 - s) / 0.07).powi(2)).sqrt();
                    }
                    let edge = (half - a) * l;
                    if edge > -0.5 {
                        let n = a / half.max(1e-4);
                        let soft = 2.5 / l;
                        let fade = 0.35 * smoothstep(0.6, 1.0, n) + 0.3 * smoothstep(0.72, 0.92, t);
                        let patch = |p: f32, at: f32| smoothstep(at - soft, at + soft, p);
                        let p = 0.65 * noise2(seed, s * 3.5, v * 6.0) + 0.35 * noise2(seed ^ 0x51, s * 8.0, v * 12.0) + 0.12 * smoothstep(0.24, 0.1, s) * smoothstep(0.0, 0.04, s) - fade;
                        let net = {
                            let (fa, fb) = (((s + v) / 0.05).fract(), ((s - v) / 0.05).fract());
                            let e = fa.min(1.0 - fa).min(fb.min(1.0 - fb));
                            smoothstep(0.16, 0.06, e) * smoothstep(0.12, 0.2, s) * smoothstep(0.9, 0.75, t)
                        };
                        let mut c = match f.variety {
                            Variety::Kohaku => mix(white, red, patch(p, 0.45)),
                            Variety::Sanke => {
                                let spots = patch(noise2(seed ^ 0x77, s * 10.0, v * 14.0), 0.74) * smoothstep(0.2, 0.26, s) * smoothstep(0.85, 0.78, t);
                                mix(mix(white, red, patch(p, 0.53)), sumi, spots)
                            }
                            Variety::Showa => {
                                let pale = patch(noise2(seed ^ 0x33, s * 3.0, v * 5.0) - fade * 0.5, 0.45);
                                mix(mix(sumi, white, pale), red, patch(p, 0.52))
                            }
                            Variety::Ogon => mix(gold, mix(gold, light, 0.35), net * 0.5),
                            Variety::Asagi => {
                                let back = mix(mix(blue, white, 0.15), mix(blue, shadow, 0.3), net);
                                let head = mix(back, mix(blue, white, 0.6), smoothstep(0.18, 0.1, s));
                                let flank = smoothstep(0.66, 0.76, n).max(smoothstep(0.16, 0.1, s) * smoothstep(0.45, 0.55, n));
                                mix(head, orange, flank)
                            }
                        };
                        c = mix(c, mix(c, [0.0; 3], 0.15), smoothstep(0.013, 0.005, a) * smoothstep(0.28, 0.34, s) * smoothstep(0.72, 0.66, s));

                        // Cel tones: the lit ridge of the back warms toward `highlight`, the
                        // flanks and the tail cool toward `shadow`.
                        let lit = smoothstep(0.42, 0.34, n) * smoothstep(0.05, 0.12, s) * smoothstep(0.8, 0.62, t);
                        let shade = smoothstep(0.66, 0.74, n).max(0.7 * smoothstep(0.72, 0.95, t));
                        let tone = steps((0.5 + 0.5 * lit - 0.5 * shade).clamp(0.0, 1.0), tones, style.band_softness);
                        c = mix(c, light, (tone - 0.5).max(0.0) * 2.0 * (0.15 + 0.5 * light_cfg.warm));
                        c = mix(c, mix(mix(c, [0.0; 3], 0.2), shadow, 0.15 + 0.5 * light_cfg.cool), (0.5 - tone).max(0.0) * 2.0);
                        let glint = (-(((s - 0.12) / 0.045).powi(2) + (v / 0.028).powi(2))).exp();
                        c = mix(c, light, 0.55 * glint);
                        over(c, cover(edge));

                        let eye = (0.016 - (s - 0.1).hypot(a - 0.074)) * l;
                        over(mix(sumi, ink, 0.4), cover(eye) * 0.9);
                    }
                }
                texels[ty * w + tx] = out;
            }
        }

        // An edge texel is covered while a neighbour is not. Translucent fins drop too little
        // to count, so only the body and the fin roots get a line. Painted outlines fade in
        // with the drop; the other modes are a solid line along where coverage crosses 3/4.
        let edges = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let here = texels[i][3];
                if style.outline == Outline::None || here < 0.5 / 255.0 {
                    return 0.0;
                }
                let around = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
                let least = around.iter().map(|j| j.map_or(0.0, |j| texels[j][3])).fold(here, f32::min);
                let drop = here - least;
                if style.outline == Outline::Soft { smoothstep(0.55, 0.9, drop) } else if here >= 0.75 && least < 0.75 { 1.0 } else { 0.0 }
            })
            .collect();
        Body { w, h, l, texels, edges, offset: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selective outlines are light on the edge that faces the sun and dark on the other, in
    /// screen space: a koi heading east or west keeps its light edge on top, toward the sun
    /// in the upper left. A sun side baked into the texture would flip with the fish.
    #[test]
    fn selout_light_edge_stays_toward_the_sun() {
        let theme = koi_theme::Catalog::load(None).0.resolve("hillside-summer").expect("built-in theme");
        assert_eq!(theme.style.outline, Outline::Selout);
        let school = School::new(320, 190, 1, 3);
        // Only a chosen Vulkan driver (lavapipe in CI) is used, and then it must be there.
        let gpu = std::env::var_os("VK_ICD_FILENAMES").map(|_| Gpu::new().expect("VK_ICD_FILENAMES is set, so an adapter must be found"));
        let mut backends = vec![None];
        if let Some(gpu) = &gpu {
            backends.push(Some(gpu));
        }
        for gpu in backends {
            let mut poser = Poser::new(gpu, &school, &theme, 2.0);
            for heading in [1.0f32, -1.0] {
                let (x, y, len) = (160.0, 95.0, school.fish[0].len);
                let spine = std::array::from_fn(|j| (x + heading * len * (0.5 - j as f32 / (JOINTS - 1) as f32), y));
                let pose = Pose { x, y, len, phase: 0.0, energy: 0.2, depth: 0.0, joy: 0.0, spine };
                let (left, top, right, bottom) = poser.bounds(0, &pose);
                let (w, h) = ((right - left + 1) as usize, (bottom - top + 1) as usize);
                let rgba = poser.pose(0, &pose, left as f32, top as f32, w, h);
                // Down the middle columns of the body, the first and last solid pixel.
                let (mut upper, mut lower) = (0u32, 0u32);
                for col in w * 2 / 5..w * 3 / 5 {
                    let solid: Vec<usize> = (0..h).filter(|&row| rgba[(row * w + col) * 4 + 3] > 200).collect();
                    let brightness = |row: usize| (0..3).map(|ch| u32::from(rgba[(row * w + col) * 4 + ch])).sum::<u32>();
                    if let (Some(&first), Some(&last)) = (solid.first(), solid.last()) {
                        (upper, lower) = (upper + brightness(first), lower + brightness(last));
                    }
                }
                let backend = if gpu.is_some() { "gpu" } else { "cpu" };
                assert!(upper * 4 > lower * 5, "{backend}, heading {heading}: upper edge {upper} should be lighter than lower edge {lower}");
            }
        }
    }

    /// A koi drifting 0.1 sprite pixels per frame, placed on whole pixels as the binary's
    /// layers.rs does, moves its intensity-weighted centroid by about 0.1 every frame, with no stalls or jumps.
    #[test]
    fn subpixel_motion_is_even() {
        let theme = koi_theme::Catalog::load(None).0.resolve(koi_theme::ROOT).expect("root theme");
        let school = School::new(320, 190, 1, 3);
        let scale = 2.0;
        // Only a chosen Vulkan driver (lavapipe in CI) is used, and then it must be there.
        let gpu = std::env::var_os("VK_ICD_FILENAMES").map(|_| Gpu::new().expect("VK_ICD_FILENAMES is set, so an adapter must be found"));
        let mut backends = vec![None];
        if let Some(gpu) = &gpu {
            backends.push(Some(gpu));
        }
        for gpu in backends {
            let mut poser = Poser::new(gpu, &school, &theme, scale);
            let start = school.fish[0].pose();
            let mut last: Option<f32> = None;
            for frame in 0..30 {
                let dx = frame as f32 * 0.1 / scale;
                let pose = Pose { x: start.x + dx, spine: start.spine.map(|(x, y)| (x + dx, y)), ..start };
                let (cx, cy) = (pose.x * scale, pose.y * scale);
                let (left, top, right, bottom) = poser.bounds(0, &pose);
                let (x, y) = ((cx + left as f32).round(), (cy + top as f32).round());
                let (w, h) = ((right - left + 1) as usize, (bottom - top + 1) as usize);
                let rgba = poser.pose(0, &pose, x - cx, y - cy, w, h);
                let (mut sum, mut moment) = (0.0f32, 0.0f32);
                for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
                    let weight = f32::from(px[3]) * (f32::from(px[0]) + f32::from(px[1]) + f32::from(px[2]));
                    sum += weight;
                    moment += weight * (x + (i % w) as f32);
                }
                let centroid = moment / sum;
                if let Some(last) = last {
                    let step = centroid - last;
                    assert!((0.07..=0.13).contains(&step), "frame {frame} ({}): centroid moved {step:.3} px", if gpu.is_some() { "gpu" } else { "cpu" });
                }
                last = Some(centroid);
            }
        }
    }
}

