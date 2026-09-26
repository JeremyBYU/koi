struct Params {
    size: vec2<u32>,
    splashes: u32,
    shadows: u32,
    petals: u32,
    tex: u32,
    wave: f32,
    refract: f32,
    slope: f32,
    unit: f32,
    pad: vec2<f32>,
    cloud_off: vec2<f32>,
    dapple_off: vec2<f32>,
    caustic_a: vec2<f32>,
    caustic_b: vec2<f32>,
    scales: vec4<f32>,
    deep: vec4<f32>,
    mid: vec4<f32>,
    shallow: vec4<f32>,
    sun: vec4<f32>,
    shade: vec4<f32>,
    cloud: vec4<f32>,
    // [light] and [style] from the theme; see `Params` in water.rs.
    sun_dir: vec2<f32>,
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
    wash_top: vec4<f32>,
    wash_bottom: vec4<f32>,
    // Fireflies in `petals` after the petals.
    fireflies: u32,
    // 1 when the water snaps to `lock`.
    lock_on: u32,
    // 1 for dash glints.
    dashes: u32,
    // 1 on the pixel art grid: petals have hard edges.
    crisp: u32,
    // Mist strength toward the edges.
    mist: f32,
    pad2: f32,
    mist_off: vec2<f32>,
}

struct Item {
    a: vec4<f32>,
    b: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> height: array<f32>;
@group(0) @binding(2) var<storage, read_write> next_height: array<f32>;
@group(0) @binding(3) var<storage, read> splashes: array<vec4<f32>>;
// Koi shadows: a = (x, y, dir_x, dir_y), b.x = length, b.y = dive depth.
@group(0) @binding(4) var<storage, read> shadows: array<Item>;
@group(0) @binding(5) var<storage, read_write> out: array<u32>;
// Painted once on the CPU; see `paint` in water.rs for the layout.
@group(0) @binding(6) var<storage, read> statics: array<vec4<f32>>;
// Drifting petals: a = (x, y, angle, size), b = (r, g, b, shape: 0 petal, 1 leaf, 2 maple leaf).
// Then fireflies: a = (x, y, glow, reach), b = (r, g, b, 3).
@group(0) @binding(7) var<storage, read> petals: array<Item>;
// The palette lock table; see `lock_table` in water.rs.
@group(0) @binding(8) var<storage, read> lock: array<u32>;

const PI: f32 = 3.14159265;

fn sq(x: f32) -> f32 {
    return x * x;
}

// Keep in step with `hash` in water.rs.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    var h = (bitcast<u32>(x) * 0x8da6b343u) ^ (bitcast<u32>(y) * 0xd8163841u) ^ (seed * 0xcb1ab31fu);
    h ^= h >> 13u;
    h *= 0x5bd1e995u;
    h ^= h >> 15u;
    return f32(h >> 8u) / 16777216.0;
}

// Keep in step with `steps` in water.rs.
fn steps(x: f32, n: f32, soft: f32) -> f32 {
    let v = x * n;
    let f = v - floor(v);
    var edge = 0.0;
    if soft > 0.0 {
        edge = smoothstep(0.5 - soft / 2.0, 0.5 + soft / 2.0, f);
    } else if f >= 0.5 {
        edge = 1.0;
    }
    return (floor(v) + edge) / n;
}

// One step of the damped wave equation. `next_height` holds the step before this one on
// entry, so the two buffers swap roles every step. Damping is per pixel: zero on land and
// stones, lower in the sponge band along the shore.
@compute @workgroup_size(16, 16)
fn wave(@builtin(global_invocation_id) id: vec3<u32>) {
    let w = params.size.x;
    if id.x < 1u || id.y < 1u || id.x >= w - 1u || id.y >= params.size.y - 1u {
        return;
    }
    let i = id.y * w + id.x;
    let damp = statics[w * params.size.y + i].w;
    let h = height[i];
    let sum = height[i - 1u] + height[i + 1u] + height[i - w] + height[i + w];
    var v = (2.0 * h - next_height[i] + params.wave * (sum - 4.0 * h)) * damp;
    for (var k = 0u; k < params.splashes; k++) {
        let s = splashes[k];
        let d = distance(vec2<f32>(id.xy), s.xy);
        if d < s.z && damp > 0.0 {
            v += s.w * 0.5 * (1.0 + cos(PI * d / s.z));
        }
    }
    if abs(v) < 1e-4 {
        v = 0.0;
    }
    next_height[i] = v;
}

