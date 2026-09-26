use crate::gpu::Gpu;
use bytemuck::{Pod, Zeroable};
use koi_sim::{DT, Shadow, Splash};
use koi_theme::{Bank, Corner, Dither, Floor, Flower, FoliageKind, HighlightStyle, Light, Outline, Palette, PaletteLock, PetalKind, Rgb, Rim, Style, Theme, Weather, linear};
use std::f32::consts::{PI, TAU};

const DAMP: f32 = 0.993;
const MAX_SPLASHES: usize = 64;
const MAX_SHADOWS: usize = 32;
/// The most petals a theme may have (`scene.petals`); the GPU petal buffer holds this many.
const PETALS: usize = 24;
/// The most fireflies; they share the petal buffer, after the petals.
const FIREFLIES: usize = 16;
/// Entries in a palette lock table: 5 bits per sRGB channel.
pub(crate) const LOCK_SIZE: usize = 32 * 32 * 32;
/// Side of the tiling texture that holds clouds, leaf dapple and caustics.
const TEX: usize = 256;

/// The water layer: a coarse height field (a few pixels per cell) that the terminal scales
/// up, shaded as a painted pond. `ratio` is water pixels per pixel of a reference pond at 8
/// pixels per cell; wave speed, refraction and slope are rescaled by it so ripples look the same.
///
/// The simulation has its own grid. `splash` and `render` take its units and scale them by
/// `per_sim` (water pixels per simulation unit, across and down), so a pixel theme can
/// render on its art grid while the koi swim on, and keep, the same simulation grid.
///
/// Everything that does not move (floor, depth, stones, lily pads, foliage, paper grain) is
/// painted once per theme on the CPU into `statics` and shared by both backends, so the GPU
/// and CPU paths only differ in the per-frame shading, which is written twice (here and
/// water.wgsl). Every `[light]` and `[style]` number the shading uses travels in `Params`,
/// which the CPU path reads too, so both backends shade from the same values.
pub struct Water {
    /// Width in water pixels.
    pub w: usize,
    /// Height in water pixels.
    pub h: usize,
    wave: f32,
    refract: f32,
    slope: f32,
    unit: f32,
    per_sim: [f32; 2],
    seed: u64,
    colors: [[f32; 4]; 6],
    light: Light,
    style: Style,
    weather: Weather,
    weather_amount: f32,
    /// Firefly colour, linear.
    firefly: [f32; 3],
    /// The palette lock table, empty when the theme does not lock the water.
    lock: Vec<u32>,
    petals: Vec<Petal>,
    ticks: u64,
    rng: u64,
    splashes: Vec<Splash>,
    rgba: Vec<u8>,
    backend: Backend,
}

enum Backend {
    Gpu(Box<GpuWater>),
    Cpu(CpuWater),
}

struct CpuWater {
    height: Vec<f32>,
    prev_height: Vec<f32>,
    statics: Vec<[f32; 4]>,
    koi_shade: Vec<f32>,
    encode: Vec<u8>,
}

struct GpuWater {
    gpu: Gpu,
    wave: wgpu::ComputePipeline,
    shade: wgpu::ComputePipeline,
    bind_groups: [wgpu::BindGroup; 2],
    current: usize,
    params: wgpu::Buffer,
    heights: [wgpu::Buffer; 2],
    statics: wgpu::Buffer,
    splashes: wgpu::Buffer,
    shadows: wgpu::Buffer,
    petals: wgpu::Buffer,
    lock: wgpu::Buffer,
    out: wgpu::Buffer,
    readback: wgpu::Buffer,
}

/// A petal or small leaf drifting on the surface. Its place is a function of time only.
struct Petal {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    angle: f32,
    spin: f32,
    size: f32,
    color: [f32; 3],
    kind: PetalKind,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    size: [u32; 2],
    splashes: u32,
    shadows: u32,
    petals: u32,
    tex: u32,
    wave: f32,
    refract: f32,
    slope: f32,
    unit: f32,
    pad: [f32; 2],
    cloud_off: [f32; 2],
    dapple_off: [f32; 2],
    caustic_a: [f32; 2],
    caustic_b: [f32; 2],
    scales: [f32; 4],
    /// deep, mid, shallow, sun, shade, cloud; linear light.
    colors: [[f32; 4]; 6],
    /// Towards the sun, unit length.
    sun_dir: [f32; 2],
    ambient: f32,
    shadow_len: f32,
    diffuse: f32,
    tone_steps: f32,
    band_softness: f32,
    grain: f32,
    caustics: f32,
    caustic_softness: f32,
    glint: f32,
    glint_threshold: f32,
    bloom: f32,
    cloud_reflections: f32,
    leaf_shadows: f32,
    wash: f32,
    /// Linear light; the fourth channel is unused.
    wash_top: [f32; 4],
    wash_bottom: [f32; 4],
    /// Fireflies in the petal buffer after the petals.
    fireflies: u32,
    /// 1 when the water snaps to the palette lock table.
    lock: u32,
    /// 1 for `highlight_style = "dashes"`.
    dashes: u32,
    /// 1 on the pixel art grid: petals have hard edges.
    crisp: u32,
    /// Mist strength toward the edges, 0 without mist.
    mist: f32,
    pad2: f32,
    mist_off: [f32; 2],
}

fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = x.cast_unsigned().wrapping_mul(0x8da6_b343) ^ y.cast_unsigned().wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth value noise with one lattice cell per unit, repeating every `period` cells.
fn noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (x - xf, y - yf);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (x0, y0) = ((xf as i32).rem_euclid(period), (yf as i32).rem_euclid(period));
    let (x1, y1) = ((x0 + 1) % period, (y0 + 1) % period);
    let top = mix(hash(x0, y0, seed), hash(x1, y0, seed), sx);
    let bottom = mix(hash(x0, y1, seed), hash(x1, y1, seed), sx);
    mix(top, bottom, sy)
}

