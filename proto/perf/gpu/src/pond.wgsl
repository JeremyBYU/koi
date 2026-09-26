struct Params {
    w: u32,
    h: u32,
    splashes: u32,
    fish: u32,
    food: u32,
    rgb: u32,
}

struct Fish {
    pos_dir: vec4<f32>,
    len_phase_seed_threshold: vec4<f32>,
    base: vec4<f32>,
    spot: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> height: array<f32>;
@group(0) @binding(2) var<storage, read_write> prev_height: array<f32>;
@group(0) @binding(3) var<storage, read> splashes: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> fish: array<Fish>;
@group(0) @binding(5) var<storage, read> food: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> out: array<u32>;

const PI: f32 = 3.14159265;

fn sq(x: f32) -> f32 {
    return x * x;
}

@compute @workgroup_size(16, 16)
fn wave(@builtin(global_invocation_id) id: vec3<u32>) {
    let w = params.w;
    if id.x < 1u || id.y < 1u || id.x >= w - 1u || id.y >= params.h - 1u {
        return;
    }
    let i = id.y * w + id.x;
    let sum = height[i - 1u] + height[i + 1u] + height[i - w] + height[i + w];
    var v = (sum * 0.5 - prev_height[i]) * 0.992;
    for (var k = 0u; k < params.splashes; k++) {
        let s = splashes[k];
        let d = distance(vec2<f32>(id.xy), s.xy);
        if d < s.z {
            v += s.w * 0.5 * (1.0 + cos(PI * d / s.z));
        }
    }
    prev_height[i] = v;
}

fn layer(x: u32, y: u32) -> vec3<f32> {
    let w = f32(params.w);
    let h = f32(params.h);
    let p = vec2<f32>(f32(x), f32(y));
    let n = p / vec2<f32>(w, h) - 0.5;
    let r = min(length(n) * 1.6, 1.0);
    let t = r * r * (3.0 - 2.0 * r);
    let f = p / h;
    let mottle = sin(f.x * 9.0 + sin(f.y * 7.0) * 1.5) * cos(f.y * 11.0 - f.x * 4.0);
    var col = clamp(mix(vec3<f32>(12.0, 58.0, 72.0), vec3<f32>(46.0, 112.0, 104.0), t) * (1.0 + 0.07 * mottle), vec3<f32>(0.0), vec3<f32>(255.0));

    for (var k = 0u; k < params.fish; k++) {
        let fh = fish[k];
        let l = fh.len_phase_seed_threshold.x;
        let phase = fh.len_phase_seed_threshold.y;
        let seed = fh.len_phase_seed_threshold.z;
        let c = fh.pos_dir.z;
        let s = fh.pos_dir.w;
        let d = p - fh.pos_dir.xy;
        let shadow_off = vec2<f32>(l * 0.06, l * 0.1);
        let reach = l * 0.62;
        if d.x < -reach || d.y < -reach || d.x > reach + shadow_off.x || d.y > reach + shadow_off.y {
            continue;
        }

        let sd = d - shadow_off;
        let su = (sd.x * c + sd.y * s) / l;
        let sv = (-sd.x * s + sd.y * c) / l;
        let shadow_r = sq((su - 0.1) / 0.42) + sq(sv / 0.15);
        let shadow = clamp((1.0 - shadow_r) * 2.0, 0.0, 1.0) * 0.35;

        let u = (d.x * c + d.y * s) / l;
        let v = (-d.x * s + d.y * c) / l;
        let bend = sin(phase - u * 5.0) * 0.07 * sq(max(0.5 - u, 0.0));
        let vb = v - bend;
        let body_r = sq((u - 0.1) / 0.4) + sq(vb / 0.13);
        let body = clamp((1.0 - body_r) * l * 0.07, 0.0, 1.0);
        let tail_t = (-0.25 - u) / 0.33;
        var tail = 0.0;
        if tail_t >= 0.0 && tail_t < 1.0 {
            tail = clamp((0.03 + 0.13 * tail_t - abs(vb)) * l, 0.0, 1.0) * (0.85 - 0.4 * tail_t);
        }
        let fin_r = sq((u - 0.18) / 0.07) + sq((abs(vb) - 0.15) / 0.05);
        let fin = clamp((1.0 - fin_r) * 2.0, 0.0, 1.0) * 0.55;

        let pattern = sin(u * 9.0 + seed) + cos(v * 14.0 + seed * 1.7) * 0.8;
        var color = fh.base.rgb;
        if pattern > fh.len_phase_seed_threshold.w {
            color = fh.spot.rgb;
        }
        let rounding = 1.0 - 0.3 * min(body_r, 1.0);
        col *= 1.0 - shadow;
        col += (color * 0.9 - col) * fin;
        col += (color - col) * tail;
        col += (color * rounding - col) * body;
    }
    return col;
}

fn pixel(i: u32) -> vec3<f32> {
    let w = params.w;
    let x = i % w;
    let y = i / w;
    var col: vec3<f32>;
    if x >= 1u && y >= 1u && x < w - 1u && y < params.h - 1u {
        let gx = height[i + 1u] - height[i - 1u];
        let gy = height[i + w] - height[i - w];
        let sx = clamp(i32(x) + i32(gx * 6.0), 0, i32(w) - 1);
        let sy = clamp(i32(y) + i32(gy * 6.0), 0, i32(params.h) - 1);
        let glint = f32(i32(-(gx + gy) * 40.0));
        col = clamp(floor(layer(u32(sx), u32(sy))) + glint, vec3<f32>(0.0), vec3<f32>(255.0));
    } else {
        col = layer(x, y);
    }

    let radius = f32(params.h) * 0.011;
    for (var k = 0u; k < params.food; k++) {
        let fd = food[k];
        let fade = clamp(1.0 - (fd.z - 20.0) / 5.0, 0.0, 1.0);
        let d = distance(vec2<f32>(f32(x), f32(y)), fd.xy);
        let a = clamp(radius - d + 0.5, 0.0, 1.0) * fade;
        col += (vec3<f32>(206.0, 158.0, 92.0) * (1.0 - 0.3 * d / radius) - col) * a;
    }
    return clamp(col, vec3<f32>(0.0), vec3<f32>(255.0));
}

@compute @workgroup_size(64)
fn shade(@builtin(global_invocation_id) id: vec3<u32>) {
    let g = id.x;
    if g * 4u >= params.w * params.h {
        return;
    }
    let c0 = vec4<u32>(vec4<f32>(pixel(g * 4u), 255.0));
    let c1 = vec4<u32>(vec4<f32>(pixel(g * 4u + 1u), 255.0));
    let c2 = vec4<u32>(vec4<f32>(pixel(g * 4u + 2u), 255.0));
    let c3 = vec4<u32>(vec4<f32>(pixel(g * 4u + 3u), 255.0));
    if params.rgb == 1u {
        out[g * 3u] = c0.r | (c0.g << 8u) | (c0.b << 16u) | (c1.r << 24u);
        out[g * 3u + 1u] = c1.g | (c1.b << 8u) | (c2.r << 16u) | (c2.g << 24u);
        out[g * 3u + 2u] = c2.b | (c3.r << 8u) | (c3.g << 16u) | (c3.b << 24u);
    } else {
        out[g * 4u] = c0.r | (c0.g << 8u) | (c0.b << 16u) | (c0.a << 24u);
        out[g * 4u + 1u] = c1.r | (c1.g << 8u) | (c1.b << 16u) | (c1.a << 24u);
        out[g * 4u + 2u] = c2.r | (c2.g << 8u) | (c2.b << 16u) | (c2.a << 24u);
        out[g * 4u + 3u] = c3.r | (c3.g << 8u) | (c3.b << 16u) | (c3.a << 24u);
    }
}