fn bilinear(base: u32, p: vec2<f32>) -> vec4<f32> {
    let w = params.size.x;
    let f = fract(p);
    let p0 = vec2<u32>(floor(p));
    let p1 = min(p0 + 1u, params.size - 1u);
    let top = mix(statics[base + p0.y * w + p0.x], statics[base + p0.y * w + p1.x], f.x);
    let bottom = mix(statics[base + p1.y * w + p0.x], statics[base + p1.y * w + p1.x], f.x);
    return mix(top, bottom, f.y);
}

fn tex(p: vec2<f32>) -> vec4<f32> {
    let n = params.tex;
    let base = 3u * params.size.x * params.size.y;
    let f = fract(p);
    let p0 = vec2<u32>(vec2<i32>(floor(p)) % i32(n) + i32(n)) % n;
    let p1 = (p0 + 1u) % n;
    let top = mix(statics[base + p0.y * n + p0.x], statics[base + p0.y * n + p1.x], f.x);
    let bottom = mix(statics[base + p1.y * n + p0.x], statics[base + p1.y * n + p1.x], f.x);
    return mix(top, bottom, f.y);
}

fn koi_shadow(p: vec2<f32>) -> f32 {
    var total = 0.0;
    for (var k = 0u; k < params.shadows; k++) {
        let s = shadows[k];
        let l = s.b.x;
        let d = p - s.a.xy - vec2<f32>(0.1, 0.15) * l * params.shadow_len * (1.0 - s.b.y);
        let su = dot(d, s.a.zw) / l;
        let sv = (-d.x * s.a.w + d.y * s.a.z) / l;
        let width = 0.14 * (0.45 + 0.55 * smoothstep(-0.5, 0.1, su));
        let r = sq((su - 0.1) / 0.45) + sq(sv / width);
        total = max(total, 1.0 - smoothstep(0.3 + 0.4 * s.b.y, 1.0, r));
    }
    return total;
}