/// Four octaves of `noise`, in 0..1, still repeating every `period` cells of the first octave.
fn fbm(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let mut sum = 0.0;
    for k in 0..4 {
        let f = (1 << k) as f32;
        sum += noise(x * f, y * f, period << k, seed + k) / f;
    }
    sum / 1.875
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft posterize: `n` flat tones across 0..1, blending over `soft` of a tone around each
/// edge. 0 is a hard edge. water.wgsl has the same.
pub(crate) fn steps(x: f32, n: f32, soft: f32) -> f32 {
    let v = x * n;
    let f = v - v.floor();
    let edge = if soft > 0.0 { smoothstep(0.5 - soft / 2.0, 0.5 + soft / 2.0, f) } else if f >= 0.5 { 1.0 } else { 0.0 };
    (v.floor() + edge) / n
}

/// Linear 0..1 to an sRGB byte, computed exactly as water.wgsl and koi.wgsl do before a
/// palette lock lookup, so both backends pick the same table entry.
pub(crate) fn srgb_byte(c: f32) -> u32 {
    let x = c.clamp(0.0, 1.0);
    let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u32
}

/// The palette lock table for `palette`: every sRGB colour at 5 bits per channel, entry
/// `(r >> 3) << 10 | (g >> 3) << 5 | b >> 3`, mapped to its nearest swatch in OKLab and packed
/// as opaque RGBA bytes, red lowest. Built once per theme; each backend then snaps a pixel
/// with one lookup, and both snap alike. With no `swatches`, every slot colour is a swatch.
pub(crate) fn lock_table(palette: &Palette) -> Vec<u32> {
    let p = palette;
    let slots = [
        p.deep, p.mid, p.shallow, p.highlight, p.shadow, p.stone_light, p.stone_dark, p.lily_dark, p.lily_light, p.lily_flower, p.koi_white, p.koi_red, p.koi_sumi, p.ogon, p.outline, p.cloud,
        p.asagi_blue, p.asagi_red, p.food, p.ui_text, p.ui_dim, p.ui_accent,
    ];
    let swatches = if p.swatches.is_empty() { &slots[..] } else { &p.swatches[..] };
    let oklab = |[r, g, b]: [f32; 3]| {
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        [0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s, 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s, 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s]
    };
    let labs: Vec<([f32; 3], u32)> = swatches.iter().map(|&[r, g, b]| (oklab(linear([r, g, b])), u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16 | 255 << 24)).collect();
    (0..LOCK_SIZE)
        .map(|i| {
            let centre = [i >> 10, (i >> 5) & 31, i & 31].map(|v| u8::try_from(v * 8 + 4).expect("bin centre fits a byte"));
            let lab = oklab(linear(centre));
            labs.iter().map(|(s, packed)| ((s[0] - lab[0]).powi(2) + (s[1] - lab[1]).powi(2) + (s[2] - lab[2]).powi(2), *packed)).fold((f32::MAX, 0), |best, c| if c.0 < best.0 { c } else { best }).1
        })
        .collect()
}

/// Paints `color` at `alpha` over the premultiplied `under`.
fn over(under: [f32; 4], color: [f32; 3], alpha: f32) -> [f32; 4] {
    let keep = 1.0 - alpha;
    [color[0] * alpha + under[0] * keep, color[1] * alpha + under[1] * keep, color[2] * alpha + under[2] * keep, alpha + under[3] * keep]
}

fn scale3(c: [f32; 3], k: f32) -> [f32; 3] {
    c.map(|v| v * k)
}

/// How much of a koi's shadow covers `(x, y)`; the shadow falls down and right of the fish,
/// `len` times the default offset. A diving koi is nearer the floor, so its shadow is closer
/// under it and sharper. water.wgsl has the same.
fn koi_shadow(x: f32, y: f32, s: &Shadow, len: f32) -> f32 {
    let (l, lift) = (s.len, 1.0 - s.depth);
    let (dx, dy) = (x - s.x - l * 0.1 * len * lift, y - s.y - l * 0.15 * len * lift);
    let su = (dx * s.dir_x + dy * s.dir_y) / l;
    let sv = (-dx * s.dir_y + dy * s.dir_x) / l;
    let width = 0.14 * (0.45 + 0.55 * smoothstep(-0.5, 0.1, su));
    let r = ((su - 0.1) / 0.45).powi(2) + (sv / width).powi(2);
    1.0 - smoothstep(0.3 + 0.4 * s.depth, 1.0, r)
}

impl Water {
    /// A still `w` x `h` pond, its banks, stones and lily pads laid out by `seed` and painted
    /// in `theme`. `per_sim` is water pixels per simulation unit, across and down. With a
    /// `gpu`, the height field and shading live there.
    pub fn new(gpu: Option<&Gpu>, w: usize, h: usize, per_sim: [f32; 2], ratio: f32, theme: &Theme, seed: u64) -> Water {
        let backend = match gpu {
            Some(gpu) => Backend::Gpu(Box::new(GpuWater::new(gpu, w, h))),
            None => Backend::Cpu(CpuWater { height: vec![0.0; w * h], prev_height: vec![0.0; w * h], statics: Vec::new(), koi_shade: vec![0.0; w * h], encode: srgb_table() }),
        };
        let mut water = Water {
            w,
            h,
            wave: 0.5 * ratio * ratio,
            refract: 6.0 * ratio * ratio,
            slope: 4.0 * ratio,
            unit: w.min(h) as f32,
            per_sim,
            seed,
            colors: [[0.0; 4]; 6],
            light: theme.light,
            style: theme.style,
            weather: theme.scene.weather,
            weather_amount: theme.scene.weather_amount,
            firefly: [0.0; 3],
            lock: Vec::new(),
            petals: Vec::new(),
            ticks: 0,
            rng: seed | 1,
            splashes: Vec::new(),
            rgba: vec![255; w * h * 4],
            backend,
        };
        water.set_theme(theme);
        water
    }

    /// Repaints the pond in `theme` with the same layout. The waves, the time and the
    /// petals' places carry on.
    pub fn set_theme(&mut self, theme: &Theme) {
        let palette = &theme.palette;
        let shadow = linear(palette.shadow);
        let brightest = shadow.iter().copied().fold(0.001, f32::max);
        let shade = shadow.map(|c| c / brightest);
        self.colors = [linear(palette.deep), linear(palette.mid), linear(palette.shallow), linear(palette.highlight), shade, linear(palette.cloud)].map(|[r, g, b]| [r, g, b, 0.0]);
        (self.light, self.style) = (theme.light, theme.style);
        (self.weather, self.weather_amount) = (theme.scene.weather, theme.scene.weather_amount);
        self.firefly = mix3(linear(palette.ogon), linear(palette.highlight), 0.5);
        self.lock = if theme.style.palette_lock == PaletteLock::Off { Vec::new() } else { lock_table(palette) };

        let (w, h, unit) = (self.w, self.h, self.unit);
        let mut rng = self.seed ^ 0x9e37_79b9_7f4a_7c15 | 1;
        let drift = rand(&mut rng) * TAU;
        let kinds = &theme.scene.petal_kinds;
        // Every petal draws the same random numbers whatever its kind, so petal k drifts the
        // same way in every theme.
        let count = if kinds.is_empty() { 0 } else { (theme.scene.petals as usize).min(PETALS) };
        self.petals = (0..count)
            .map(|k| {
                let speed = unit * (0.008 + 0.006 * rand(&mut rng));
                let heading = drift + (rand(&mut rng) - 0.5) * 0.8;
                let kind = kinds[k % kinds.len()];
                let color = match kind {
                    PetalKind::Blossom => linear(palette.lily_flower),
                    PetalKind::White => linear(palette.koi_white),
                    PetalKind::Leaf => mix3(linear(palette.lily_light), linear(palette.ogon), 0.35),
                    PetalKind::Maple => mix3(linear(palette.lily_flower), linear(palette.ogon), 0.25),
                };
                let (x, y, angle, spin, grow) = (rand(&mut rng), rand(&mut rng), rand(&mut rng), rand(&mut rng), rand(&mut rng));
                Petal {
                    x: w as f32 * x,
                    y: h as f32 * y,
                    vx: heading.cos() * speed,
                    vy: heading.sin() * speed,
                    angle: angle * TAU,
                    spin: (spin - 0.5) * 0.12,
                    size: unit * if matches!(kind, PetalKind::Leaf | PetalKind::Maple) { 0.022 } else { 0.014 + 0.005 * grow },
                    color,
                    kind,
                }
            })
            .collect();

        let statics = paint(w, h, theme, self.seed);
        match &mut self.backend {
            Backend::Gpu(g) => {
                g.gpu.queue.write_buffer(&g.statics, 0, bytemuck::cast_slice(&statics));
                if !self.lock.is_empty() {
                    g.gpu.queue.write_buffer(&g.lock, 0, bytemuck::cast_slice(&self.lock));
                }
            }
            Backend::Cpu(c) => c.statics = statics,
        }
    }

    /// Moves the pond to a `w` x `h` grid with `per_sim` and `ratio` as in `new`, and paints it
    /// in `theme`. The waves are resampled onto the new grid, so a switch between a painted
    /// and a pixel theme keeps them.
    pub fn regrid(&mut self, w: usize, h: usize, per_sim: [f32; 2], ratio: f32, theme: &Theme) {
        let (ow, oh) = (self.w, self.h);
        let (gpu, old) = match &self.backend {
            Backend::Gpu(g) => {
                let bytes = ow * oh * 4;
                let read = |buffer: &wgpu::Buffer| {
                    let mut raw = vec![0u8; bytes];
                    g.gpu.finish_into(g.gpu.device.create_command_encoder(&Default::default()), buffer, &g.readback, &mut raw);
                    bytemuck::cast_slice::<u8, f32>(&raw).to_vec()
                };
                (Some(g.gpu.clone()), [read(&g.heights[g.current]), read(&g.heights[1 - g.current])])
            }
            Backend::Cpu(c) => (None, [c.height.clone(), c.prev_height.clone()]),
        };
        let resample = |from: &[f32]| -> Vec<f32> {
            (0..w * h)
                .map(|i| {
                    let x = (((i % w) as f32 + 0.5) * ow as f32 / w as f32 - 0.5).clamp(0.0, (ow - 1) as f32);
                    let y = (((i / w) as f32 + 0.5) * oh as f32 / h as f32 - 0.5).clamp(0.0, (oh - 1) as f32);
                    let (x0, y0) = (x as usize, y as usize);
                    let (x1, y1) = ((x0 + 1).min(ow - 1), (y0 + 1).min(oh - 1));
                    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
                    mix(mix(from[y0 * ow + x0], from[y0 * ow + x1], fx), mix(from[y1 * ow + x0], from[y1 * ow + x1], fx), fy)
                })
                .collect()
        };
        let [height, prev_height] = old.map(|heights| resample(&heights));
        let mut fresh = Water::new(gpu.as_ref(), w, h, per_sim, ratio, theme, self.seed);
        (fresh.ticks, fresh.rng) = (self.ticks, self.rng);
        match &mut fresh.backend {
            Backend::Gpu(g) => {
                g.gpu.queue.write_buffer(&g.heights[g.current], 0, bytemuck::cast_slice(&height));
                g.gpu.queue.write_buffer(&g.heights[1 - g.current], 0, bytemuck::cast_slice(&prev_height));
            }
            Backend::Cpu(c) => (c.height, c.prev_height) = (height, prev_height),
        }
        *self = fresh;
    }

    /// Queues a splash, in simulation units, for the next `step`.
    pub fn splash(&mut self, splash: Splash) {
        let [kx, ky] = self.per_sim;
        self.splashes.push(Splash { x: splash.x * kx, y: splash.y * ky, radius: splash.radius * kx, amount: splash.amount });
    }

    /// Advances the surface by one DT, applying the splashes queued since the last step.
    pub fn step(&mut self) {
        self.ticks += 1;
        if rand(&mut self.rng) < DT / 4.0 {
            let x = self.w as f32 * (0.15 + 0.7 * rand(&mut self.rng));
            let y = self.h as f32 * (0.15 + 0.7 * rand(&mut self.rng));
            self.splashes.push(Splash { x, y, radius: self.unit * 0.015, amount: 0.4 });
        }
        // Raindrops: small real ripples anywhere on the water, about 11 a second at full rain.
        if self.weather == Weather::Rain && rand(&mut self.rng) < DT * 11.0 * self.weather_amount {
            let (x, y) = (self.w as f32 * rand(&mut self.rng), self.h as f32 * rand(&mut self.rng));
            self.splashes.push(Splash { x, y, radius: self.unit * 0.012, amount: 0.3 });
        }
        let params = self.params(self.splashes.len().min(MAX_SPLASHES), 0);
        match &mut self.backend {
            Backend::Gpu(g) => {
                let splashes: Vec<[f32; 4]> = self.splashes.iter().take(MAX_SPLASHES).map(|s| [s.x, s.y, s.radius, s.amount]).collect();
                g.gpu.queue.write_buffer(&g.params, 0, bytemuck::bytes_of(&params));
                if !splashes.is_empty() {
                    g.gpu.queue.write_buffer(&g.splashes, 0, bytemuck::cast_slice(&splashes));
                }
                let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&g.wave);
                    pass.set_bind_group(0, &g.bind_groups[g.current], &[]);
                    pass.dispatch_workgroups(params.size[0].div_ceil(16), params.size[1].div_ceil(16), 1);
                }
                g.gpu.queue.submit([encoder.finish()]);
                g.current = 1 - g.current;
            }
            Backend::Cpu(c) => {
                let (w, h) = (self.w, self.h);
                let n = w * h;
                for y in 1..h - 1 {
                    for x in 1..w - 1 {
                        let i = y * w + x;
                        let sum = c.height[i - 1] + c.height[i + 1] + c.height[i - w] + c.height[i + w];
                        let hh = c.height[i];
                        let v = (2.0 * hh - c.prev_height[i] + self.wave * (sum - 4.0 * hh)) * c.statics[n + i][3];
                        c.prev_height[i] = if v.abs() < 1e-4 { 0.0 } else { v };
                    }
                }
                // Like water.wgsl: splashes land on the new heights, and only on open water.
                for s in &self.splashes {
                    let r = s.radius.ceil() as i32;
                    for py in (s.y as i32 - r).max(1)..=(s.y as i32 + r).min(h as i32 - 2) {
                        for px in (s.x as i32 - r).max(1)..=(s.x as i32 + r).min(w as i32 - 2) {
                            let i = py as usize * w + px as usize;
                            let d = ((px as f32 - s.x).powi(2) + (py as f32 - s.y).powi(2)).sqrt();
                            if d < s.radius && c.statics[n + i][3] > 0.0 {
                                c.prev_height[i] += s.amount * 0.5 * (1.0 + (PI * d / s.radius).cos());
                            }
                        }
                    }
                }
                std::mem::swap(&mut c.height, &mut c.prev_height);
            }
        }
        self.splashes.clear();
    }

    /// The painted pond with koi shadows (in simulation units), ripples, clouds, petals and
    /// weather, as straight RGBA `w` x `h`.
    pub fn render(&mut self, shadows: &[Shadow]) -> &[u8] {
        let [kx, ky] = self.per_sim;
        let shadows: Vec<Shadow> = shadows.iter().take(MAX_SHADOWS).map(|s| Shadow { x: s.x * kx, y: s.y * ky, len: s.len * kx, ..*s }).collect();
        let hz = f64::from(self.style.anim_hz);
        let t = (self.ticks as f64 * f64::from(DT) * hz).floor() / hz;
        let u = f64::from(self.unit);
        let caustic = f64::from(self.style.caustic_scale);
        let scales = [TEX as f64 / (2.4 * u), TEX as f64 / (1.1 * u), TEX as f64 / (0.62 * u * caustic), TEX as f64 / (0.5 * u * caustic)];
        let wrap = |px: f64, py: f64, scale: f64| [(px * scale).rem_euclid(TEX as f64) as f32, (py * scale).rem_euclid(TEX as f64) as f32];
        let offsets = [
            wrap(t * 0.012 * u, t * 0.004 * u, scales[0]),
            wrap(((t * 0.35).sin() * 0.012 + (t * 0.13).sin() * 0.01) * u, (t * 0.29).cos() * 0.01 * u, scales[1]),
            wrap(t * 0.010 * u, t * 0.006 * u, scales[2]),
            wrap(-t * 0.007 * u, t * 0.009 * u, scales[3]),
        ];
        let petals: Vec<[f32; 8]> = self
            .petals
            .iter()
            .map(|p| {
                let margin = f64::from(p.size) * 2.0;
                let x = (f64::from(p.x) + f64::from(p.vx) * t).rem_euclid(self.w as f64 + 2.0 * margin) - margin;
                let y = (f64::from(p.y) + f64::from(p.vy) * t).rem_euclid(self.h as f64 + 2.0 * margin) - margin;
                let angle = (f64::from(p.angle) + f64::from(p.spin) * t).rem_euclid(std::f64::consts::TAU);
                let [r, g, b] = p.color;
                let shape = match p.kind {
                    PetalKind::Leaf => 1.0,
                    PetalKind::Maple => 2.0,
                    PetalKind::Blossom | PetalKind::White => 0.0,
                };
                [x as f32, y as f32, angle as f32, p.size, r, g, b, shape]
            })
            .collect();
        // Fireflies follow the petals in the same buffer: (x, y, glow, reach) and their colour.
        // Each hovers near its own spot, wandering a little and pulsing.
        let fireflies: Vec<[f32; 8]> = if self.weather == Weather::Fireflies {
            let s = (self.seed ^ (self.seed >> 32)) as u32;
            let [r, g, b] = self.firefly;
            let count = ((14.0 * self.weather_amount).round() as usize).min(FIREFLIES);
            (0..count)
                .map(|k| {
                    let kf = k as f64;
                    let x = (f64::from(hash(k as i32, 1, s + 80)) + 0.02 * (t * 0.7 + kf).sin()) * self.w as f64;
                    let y = (f64::from(hash(k as i32, 2, s + 80)) + 0.02 * (t * 0.5 + 2.0 * kf).cos()) * self.h as f64;
                    let glow = 0.5 + 0.5 * (t * 1.3 + 1.7 * kf).sin();
                    [x as f32, y as f32, glow as f32, self.unit * 0.03, r, g, b, 3.0]
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut params = self.params(0, shadows.len());
        [params.cloud_off, params.dapple_off, params.caustic_a, params.caustic_b] = offsets;
        params.mist_off = wrap(t * 0.004 * u, -t * 0.002 * u, scales[0]);
        params.scales = scales.map(|s| s as f32);
        params.petals = u32::try_from(petals.len()).expect("petal count fits u32");
        params.fireflies = u32::try_from(fireflies.len()).expect("firefly count fits u32");
        let items: Vec<[f32; 8]> = petals.iter().chain(&fireflies).copied().collect();

        match &mut self.backend {
            Backend::Gpu(g) => {
                let data: Vec<[f32; 8]> = shadows.iter().map(|s| [s.x, s.y, s.dir_x, s.dir_y, s.len, s.depth, 0.0, 0.0]).collect();
                g.gpu.queue.write_buffer(&g.params, 0, bytemuck::bytes_of(&params));
                if !data.is_empty() {
                    g.gpu.queue.write_buffer(&g.shadows, 0, bytemuck::cast_slice(&data));
                }
                if !items.is_empty() {
                    g.gpu.queue.write_buffer(&g.petals, 0, bytemuck::cast_slice(&items));
                }
                let mut encoder = g.gpu.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&g.shade);
                    pass.set_bind_group(0, &g.bind_groups[g.current], &[]);
                    pass.dispatch_workgroups(params.size[0].div_ceil(16), params.size[1].div_ceil(16), 1);
                }
                g.gpu.finish_into(encoder, &g.out, &g.readback, &mut self.rgba);
            }
            Backend::Cpu(c) => {
                let (w, h) = (self.w, self.h);
                let n = w * h;
                c.koi_shade.fill(0.0);
                for s in &shadows {
                    let lift = (1.0 - s.depth) * params.shadow_len;
                    let (cx, cy, reach) = (s.x + s.len * 0.1 * lift, s.y + s.len * 0.15 * lift, s.len * 0.6);
                    for py in (cy - reach).max(0.0) as usize..=((cy + reach) as usize).min(h - 1) {
                        for px in (cx - reach).max(0.0) as usize..=((cx + reach) as usize).min(w - 1) {
                            let k = &mut c.koi_shade[py * w + px];
                            *k = k.max(koi_shadow(px as f32, py as f32, s, params.shadow_len));
                        }
                    }
                }

                let statics = &c.statics;
                let bilinear = |base: usize, x: f32, y: f32| -> [f32; 4] {
                    let (x0, y0) = (x.floor(), y.floor());
                    let (fx, fy) = (x - x0, y - y0);
                    let (x0, y0) = (x0 as usize, y0 as usize);
                    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                    let (a, b, cc, d) = (statics[base + y0 * w + x0], statics[base + y0 * w + x1], statics[base + y1 * w + x0], statics[base + y1 * w + x1]);
                    std::array::from_fn(|k| mix(mix(a[k], b[k], fx), mix(cc[k], d[k], fx), fy))
                };
                let tex = |channel: usize, x: f32, y: f32| -> f32 {
                    let (x0, y0) = (x.floor(), y.floor());
                    let (fx, fy) = (x - x0, y - y0);
                    let (x0, y0) = ((x0 as i32).rem_euclid(TEX as i32) as usize, (y0 as i32).rem_euclid(TEX as i32) as usize);
                    let (x1, y1) = ((x0 + 1) % TEX, (y0 + 1) % TEX);
                    let at = |x: usize, y: usize| statics[3 * n + y * TEX + x][channel];
                    mix(mix(at(x0, y0), at(x1, y0), fx), mix(at(x0, y1), at(x1, y1), fx), fy)
                };
                let [deep, mid, shallow, sun_col, shade, cloud_col] = self.colors.map(|[r, g, b, _]| [r, g, b]);
                let p = &params;
                let (wash_top, wash_bottom) = ([p.wash_top[0], p.wash_top[1], p.wash_top[2]], [p.wash_bottom[0], p.wash_bottom[1], p.wash_bottom[2]]);
                let [cloud_off, dapple_off, caus_a, caus_b] = offsets;
                let [s_cloud, s_dapple, s_a, s_b] = params.scales;
                let height = &c.height;

                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        let (px, py) = (x as f32, y as f32);
                        let surf = statics[2 * n + i];
                        let here = statics[n + i];
                        let mut col = [surf[0], surf[1], surf[2]];
                        if surf[3] < 0.999 {
                            let (mut gx, mut gy, mut jac) = (0.0, 0.0, 1.0);
                            if x > 0 && y > 0 && x < w - 1 && y < h - 1 {
                                let (sx, sy) = (height[i + 1] - height[i - 1], height[i + w] - height[i - w]);
                                if sx.abs() + sy.abs() >= 0.003 {
                                    let hc = height[i];
                                    let d = self.refract * 3.0;
                                    let hxx = height[i + 1] + height[i - 1] - 2.0 * hc;
                                    let hyy = height[i + w] + height[i - w] - 2.0 * hc;
                                    let hxy = (height[i + w + 1] - height[i + w - 1] - height[i - w + 1] + height[i - w - 1]) * 0.25;
                                    (gx, gy, jac) = (sx, sy, (1.0 + d * hxx) * (1.0 + d * hyy) - (d * hxy).powi(2));
                                }
                            }
                            let depth = statics[i][3];
                            let bend = self.refract * (0.5 + depth);
                            let (srcx, srcy) = ((px + gx * bend).clamp(0.0, (w - 1) as f32), (py + gy * bend).clamp(0.0, (h - 1) as f32));
                            let bed = bilinear(0, srcx, srcy);
                            let ex = bilinear(n, srcx, srcy);
                            let dapple = tex(1, srcx * s_dapple + dapple_off[0], srcy * s_dapple + dapple_off[1]);
                            let open = ex[0] * (1.0 - p.leaf_shadows * ex[1] * (1.0 - dapple));
                            let koi = c.koi_shade[(srcy + 0.5) as usize * w + (srcx + 0.5) as usize] * (1.0 - p.diffuse);
                            let sun = open * (1.0 - 0.6 * koi);
                            let cells = tex(2, srcx * s_a + caus_a[0], srcy * s_a + caus_a[1]).min(tex(2, srcx * s_b + caus_b[0], srcy * s_b + caus_b[1]));
                            let lines = (1.0 - smoothstep(0.0, p.caustic_softness, cells)) * p.caustics * (1.0 - p.diffuse);
                            let focus = (1.0 / jac.abs().max(0.3)).clamp(0.5, 2.2);
                            let lit = steps((sun * focus / 1.4).clamp(0.0, 1.0), p.tone_steps, p.band_softness) * 1.4 * (1.0 + 0.35 * lines * sun);
                            let clear = shallow.iter().copied().fold(0.001, f32::max);
                            let floor: [f32; 3] = std::array::from_fn(|k| bed[k] * mix(1.0, shallow[k] / clear, 0.45) * (shade[k] * 0.4 + sun_col[k] * lit * 0.8));
                            let water = if depth < 0.5 { mix3(shallow, mid, depth * 2.0) } else { mix3(mid, deep, depth * 2.0 - 1.0) };
                            col = mix3(floor, scale3(water, (0.7 + 0.4 * open) * (1.0 - 0.3 * koi)), 0.15 + 0.72 * depth);

                            let tilt = -(gx * p.sun_dir[0] + gy * p.sun_dir[1]) * self.slope;
                            col = scale3(mix3(col, sun_col, 0.25 * smoothstep(0.15, 0.4, tilt)), 1.0 - 0.15 * smoothstep(0.12, 0.3, -tilt));
                            let cloud = tex(0, px * s_cloud + cloud_off[0] + gx * self.slope * 3.0, py * s_cloud + cloud_off[1] + gy * self.slope * 3.0);
                            col = mix3(col, cloud_col, p.cloud_reflections * cloud * (1.0 - 0.5 * here[1]));
                            if p.dashes == 1 {
                                // Rows are cut into runs of 4 pixels, each lit 2 to 4 long
                                // where the crest at its start tilts past the threshold.
                                let (row, offset) = (y as i32, (hash(0, y as i32, 61) * 4.0) as i32);
                                let run = (x as i32 + offset) / 4;
                                let long = 2 + (hash(run, row, 62) * 3.0) as i32;
                                let ax = (run * 4 - offset).clamp(1, w as i32 - 2) as usize;
                                if (x as i32 + offset) % 4 < long && y > 0 && y < h - 1 {
                                    let a = y * w + ax;
                                    let dash = -((height[a + 1] - height[a - 1]) * p.sun_dir[0] + (height[a + w] - height[a - w]) * p.sun_dir[1]) * self.slope;
                                    if dash > p.glint_threshold {
                                        col = mix3(col, sun_col, p.glint.min(1.0));
                                    }
                                }
                            } else {
                                let glint = (smoothstep(p.glint_threshold, 1.0, tilt) * 0.6 + smoothstep(0.4, 1.0, tilt) * p.bloom) * p.glint;
                                col = std::array::from_fn(|k| col[k] + sun_col[k] * glint);
                            }

                            for p in &items[..petals.len()] {
                                let (dx, dy) = (px - p[0], py - p[1]);
                                if dx.abs() > p[3] * 1.5 || dy.abs() > p[3] * 1.5 {
                                    continue;
                                }
                                let (ca, sa) = (p[2].cos(), p[2].sin());
                                let lu = (dx * ca + dy * sa) / p[3];
                                let lv = (-dx * sa + dy * ca) / p[3];
                                let width = if p[7] > 1.5 {
                                    0.8 * (0.6 + 0.4 * (lv.atan2(lu) * 2.5).cos().abs()) * (1.0 - lu * lu).max(0.0).sqrt()
                                } else if p[7] > 0.5 {
                                    0.38 * (1.0 - lu * lu).max(0.0)
                                } else {
                                    0.55 * (1.0 - lu * lu).max(0.0).sqrt() * (0.7 + 0.3 * lu)
                                };
                                let mut alpha = 1.0 - smoothstep(0.85, 1.0, lu.abs().max(lv.abs() / width.max(0.01)));
                                if params.crisp == 1 {
                                    alpha = if alpha > 0.5 { 1.0 } else { 0.0 };
                                }
                                let tone = if lv > 0.0 { 0.85 } else { 1.0 } * (0.7 + 0.3 * open);
                                col = mix3(col, scale3([p[4], p[5], p[6]], tone), alpha);
                            }
                            col = std::array::from_fn(|k| col[k] * (1.0 - surf[3]) + surf[k]);
                        }
                        for f in &items[petals.len()..] {
                            let d = (px - f[0]).hypot(py - f[1]) / f[3];
                            if d < 1.0 {
                                col = mix3(col, [f[4], f[5], f[6]], f[2] * if d < 1.0 / 6.0 { 1.0 } else { 0.35 * (1.0 - d) });
                            }
                        }
                        if p.mist > 0.0 {
                            let edge = px.min(py).min((w - 1) as f32 - px).min((h - 1) as f32 - py) / self.unit;
                            let haze = tex(3, px * s_cloud + p.mist_off[0], py * s_cloud + p.mist_off[1]);
                            col = mix3(col, sun_col, p.mist * (0.25 + 0.55 * (1.0 - smoothstep(0.02, 0.35, edge))) * (0.6 + 0.4 * haze));
                        }
                        col = mix3(col, mix3(wash_top, wash_bottom, py / h as f32), p.wash);
                        let light = p.ambient * (1.0 + here[2] * p.grain * 1.5);
                        let pixel = &mut self.rgba[i * 4..i * 4 + 3];
                        if p.lock == 1 {
                            let [r, g, b] = col.map(|v| srgb_byte(v * light) as usize >> 3);
                            pixel.copy_from_slice(&self.lock[r << 10 | g << 5 | b].to_le_bytes()[..3]);
                        } else {
                            for k in 0..3 {
                                pixel[k] = c.encode[((col[k] * light).clamp(0.0, 1.0) * 4095.0 + 0.5) as usize];
                            }
                        }
                    }
                }
            }
        }
        &self.rgba
    }

    fn params(&self, splashes: usize, shadows: usize) -> Params {
        let (light, style) = (&self.light, &self.style);
        let wash = |c: Rgb| {
            let [r, g, b] = linear(c);
            [r, g, b, 0.0]
        };
        Params {
            size: [self.w, self.h].map(|n| u32::try_from(n).expect("water size fits u32")),
            splashes: u32::try_from(splashes).expect("splash count fits u32"),
            shadows: u32::try_from(shadows).expect("shadow count fits u32"),
            petals: 0,
            tex: u32::try_from(TEX).expect("texture size fits u32"),
            wave: self.wave,
            refract: self.refract,
            slope: self.slope,
            unit: self.unit,
            pad: [0.0; 2],
            cloud_off: [0.0; 2],
            dapple_off: [0.0; 2],
            caustic_a: [0.0; 2],
            caustic_b: [0.0; 2],
            scales: [0.0; 4],
            colors: self.colors,
            sun_dir: light.sun,
            ambient: light.ambient,
            shadow_len: light.shadow_len,
            diffuse: light.diffuse,
            tone_steps: f32::from(style.tone_steps),
            band_softness: style.band_softness,
            grain: style.grain,
            caustics: style.caustics,
            caustic_softness: style.caustic_softness,
            glint: style.glint,
            glint_threshold: style.glint_threshold,
            bloom: style.bloom,
            cloud_reflections: style.cloud_reflections,
            leaf_shadows: style.leaf_shadows,
            wash: style.wash,
            wash_top: wash(style.wash_top),
            wash_bottom: wash(style.wash_bottom),
            fireflies: 0,
            lock: u32::from(!self.lock.is_empty()),
            dashes: u32::from(style.highlight_style == HighlightStyle::Dashes),
            crisp: u32::from(style.pixel_px > 0),
            mist: if self.weather == Weather::Mist { self.weather_amount } else { 0.0 },
            pad2: 0.0,
            mist_off: [0.0; 2],
        }
    }
}

/// Linear 0..1 in 4096 steps to sRGB bytes.
fn srgb_table() -> Vec<u8> {
    (0..4096)
        .map(|k| {
            let c = k as f32 / 4095.0;
            let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0 + 0.5) as u8
        })
        .collect()
}

struct Stone {
    x: f32,
    y: f32,
    a: f32,
    b: f32,
    cos: f32,
    sin: f32,
    tint: f32,
}

#[derive(Clone, Copy)]
struct Pad {
    x: f32,
    y: f32,
    r: f32,
    cos: f32,
    sin: f32,
    /// From `lily_dark` (0) to `lily_light` (1).
    shade: f32,
    /// How far the pad yellows toward `ogon`.
    gold: f32,
    /// The pad has a flower when this is below `scene.flowers`.
    bloom: f32,
    /// Picks the lotus colour.
    pick: f32,
}

struct Leaf {
    x: f32,
    y: f32,
    len: f32,
    cos: f32,
    sin: f32,
    tone: f32,
    pick: f32,
}

/// The pond's shape, rim stones and lily pads. It depends only on the size and the seed, so
/// every theme paints the same pond and a switch never moves a stone or a pad. Themes
/// choose what is drawn (planks hide the stones, `pad_size` scales the pads in place).
struct Layout {
    u: f32,
    cx: f32,
    cy: f32,
    inset: f32,
    half: (f32, f32),
    radius: f32,
    s: u32,
    stones: Vec<Stone>,
    pads: Vec<Pad>,
}