fn srgb(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

@compute @workgroup_size(16, 16)
fn shade(@builtin(global_invocation_id) id: vec3<u32>) {
    let w = params.size.x;
    let h = params.size.y;
    if id.x >= w || id.y >= h {
        return;
    }
    let n = w * h;
    let i = id.y * w + id.x;
    let p = vec2<f32>(id.xy);
    let surf = statics[2u * n + i];
    let here = statics[n + i];
    var col = surf.rgb;
    if surf.a < 0.999 {
        var g = vec2<f32>(0.0);
        var jac = 1.0;
        if id.x >= 1u && id.y >= 1u && id.x < w - 1u && id.y < h - 1u {
            let slope = vec2<f32>(height[i + 1u] - height[i - 1u], height[i + w] - height[i - w]);
            if abs(slope.x) + abs(slope.y) >= 0.003 {
                let c = height[i];
                let d = params.refract * 3.0;
                let hxx = height[i + 1u] + height[i - 1u] - 2.0 * c;
                let hyy = height[i + w] + height[i - w] - 2.0 * c;
                let hxy = (height[i + w + 1u] - height[i + w - 1u] - height[i - w + 1u] + height[i - w - 1u]) * 0.25;
                g = slope;
                jac = (1.0 + d * hxx) * (1.0 + d * hyy) - sq(d * hxy);
            }
        }
        let depth = statics[i].w;
        let src = clamp(p + g * params.refract * (0.5 + depth), vec2<f32>(0.0), vec2<f32>(f32(w - 1u), f32(h - 1u)));
        let bed = bilinear(0u, src);
        let ex = bilinear(n, src);
        let dapple = tex(src * params.scales.y + params.dapple_off).y;
        let open = ex.x * (1.0 - params.leaf_shadows * ex.y * (1.0 - dapple));
        let koi = koi_shadow(floor(src + 0.5)) * (1.0 - params.diffuse);
        let sun = open * (1.0 - 0.6 * koi);
        let cells = min(tex(src * params.scales.z + params.caustic_a).z, tex(src * params.scales.w + params.caustic_b).z);
        let lines = (1.0 - smoothstep(0.0, params.caustic_softness, cells)) * params.caustics * (1.0 - params.diffuse);
        let focus = clamp(1.0 / max(abs(jac), 0.3), 0.5, 2.2);
        let lit = steps(clamp(sun * focus / 1.4, 0.0, 1.0), params.tone_steps, params.band_softness) * 1.4 * (1.0 + 0.35 * lines * sun);
        let clear = params.shallow.rgb / max(max(params.shallow.r, params.shallow.g), max(params.shallow.b, 0.001));
        let floor_col = bed.rgb * mix(vec3<f32>(1.0), clear, 0.45) * (params.shade.rgb * 0.4 + params.sun.rgb * lit * 0.8);
        var water: vec3<f32>;
        if depth < 0.5 {
            water = mix(params.shallow.rgb, params.mid.rgb, depth * 2.0);
        } else {
            water = mix(params.mid.rgb, params.deep.rgb, depth * 2.0 - 1.0);
        }
        col = mix(floor_col, water * (0.7 + 0.4 * open) * (1.0 - 0.3 * koi), 0.15 + 0.72 * depth);

        let tilt = -dot(g, params.sun_dir) * params.slope;
        col = mix(col, params.sun.rgb, 0.25 * smoothstep(0.15, 0.4, tilt)) * (1.0 - 0.15 * smoothstep(0.12, 0.3, -tilt));
        let cloud = tex(p * params.scales.x + params.cloud_off + g * params.slope * 3.0).x;
        col = mix(col, params.cloud.rgb, params.cloud_reflections * cloud * (1.0 - 0.5 * here.y));
        if params.dashes == 1u {
            // Rows are cut into runs of 4 pixels, each lit 2 to 4 long where the crest at its
            // start tilts past the threshold.
            let row = i32(id.y);
            let offset = i32(hash(0, row, 61u) * 4.0);
            let run = (i32(id.x) + offset) / 4;
            let long = 2 + i32(hash(run, row, 62u) * 3.0);
            let ax = u32(clamp(run * 4 - offset, 1, i32(w) - 2));
            if (i32(id.x) + offset) % 4 < long && id.y > 0u && id.y < h - 1u {
                let a = id.y * w + ax;
                let dash = -((height[a + 1u] - height[a - 1u]) * params.sun_dir.x + (height[a + w] - height[a - w]) * params.sun_dir.y) * params.slope;
                if dash > params.glint_threshold {
                    col = mix(col, params.sun.rgb, min(params.glint, 1.0));
                }
            }
        } else {
            col += params.sun.rgb * ((smoothstep(params.glint_threshold, 1.0, tilt) * 0.6 + smoothstep(0.4, 1.0, tilt) * params.bloom) * params.glint);
        }

        for (var k = 0u; k < params.petals; k++) {
            let pt = petals[k];
            let d = p - pt.a.xy;
            if abs(d.x) > pt.a.w * 1.5 || abs(d.y) > pt.a.w * 1.5 {
                continue;
            }
            let ca = cos(pt.a.z);
            let sa = sin(pt.a.z);
            let lu = (d.x * ca + d.y * sa) / pt.a.w;
            let lv = (-d.x * sa + d.y * ca) / pt.a.w;
            var width: f32;
            if pt.b.w > 1.5 {
                width = 0.8 * (0.6 + 0.4 * abs(cos(atan2(lv, lu) * 2.5))) * sqrt(max(1.0 - lu * lu, 0.0));
            } else if pt.b.w > 0.5 {
                width = 0.38 * max(1.0 - lu * lu, 0.0);
            } else {
                width = 0.55 * sqrt(max(1.0 - lu * lu, 0.0)) * (0.7 + 0.3 * lu);
            }
            var alpha = 1.0 - smoothstep(0.85, 1.0, max(abs(lu), abs(lv) / max(width, 0.01)));
            if params.crisp == 1u {
                alpha = select(0.0, 1.0, alpha > 0.5);
            }
            var tone = 1.0;
            if lv > 0.0 {
                tone = 0.85;
            }
            col = mix(col, pt.b.rgb * tone * (0.7 + 0.3 * open), alpha);
        }
        col = col * (1.0 - surf.a) + surf.rgb;
    }
    for (var k = params.petals; k < params.petals + params.fireflies; k++) {
        let f = petals[k];
        let d = distance(p, f.a.xy) / f.a.w;
        if d < 1.0 {
            var core = 0.35 * (1.0 - d);
            if d < 1.0 / 6.0 {
                core = 1.0;
            }
            col = mix(col, f.b.rgb, f.a.z * core);
        }
    }
    if params.mist > 0.0 {
        let edge = min(min(p.x, p.y), min(f32(w - 1u) - p.x, f32(h - 1u) - p.y)) / params.unit;
        let haze = tex(p * params.scales.x + params.mist_off).w;
        col = mix(col, params.sun.rgb, params.mist * (0.25 + 0.55 * (1.0 - smoothstep(0.02, 0.35, edge))) * (0.6 + 0.4 * haze));
    }
    col = mix(col, mix(params.wash_top.rgb, params.wash_bottom.rgb, p.y / f32(h)), params.wash);
    let c = vec3<u32>(srgb(col * (params.ambient * (1.0 + here.z * params.grain * 1.5))) * 255.0 + 0.5);
    if params.lock_on == 1u {
        out[i] = lock[((c.r >> 3u) << 10u) | ((c.g >> 3u) << 5u) | (c.b >> 3u)];
    } else {
        out[i] = c.r | (c.g << 8u) | (c.b << 16u) | (255u << 24u);
    }
}