impl Layout {
    fn new(w: usize, h: usize, seed: u64) -> Layout {
        let u = w.min(h) as f32;
        let s = (seed ^ (seed >> 32)) as u32;
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let (inset, radius) = (0.07 * u, 0.3 * u);
        let half = ((cx - inset - radius).max(0.0), (cy - inset - radius).max(0.0));
        let mut layout = Layout { u, cx, cy, inset, half, radius, s, stones: Vec::new(), pads: Vec::new() };
        let (mut stones, mut pads) = (Vec::new(), Vec::new());
        let mut rng = seed | 1;

        let shore: Vec<(f32, f32)> = (0..720)
            .map(|k| {
                let (dy, dx) = (k as f32 / 720.0 * TAU).sin_cos();
                // Stride out to the shore, then step back and find it to half a pixel.
                let mut r = 0.0;
                while layout.sdf(cx + dx * r, cy + dy * r) < 0.0 && r < w as f32 {
                    r += 4.0;
                }
                r = (r - 4.0).max(0.0);
                while layout.sdf(cx + dx * r, cy + dy * r) < 0.0 && r < w as f32 {
                    r += 0.5;
                }
                (cx + dx * r, cy + dy * r)
            })
            .collect();

        // Uneven sizes, small clusters and the odd gap, so the rim does not read as beads on
        // a string.
        let (mut travelled, mut next) = (0.0, 0.0);
        for k in 0..shore.len() {
            let (x, y) = shore[k];
            let (px, py) = shore[(k + shore.len() - 1) % shore.len()];
            travelled += (x - px).hypot(y - py);
            if travelled < next {
                continue;
            }
            let size = rand(&mut rng);
            let a = u * (0.026 + 0.06 * size * size);
            let out = u * (0.005 + 0.02 * rand(&mut rng));
            next = travelled + a * (0.9 + 0.5 * rand(&mut rng));
            if rand(&mut rng) < 0.14 {
                next += u * (0.08 + 0.08 * rand(&mut rng));
            }
            let (nx, ny) = ((x - cx) / (x - cx).hypot(y - cy), (y - cy) / (x - cx).hypot(y - cy));
            let along = (y - py).atan2(x - px) + (rand(&mut rng) - 0.5) * 0.5;
            stones.push(Stone { x: x + nx * out, y: y + ny * out, a, b: a * (0.6 + 0.3 * rand(&mut rng)), cos: along.cos(), sin: along.sin(), tint: 0.9 + 0.18 * rand(&mut rng) });
            if rand(&mut rng) < 0.35 {
                let a = u * (0.05 + 0.03 * rand(&mut rng));
                let out = u * 0.07;
                stones.push(Stone { x: x + nx * out, y: y + ny * out, a, b: a * 0.75, cos: along.cos(), sin: along.sin(), tint: 0.85 + 0.15 * rand(&mut rng) });
            }
        }
        // Outer stones first, so the ones on the waterline paint over them.
        stones.sort_by(|p: &Stone, q: &Stone| layout.sdf(q.x, q.y).total_cmp(&layout.sdf(p.x, p.y)));

        let corners = [0.55, 2.55, 3.85, 5.6];
        let skip = (rand(&mut rng) * 4.0) as usize;
        for (c, corner) in corners.iter().enumerate() {
            if c == skip {
                continue;
            }
            let k = ((corner + (rand(&mut rng) - 0.5) * 0.3) / TAU * 720.0) as usize % 720;
            let (sx, sy) = shore[k];
            let (nx, ny) = ((cx - sx) / (cx - sx).hypot(cy - sy), (cy - sy) / (cx - sx).hypot(cy - sy));
            let count = 2 + (rand(&mut rng) * 2.5) as usize;
            let mut tries = 0;
            let first = pads.len();
            while pads.len() - first < count && tries < 40 {
                tries += 1;
                let r = u * (0.045 + 0.03 * rand(&mut rng));
                let inward = u * (0.09 + 0.08 * rand(&mut rng));
                let side = (rand(&mut rng) - 0.5) * u * 0.28;
                let (x, y) = (sx + nx * inward - ny * side, sy + ny * inward + nx * side);
                if layout.sdf(x, y) > -r * 0.6 || pads.iter().any(|p: &Pad| (p.x - x).hypot(p.y - y) < (p.r + r) * 0.85) {
                    continue;
                }
                let angle = rand(&mut rng) * TAU;
                let [shade, gold, bloom, pick] = [0.2 + 0.7 * rand(&mut rng), 0.15 * rand(&mut rng), rand(&mut rng), rand(&mut rng)];
                pads.push(Pad { x, y, r, cos: angle.cos(), sin: angle.sin(), shade, gold, bloom, pick });
            }
        }
        (layout.stones, layout.pads) = (stones, pads);
        layout
    }

    /// Signed distance to the shore: negative in the water.
    fn sdf(&self, x: f32, y: f32) -> f32 {
        let (u, (hx, hy)) = (self.u, self.half);
        let (qx, qy) = ((x - self.cx).abs() - hx, (y - self.cy).abs() - hy);
        (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - self.radius + (fbm(x / u * 3.0, y / u * 3.0, 1 << 16, self.s) - 0.5) * 0.08 * u
    }

    /// Water depth before banding, 0 at the shore to 1 in the deep middle.
    fn depth(&self, x: f32, y: f32) -> f32 {
        let (u, inset) = (self.u, self.inset);
        let oval = ((x - self.cx) / (self.cx - inset)).hypot((y - self.cy) / (self.cy - inset)) + (fbm(x / u * 2.0, y / u * 2.0, 1 << 16, self.s + 2) - 0.5) * 0.3;
        smoothstep(0.0, 0.25 * u, -self.sdf(x, y)) * (1.0 - smoothstep(0.15, 0.95, oval))
    }
}

/// Paints everything that never moves. Returns, per water pixel, three blocks of `w * h`:
/// the floor (linear rgb, depth 0..1), the light (sun reaching the floor, foliage cover,
/// paper grain, wave damping), and the surface layer (premultiplied rgba: bank, rim, pads,
/// flowers, foliage). Then one `TEX * TEX` tile: clouds, leaf dapple, caustic cells, mist.
/// The layout depends only on `w`, `h` and `seed` (see `Layout`); foliage depends on the
/// corners the theme names, each corner drawn the same in every theme that has it.
fn paint(w: usize, h: usize, theme: &Theme, seed: u64) -> Vec<[f32; 4]> {
    let (palette, light, style, scene) = (&theme.palette, &theme.light, &theme.style, &theme.scene);
    let sun_dir = light.sun;
    // Cast shadows grow with `shadow_len` and fade out under `diffuse` light.
    let (len, cast) = (light.shadow_len, 1.0 - light.diffuse);
    let (tones, soft) = (f32::from(style.tone_steps), style.band_softness);
    let hard = matches!(style.outline, Outline::Dark | Outline::Selout);
    let n = w * h;
    let layout = Layout::new(w, h, seed);
    let (u, s, cx, cy) = (layout.u, layout.s, layout.cx, layout.cy);

    let stone_light = linear(palette.stone_light);
    let stone_dark = linear(palette.stone_dark);
    let sun_col = linear(palette.highlight);
    let shadow = linear(palette.shadow);
    let brightest = shadow.iter().copied().fold(0.001, f32::max);
    let shade = shadow.map(|c| c / brightest);
    let outline = linear(palette.outline);
    let lily_dark = linear(palette.lily_dark);
    let lily_light = linear(palette.lily_light);
    let (flower_col, white, ogon, food) = (linear(palette.lily_flower), linear(palette.koi_white), linear(palette.ogon), linear(palette.food));
    let sand = scale3(mix3(stone_light, food, 0.35), 0.85);
    let pebbles = [stone_light, mix3(stone_light, food, 0.7), mix3(lily_light, stone_light, 0.6), mix3(stone_dark, stone_light, 0.5), mix3(stone_light, food, 0.3), scale3(food, 0.7)];
    let (floor_base, floor_patch) = match scene.floor {
        Floor::Sand => (sand, pebbles[2]),
        Floor::Pebbles => (scale3(mix3(stone_light, stone_dark, 0.3), 0.9), pebbles[2]),
        Floor::Moss => (mix3(scale3(lily_dark, 0.8), sand, 0.45), lily_light),
        Floor::Slate => (mix3(stone_dark, stone_light, 0.45), pebbles[2]),
    };
    // Ochre patches read as sand; on moss they are darker moss, since a locked palette snaps
    // them to bright orange blotches.
    let warm_patch = if scene.floor == Floor::Moss { scale3(lily_dark, 0.8) } else { pebbles[1] };
    let pebble_density = scene.pebbles * if scene.floor == Floor::Pebbles { 1.6 } else { 1.0 };
    let stone_cool = mix3(stone_dark, shade, 0.15 + 0.5 * light.cool);
    let stone_warm = mix3(stone_light, sun_col, light.warm);

    let pads: Vec<Pad> = layout.pads.iter().map(|p| Pad { r: p.r * scene.pad_size, ..*p }).collect();
    let has_flower = |k: usize, pad: &Pad| pad.bloom < scene.flowers || (k == 0 && scene.flowers > 0.0 && pads.iter().all(|p| p.bloom >= scene.flowers));
    let stones: &[Stone] = if scene.rim == Rim::Stones { &layout.stones } else { &[] };

    // Each corner has its own random stream, so a corner looks the same in every theme that
    // names it, whatever the other corners are.
    let mut leaves = Vec::new();
    let mut covers = Vec::new();
    for (index, corner) in scene.foliage.iter().enumerate() {
        let (ax, ay, from, id) = match corner {
            Corner::TopLeft => (-0.04 * u, -0.04 * u, 0.0, 1u64),
            Corner::TopRight => (w as f32 + 0.04 * u, -0.04 * u, PI / 2.0, 2),
            Corner::BottomRight => (w as f32 + 0.03 * u, h as f32 + 0.03 * u, PI, 3),
            Corner::BottomLeft => (-0.04 * u, h as f32 + 0.04 * u, PI * 1.5, 4),
        };
        let mut rng = (seed ^ id.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1;
        let big = index == 0;
        let count = ((if big { 90.0 } else { 45.0 }) * scene.foliage_density).round() as usize;
        let reach = if big { 0.42 } else { 0.3 } * (0.75 + 0.25 * scene.foliage_density).min(1.3);
        covers.push((ax, ay, reach));
        for k in 0..count {
            let spread = from + rand(&mut rng) * PI * 0.5;
            let d = u * reach * rand(&mut rng).sqrt();
            let dir = spread + (rand(&mut rng) - 0.5) * 1.2;
            let tone = (k as f32 / count as f32 * 0.8 + 0.3 * rand(&mut rng)).min(1.0);
            let len = u * (0.04 + 0.035 * rand(&mut rng));
            leaves.push(Leaf { x: ax + spread.cos() * d, y: ay + spread.sin() * d, len, cos: dir.cos(), sin: dir.sin(), tone, pick: rand(&mut rng) });
        }
    }
    let cover_at = |x: f32, y: f32| {
        covers.iter().fold(0.0f32, |c, &(ax, ay, reach)| {
            let d = (x - ax - 0.14 * u * (cx - ax).signum()).hypot(y - ay - 0.16 * u * (cy - ay).signum());
            c.max(1.0 - smoothstep(0.2 * u * reach / 0.42, 0.8 * u * reach / 0.42, d))
        })
    };

    let pad_mask = |x: f32, y: f32, pad: &Pad| {
        let (dx, dy) = (x - pad.x, y - pad.y);
        if dx.abs() > pad.r || dy.abs() > pad.r {
            return 0.0;
        }
        let (lx, ly) = ((dx * pad.cos + dy * pad.sin) / pad.r, (-dx * pad.sin + dy * pad.cos) / pad.r);
        let r = lx.hypot(ly);
        let notch = if lx > 0.0 { smoothstep(0.12, 0.2, (ly / r.max(0.001)).abs()) } else { 1.0 };
        (1.0 - smoothstep(0.93, 1.0, r)) * notch.max(1.0 - smoothstep(0.05, 0.12, r))
    };
    // Coverage and the signed distance across the leaf (its two halves shade differently).
    let leaf_mask = |x: f32, y: f32, leaf: &Leaf| {
        let (dx, dy) = (x - leaf.x, y - leaf.y);
        if dx.abs() > leaf.len * 1.1 || dy.abs() > leaf.len * 1.1 {
            return (0.0, 0.0);
        }
        let wide = if matches!(scene.foliage_kind, FoliageKind::Pine | FoliageKind::Bamboo) { 0.09 } else { 0.22 };
        let lu = (dx * leaf.cos + dy * leaf.sin) / leaf.len * 2.0 - 1.0;
        let lv = (-dx * leaf.sin + dy * leaf.cos) / (leaf.len * wide);
        match scene.foliage_kind {
            FoliageKind::Maple => {
                let lobes = 0.7 + 0.3 * ((lv * wide).atan2((lu + 1.0) * 0.5) * 2.5).cos().abs();
                let r = ((lu + 1.0) * 0.5 - 0.5).hypot(lv * wide * 1.1) / (0.5 * lobes);
                (1.0 - smoothstep(0.85, 1.0, r), lv)
            }
            FoliageKind::Cherry if leaf.pick < 0.45 => (1.0 - smoothstep(0.55, 0.7, lu.hypot(lv * wide * 2.0)), -1.0),
            _ => (1.0 - smoothstep(0.85, 1.0, lu.abs().max(lv.abs() / (1.0 - lu * lu).max(0.01))), lv),
        }
    };

    // One pebble at most per cell of a 0.075 u grid, fewer toward the deep middle. The grid
    // has a margin of one cell, since each pixel looks at the cells around its own.
    let cell = 0.075 * u;
    let cols = (w as f32 / cell).ceil() as i32 + 2;
    let has_pebble: Vec<bool> = (0..cols * ((h as f32 / cell).ceil() as i32 + 2))
        .map(|k| {
            let (ki, kj) = (k % cols - 1, k / cols - 1);
            let depth = layout.depth((ki as f32 + 0.5) * cell, (kj as f32 + 0.5) * cell);
            pebble_density > 0.0 && hash(ki, kj, s + 7) <= mix(0.75, -0.3, depth) * pebble_density.min(1.5)
        })
        .collect();

    let mut out = vec![[0.0f32; 4]; 3 * n + TEX * TEX];
    let (floors, rest) = out.split_at_mut(n);
    let (lights, rest) = rest.split_at_mut(n);
    let (surfs, tile) = rest.split_at_mut(n);
    // Pixels do not depend on each other, so bands of rows paint in parallel.
    let band = h.div_ceil(std::thread::available_parallelism().map_or(1, usize::from)) * w;
    let (layout, pads, leaves, has_pebble) = (&layout, &pads, &leaves, &has_pebble);
    std::thread::scope(|scope| {
        for (((k, floors), lights), surfs) in floors.chunks_mut(band).enumerate().zip(lights.chunks_mut(band)).zip(surfs.chunks_mut(band)) {
            scope.spawn(move || {
                // Only the stones, pads and leaves (with their shadows) that reach this band's
                // rows, in paint order.
                let (top, bottom) = ((k * band / w) as f32, ((k * band + floors.len()) / w) as f32);
                let near = |y: f32, reach: f32| y + reach >= top && y - reach <= bottom;
                let stones: Vec<&Stone> = stones.iter().filter(|st| near(st.y, st.a * 1.5 + 0.05 * u * len)).collect();
                let pads: Vec<(usize, &Pad)> = pads.iter().enumerate().filter(|(_, p)| near(p.y, p.r + 0.11 * u * len)).collect();
                let leaves: Vec<&Leaf> = leaves.iter().filter(|l| near(l.y, l.len * 1.2 + 0.05 * u * len)).collect();
                for (j, ((floor_out, light_out), surf_out)) in floors.iter_mut().zip(lights.iter_mut()).zip(surfs.iter_mut()).enumerate() {
                    let i = k * band + j;
                    let (x, y) = (i % w, i / w);
                    let (px, py) = (x as f32, y as f32);
                    let dist = -layout.sdf(px, py);
                    let grain = if style.grain > 0.0 { (hash(x as i32, y as i32, s + 3) - 0.5) * 0.6 + (noise(px * 0.9, py * 0.25, 1 << 16, s + 4) - 0.5) * 0.8 } else { 0.0 };
                    let depth_raw = layout.depth(px, py);
                    // Ordered dither nudges the depth, so it only shows as a seam where bands meet.
                    let bayer = match style.dither {
                        Dither::None => 0.5,
                        Dither::Bayer2 => ([0.0, 2.0, 3.0, 1.0][(y & 1) * 2 + (x & 1)] + 0.5) / 4.0,
                        Dither::Bayer4 => ([0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0][(y & 3) * 4 + (x & 3)] + 0.5) / 16.0,
                    };
                    let bands = f32::from(style.depth_bands);
                    let depth = steps((depth_raw + grain * 0.03 + (bayer - 0.5) * style.dither_strength / bands).clamp(0.0, 1.0), bands, soft);

                    let mut floor = mix3(floor_base, warm_patch, smoothstep(0.45, 0.7, fbm(px / u * 3.0, py / u * 3.0, 1 << 16, s + 5)) * 0.5);
                    floor = mix3(floor, floor_patch, smoothstep(0.5, 0.7, fbm(px / u * 5.0, py / u * 5.0, 1 << 16, s + 6)) * 0.55);
                    if scene.floor == Floor::Slate {
                        floor = scale3(floor, 0.9 + 0.2 * smoothstep(0.4, 0.6, noise(px / u * 9.0, py / u * 2.5, 1 << 16, s + 30)));
                    }
                    let (ci, cj) = ((px / cell).floor() as i32, (py / cell).floor() as i32);
                    for dj in -1..=1 {
                        for di in -1..=1 {
                            let (ki, kj) = (ci + di, cj + dj);
                            if !has_pebble[((kj + 1) * cols + ki + 1) as usize] {
                                continue;
                            }
                            let (ox, oy) = ((ki as f32 + 0.2 + 0.6 * hash(ki, kj, s + 8)) * cell, (kj as f32 + 0.2 + 0.6 * hash(ki, kj, s + 9)) * cell);
                            let rad = cell * (0.25 + 0.25 * hash(ki, kj, s + 10));
                            let turn = hash(ki, kj, s + 11) * PI;
                            let (dx, dy) = (px - ox, py - oy);
                            let (lx, ly) = ((dx * turn.cos() + dy * turn.sin()) / rad, (-dx * turn.sin() + dy * turn.cos()) / (rad * (0.65 + 0.3 * hash(ki, kj, s + 12))));
                            let r = lx.hypot(ly);
                            let under = (lx - 0.25 * len * turn.cos() + 0.3 * turn.sin()).hypot(ly + 0.25 * turn.sin() - 0.35 * len * turn.cos());
                            floor = scale3(floor, 1.0 - 0.3 * cast * (1.0 - smoothstep(0.85, 1.15, under)));
                            let color = pebbles[(hash(ki, kj, s + 13) * pebbles.len() as f32) as usize % pebbles.len()];
                            let facing = (dx * sun_dir[0] + dy * sun_dir[1]) / rad;
                            let tone = 0.68 + 0.37 * steps(smoothstep(-0.3, 0.3, facing), tones, soft);
                            floor = mix3(floor, scale3(color, tone), 1.0 - smoothstep(0.9, 1.0, r));
                        }
                    }
                    floor = scale3(floor, mix(1.0, 0.8, depth_raw));

                    let mut sun = mix(0.55, 1.0, smoothstep(0.0, 0.05 * u, dist));
                    let floor_offset = (0.05 * u * (0.5 + depth_raw) * len, 0.07 * u * (0.5 + depth_raw) * len);
                    for &(_, pad) in &pads {
                        sun *= 1.0 - 0.55 * cast * pad_mask(px - floor_offset.0, py - floor_offset.1, pad);
                    }
                    let cover = cover_at(px, py);

                    let mut surf = [0.0f32; 4];
                    if dist < 0.5 {
                        let bank = match scene.bank {
                            Bank::Moss => mix3(mix3(scale3(lily_dark, 0.75), stone_dark, 0.35), lily_light, 0.45 * smoothstep(0.45, 0.65, fbm(px / u * 8.0, py / u * 8.0, 1 << 16, s + 14))),
                            Bank::Grass => mix3(mix3(lily_dark, lily_light, 0.35), lily_light, 0.35 * smoothstep(0.45, 0.6, noise(px / u * 30.0, py / u * 8.0, 1 << 16, s + 41))),
                            Bank::Gravel => mix3(mix3(stone_dark, stone_light, 0.45), stone_light, 0.4 * smoothstep(0.3, 0.7, noise(px / (u * 0.012), py / (u * 0.012), 1 << 16, s + 42))),
                        };
                        surf = over(surf, scale3(bank, 1.0 - 0.35 * cover), smoothstep(0.5, -0.5, dist));
                    }
                    if scene.rim == Rim::Planks && dist > -0.02 * u && dist < 0.045 * u {
                        if dist > 0.02 * u {
                            surf = over(surf, scale3(shade, 0.08), 0.4 * cast);
                        } else {
                            // Boards run around the pond, one every 0.05 u of arc, with dark gaps.
                            let arc = (py - cy).atan2(px - cx) * (px - cx).hypot(py - cy) / (0.05 * u);
                            let wood = mix3(food, stone_light, 0.25 + 0.3 * hash(arc.floor() as i32, 0, s + 50));
                            let gap = (arc - arc.floor() - 0.5).abs() > 0.44;
                            let edge = if dist < 0.004 * u { 0.8 } else { 1.0 };
                            surf = over(surf, scale3(mix3(wood, sun_col, 0.15 * (1.0 - edge)), if gap { 0.62 } else { edge }), 1.0);
                        }
                    }
                    for stone in &stones {
                        let (dx, dy) = (px - stone.x - 0.02 * u * len, py - stone.y - 0.028 * u * len);
                        if dx.abs() < stone.a * 1.4 && dy.abs() < stone.a * 1.4 {
                            let (lx, ly) = ((dx * stone.cos + dy * stone.sin) / stone.a, (-dx * stone.sin + dy * stone.cos) / stone.b);
                            surf = over(surf, scale3(shade, 0.08), 0.4 * cast * (1.0 - smoothstep(0.8, 1.3, lx.hypot(ly))));
                        }
                    }
                    for &(_, pad) in &pads {
                        surf = over(surf, scale3(shade, 0.08), 0.35 * cast * pad_mask(px - 0.012 * u * len, py - 0.018 * u * len, pad));
                    }
                    for &(k, pad) in &pads {
                        let alpha = pad_mask(px, py, pad);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let (dx, dy) = (px - pad.x, py - pad.y);
                        let (lx, ly) = ((dx * pad.cos + dy * pad.sin) / pad.r, (-dx * pad.sin + dy * pad.cos) / pad.r);
                        let r = lx.hypot(ly);
                        let facing = (dx * sun_dir[0] + dy * sun_dir[1]) / pad.r;
                        let tone = steps(((0.5 + 0.45 * facing) + grain * 0.1).clamp(0.0, 1.0), tones, soft);
                        let base = mix3(mix3(lily_dark, lily_light, pad.shade), ogon, pad.gold);
                        let toward = if tone > 0.5 { mix3(base, sun_col, light.warm * 0.5) } else { mix3(base, shadow, light.cool) };
                        let mut color = scale3(mix3(base, toward, (tone - 0.5).abs()), 0.8 + 0.35 * tone);
                        let vein = smoothstep(0.9, 1.0, (ly.atan2(lx) * 5.5).cos()) * smoothstep(0.15, 0.3, r) * (1.0 - smoothstep(0.85, 0.9, r));
                        color = scale3(color, 1.0 - 0.12 * vein);
                        color = mix3(color, mix3(lily_light, sun_col, 0.3), 0.35 * smoothstep(0.84, 0.9, r) * (1.0 - smoothstep(0.94, 0.98, r)));
                        surf = over(surf, color, alpha);
                        if style.outline == Outline::Selout && r > 0.86 && r < 1.02 && facing < -0.2 {
                            surf = over(surf, mix3(lily_dark, outline, 0.5), alpha * 0.8);
                        }
                        if !has_flower(k, pad) {
                            continue;
                        }
                        let lily = scene.flower == Flower::WaterLily;
                        let (fx, fy) = (pad.x - pad.r * 0.2 * pad.cos, pad.y - pad.r * 0.2 * pad.sin);
                        let fr = pad.r * if lily { 0.45 } else { 0.55 };
                        let (dx, dy) = (px - fx, py - fy);
                        let (rho, theta) = (dx.hypot(dy) / fr, dy.atan2(dx));
                        let (sdx, sdy) = (px - fx - 0.012 * u * len, py - fy - 0.016 * u * len);
                        let outer_shadow = sdx.hypot(sdy) / fr < 0.75 + 0.25 * (4.0 * theta).cos().abs();
                        surf = over(surf, scale3(shade, 0.1), if outer_shadow { 0.3 * cast } else { 0.0 });
                        let facing = (dx * sun_dir[0] + dy * sun_dir[1]) / fr;
                        let light = 0.85 + 0.25 * smoothstep(-0.2, 0.4, facing);
                        let petal = if lily || pad.pick < 0.5 { flower_col } else { white };
                        if lily {
                            // Flat and star-shaped: eight pointed petals, paler toward the middle.
                            let lobes = 0.55 + 0.45 * (4.0 * theta).cos().abs().powi(3);
                            surf = over(surf, scale3(mix3(petal, white, 0.3 * (1.0 - rho).max(0.0)), light), 1.0 - smoothstep(0.92, 1.0, rho / lobes));
                            surf = over(surf, scale3(ogon, light), 1.0 - smoothstep(0.2, 0.26, rho));
                        } else {
                            let lobes = |k: f32, turn: f32| 0.6 + 0.4 * (k * (theta - turn)).cos().abs().powf(0.6);
                            let tip = mix3(petal, flower_col, 0.5);
                            let outer = mix3(petal, tip, smoothstep(0.5, 1.0, rho));
                            surf = over(surf, scale3(outer, light), 1.0 - smoothstep(0.92, 1.0, rho / lobes(4.0, 0.0)));
                            let inner = mix3(petal, white, 0.45);
                            surf = over(surf, scale3(inner, light * 1.05), 1.0 - smoothstep(0.92, 1.0, rho / (0.62 * lobes(3.0, 0.5))));
                            surf = over(surf, scale3(ogon, light), 1.0 - smoothstep(0.18, 0.24, rho));
                        }
                    }
                    for stone in &stones {
                        let (dx, dy) = (px - stone.x, py - stone.y);
                        if dx.abs() > stone.a * 1.3 || dy.abs() > stone.a * 1.3 {
                            continue;
                        }
                        let (lx, ly) = ((dx * stone.cos + dy * stone.sin) / stone.a, (-dx * stone.sin + dy * stone.cos) / stone.b);
                        let r = lx.hypot(ly) * (1.0 + (noise(px / u * 40.0, py / u * 40.0, 1 << 16, s + 15) - 0.5) * 0.2);
                        if r > 1.05 {
                            continue;
                        }
                        let nz = (1.0 - r.min(1.0).powi(2)).sqrt();
                        let facing = (dx * sun_dir[0] + dy * sun_dir[1]) / stone.a * 0.8 + nz * 0.6;
                        let tone = steps(((facing + 0.25) / 1.3 + grain * 0.08).clamp(0.0, 1.0), tones, soft);
                        let mut color = scale3(mix3(stone_cool, stone_warm, tone), stone.tint);
                        let moss = smoothstep(0.6, 0.68, fbm(px / u * 12.0, py / u * 12.0, 1 << 16, s + 16) + 0.25 * nz - 0.15 * tone + (scene.moss - 1.0) * 0.2);
                        color = mix3(color, mix3(lily_dark, lily_light, 0.2 + 0.7 * tone), 0.85 * moss);
                        color = scale3(mix3(color, shade, 0.2 * smoothstep(0.8, 1.0, r)), 1.0 - 0.3 * smoothstep(0.8, 1.0, r));
                        if hard && r > 0.9 && (style.outline == Outline::Dark || facing < 0.2) {
                            color = mix3(color, outline, 0.6);
                        }
                        surf = over(surf, color, 1.0 - smoothstep(0.95, 1.05, r));
                    }
                    for leaf in &leaves {
                        let (alpha, _) = leaf_mask(px - 0.03 * u * len, py - 0.04 * u * len, leaf);
                        if alpha > 0.0 {
                            surf = over(surf, scale3(shade, 0.06), 0.3 * cast * alpha);
                        }
                    }
                    for leaf in &leaves {
                        let (alpha, lv) = leaf_mask(px, py, leaf);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let base = match scene.foliage_kind {
                            FoliageKind::Maple => mix3(mix3(flower_col, ogon, leaf.pick * 0.6), scale3(flower_col, 0.55), 1.0 - leaf.tone),
                            FoliageKind::Cherry if leaf.pick < 0.45 => mix3(flower_col, white, 0.25 + 0.4 * leaf.tone),
                            FoliageKind::Bamboo => mix3(scale3(lily_dark, 0.6), mix3(lily_light, sun_col, 0.1), leaf.tone),
                            _ => mix3(scale3(lily_dark, 0.45), mix3(lily_light, sun_col, 0.2), leaf.tone),
                        };
                        let side = if lv > 0.0 { 0.8 } else { 1.0 };
                        let rib = 1.0 - smoothstep(0.05, 0.15, lv.abs());
                        surf = over(surf, mix3(scale3(base, side), mix3(base, sun_col, 0.3), 0.4 * rib), alpha);
                    }

                    let damp = if dist < 1.0 || surf[3] > 0.9 { 0.0 } else { DAMP * mix(0.9, 1.0, smoothstep(0.0, 0.07 * u, dist)) * if surf[3] > 0.5 { 0.97 } else { 1.0 } };
                    *floor_out = [floor[0], floor[1], floor[2], depth];
                    *light_out = [sun, cover, grain, damp];
                    *surf_out = surf;
                }
            });
        }
        // The tile of clouds, dapple and caustic cells, in bands of 16 rows.
        let cells = 8;
        let size = TEX as f32 / cells as f32;
        for (k, rows) in tile.chunks_mut(TEX * 16).enumerate() {
            scope.spawn(move || {
                for (j, texel) in rows.iter_mut().enumerate() {
                    let (tx, ty) = (j % TEX, k * 16 + j / TEX);
                    let (x, y) = (tx as f32, ty as f32);
                    let (wx, wy) = (x + (noise(x / 32.0, y / 32.0, 8, s + 24) - 0.5) * 24.0, y + (noise(x / 32.0, y / 32.0, 8, s + 25) - 0.5) * 24.0);
                    let n1 = fbm(x / 64.0, y / 64.0, 4, s + 20);
                    let cloud = smoothstep(0.5, 0.58, n1) * 0.6 + smoothstep(0.6, 0.72, n1) * 0.4;
                    let dapple = smoothstep(0.5, 0.56, fbm(x / 32.0, y / 32.0, 8, s + 21));
                    let (ci, cj) = ((wx / size).floor() as i32, (wy / size).floor() as i32);
                    let (mut f1, mut f2) = (f32::MAX, f32::MAX);
                    for dj in -1..=1 {
                        for di in -1..=1 {
                            let (ki, kj) = (ci + di, cj + dj);
                            let (wi, wj) = (ki.rem_euclid(cells), kj.rem_euclid(cells));
                            let (ox, oy) = ((ki as f32 + hash(wi, wj, s + 22)) * size, (kj as f32 + hash(wi, wj, s + 23)) * size);
                            let d = (wx - ox).hypot(wy - oy) / size;
                            if d < f1 {
                                (f1, f2) = (d, f1);
                            } else if d < f2 {
                                f2 = d;
                            }
                        }
                    }
                    *texel = [cloud, dapple, f2 - f1, fbm(x / 64.0, y / 64.0, 4, s + 26)];
                }
            });
        }
    });

    out
}

impl GpuWater {
    /// Buffers and pipelines for a `w` x `h` pond. `Water::set_theme` fills the statics.
    fn new(gpu: &Gpu, w: usize, h: usize) -> GpuWater {
        let device = &gpu.device;
        let pixels = u64::try_from(w * h).expect("water size fits u64");
        let storage = |label, size, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | usage, mapped_at_creation: false });
        // Copyable both ways, so `Water::regrid` can carry the waves to a new grid.
        let heights = ["height a", "height b"].map(|label| storage(label, pixels * 4, wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST));
        let splashes = storage("splashes", 16 * MAX_SPLASHES as u64, wgpu::BufferUsages::COPY_DST);
        let shadows = storage("shadows", 32 * MAX_SHADOWS as u64, wgpu::BufferUsages::COPY_DST);
        let petals = storage("petals", 32 * (PETALS + FIREFLIES) as u64, wgpu::BufferUsages::COPY_DST);
        let lock = storage("palette lock", 4 * LOCK_SIZE as u64, wgpu::BufferUsages::COPY_DST);
        let out = storage("water out", pixels * 4, wgpu::BufferUsages::COPY_SRC);
        let still = storage("statics", u64::try_from((3 * w * h + TEX * TEX) * std::mem::size_of::<[f32; 4]>()).expect("statics fit u64"), wgpu::BufferUsages::COPY_DST);
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: Some("water readback"), size: pixels * 4, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let write = wgpu::BufferBindingType::Storage { read_only: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[entry(0, wgpu::BufferBindingType::Uniform), entry(1, read), entry(2, write), entry(3, read), entry(4, read), entry(5, write), entry(6, read), entry(7, read), entry(8, read)],
        });
        let bind_groups = [0, 1].map(|k| {
            let buffers = [&params, &heights[k], &heights[1 - k], &splashes, &shadows, &out, &still, &petals, &lock];
            let entries: Vec<wgpu::BindGroupEntry> = (0u32..).zip(buffers).map(|(binding, b)| wgpu::BindGroupEntry { binding, resource: b.as_entire_binding() }).collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries })
        });

        let module = device.create_shader_module(wgpu::include_wgsl!("water.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        GpuWater { gpu: gpu.clone(), wave: pipeline("wave"), shade: pipeline("shade"), bind_groups, current: 0, params, heights, statics: still, splashes, shadows, petals, lock, out, readback }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Petal kinds and sizes differ between themes, but petal k starts in the same place and
    /// drifts the same way in every theme, so a switch does not scatter them.
    #[test]
    fn petals_keep_their_places_across_themes() {
        let catalog = koi_theme::Catalog::load(None).0;
        let (summer, spring) = (catalog.resolve("summer-garden").expect("built-in theme"), catalog.resolve("petal-spring").expect("built-in theme"));
        assert_ne!(summer.scene.petal_kinds, spring.scene.petal_kinds);
        let mut water = Water::new(None, 96, 54, [1.0; 2], 0.25, &summer, 5);
        let before: Vec<(f32, f32, f32, f32)> = water.petals.iter().map(|p| (p.x, p.y, p.vx, p.vy)).collect();
        water.set_theme(&spring);
        assert_eq!(water.petals.len(), 24);
        let after: Vec<(f32, f32, f32, f32)> = water.petals.iter().take(before.len()).map(|p| (p.x, p.y, p.vx, p.vy)).collect();
        assert_eq!(before, after);
    }

    /// Every entry of a lock table is one of the theme's swatches, and each swatch's own
    /// colour snaps to itself, so a locked image only ever shows the listed colours.
    #[test]
    fn lock_table_snaps_to_swatches() {
        let catalog = koi_theme::Catalog::load(None).0;
        for id in ["hillside-summer", "lantern-dusk", "pocket-moss", "ink-and-vermilion"] {
            let palette = catalog.resolve(id).expect("built-in theme").palette;
            let packed: Vec<u32> = palette.swatches.iter().map(|&[r, g, b]| u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16 | 255 << 24).collect();
            let table = lock_table(&palette);
            assert_eq!(table.len(), LOCK_SIZE);
            assert!(table.iter().all(|entry| packed.contains(entry)), "{id}: a table entry is not a swatch");
            for (&[r, g, b], &own) in palette.swatches.iter().zip(&packed) {
                let bin = usize::from(r >> 3) << 10 | usize::from(g >> 3) << 5 | usize::from(b >> 3);
                assert_eq!(table[bin], own, "{id}: swatch {r:02x}{g:02x}{b:02x} does not snap to itself");
            }
        }
    }

    /// Moving to a finer grid keeps the waves: a ring from one splash has the same height and
    /// sits at the same place in simulation units, scaled onto the new grid.
    #[test]
    fn regrid_keeps_the_waves() {
        let catalog = koi_theme::Catalog::load(None).0;
        let (garden, hillside) = (catalog.resolve("summer-garden").expect("built-in theme"), catalog.resolve("hillside-summer").expect("built-in theme"));
        let mut water = Water::new(None, 160, 90, [1.0; 2], 0.25, &garden, 9);
        water.splash(Splash { x: 80.0, y: 45.0, radius: 6.0, amount: 1.5 });
        for _ in 0..20 {
            water.step();
        }
        let peak = |water: &Water| {
            let Backend::Cpu(c) = &water.backend else { unreachable!("built without a GPU") };
            c.height.iter().enumerate().fold((0.0f32, 0), |best, (i, &h)| if h.abs() > best.0 { (h.abs(), i) } else { best })
        };
        let (before, _) = peak(&water);
        water.regrid(240, 135, [1.5; 2], 0.375, &hillside);
        let (after, at) = peak(&water);
        assert!(before > 0.05 && (after - before).abs() < 0.35 * before, "peak {before} became {after}");
        let (x, y) = ((at % 240) as f32 / 1.5, (at / 240) as f32 / 1.5);
        assert!((x - 80.0).hypot(y - 45.0) < 20.0, "the ring moved to ({x}, {y})");
    }
}
