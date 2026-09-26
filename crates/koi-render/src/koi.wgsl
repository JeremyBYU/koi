struct Params {
    origin: vec2<f32>,
    size: vec2<u32>,
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
    // Outline mode: 0 none, 1 soft, 2 dark, 3 selout, 4 rim.
    mode: u32,
    // 1 in pixel themes: the nearest texel of the nearest beat, no anti-aliasing.
    crisp: u32,
    // Towards the sun, unit length.
    sun: vec2<f32>,
    // 1 when the colours snap to `lock`.
    lock_on: u32,
    // How deep the koi has dived, 0 to 1 (`Pose::depth`).
    depth: f32,
    // 0..1 sRGB. `water` is `mid`, standing in for the water under the fish; `deep` is the
    // water over a diving koi.
    outline: vec4<f32>,
    shadow: vec4<f32>,
    water: vec4<f32>,
    deep: vec4<f32>,
    curve: array<vec4<f32>, CURVE>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;
@group(0) @binding(2) var<storage, read> texels: array<vec4<f32>>;
// How much of the outline colour replaces the body, per texel; same layout as `texels`.
@group(0) @binding(3) var<storage, read> edges: array<f32>;
// The palette lock table; see `lock_table` in water.rs.
@group(0) @binding(4) var<storage, read> lock: array<u32>;

// CURVE in koi.rs.
const CURVE: u32 = 31u;
const TAIL: f32 = 1.3;
// DIVE_DIM in koi.rs.
const DIVE_DIM: f32 = 0.3;
const TAU: f32 = 6.28318530718;

fn texel(x: u32, y: u32) -> vec4<f32> {
    let i = y * params.tex_w + x;
    return mix(texels[params.tex_a + i], texels[params.tex_b + i], params.blend);
}

fn edge(x: u32, y: u32) -> f32 {
    let i = y * params.tex_w + x;
    return mix(edges[params.tex_a + i], edges[params.tex_b + i], params.blend);
}

// Keep in step with `edge_color` in koi.rs.
fn edge_color(body: vec3<f32>, facing: f32) -> vec3<f32> {
    switch params.mode {
        case 1u: {
            return mix(mix(params.water.rgb, body, 0.3), params.outline.rgb, 0.8);
        }
        case 3u: {
            if facing > 0.25 {
                return mix(body, params.outline.rgb, 0.45);
            }
        }
        case 4u: {
            if facing > 0.0 {
                return mix(params.outline.rgb, body, 0.2);
            }
            return mix(body, params.shadow.rgb, 0.6);
        }
        default: {}
    }
    return params.outline.rgb;
}

// One koi pose, as straight-alpha RGBA. Pixel (0, 0) of the canvas sits at `origin` sprite
// pixels from the fish centre. Each pixel finds its nearest point on the spine curve, which
// gives its distance along the body and to the side; those index the straight body texture,
// after the travelling tail wave. Keep in step with the CPU path in koi.rs.
@compute @workgroup_size(16, 16)
fn pose(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.size.x || id.y >= params.size.y {
        return;
    }
    let i = id.y * params.size.x + id.x;
    out[i] = 0u;
    let l = params.len;
    let p = (vec2<f32>(id.xy) + params.origin) / l;

    var best = 3.4e38;
    var a = 0.0;
    var vb = 0.0;
    var along = vec2<f32>(1.0, 0.0);
    for (var n = 0u; n + 1u < CURVE; n++) {
        let q = params.curve[n];
        let r = params.curve[n + 1u];
        let t = r.xy - q.xy;
        let len2 = dot(t, t);
        let s = clamp(dot(p - q.xy, t) / len2, 0.0, 1.0);
        let d = p - q.xy - t * s;
        let d2 = dot(d, d);
        if d2 < best {
            best = d2;
            a = mix(q.z, r.z, s);
            vb = (d.x * t.y - d.y * t.x) / sqrt(len2);
            along = t / sqrt(len2);
        }
    }

    let t = clamp(a / TAIL, 0.0, 1.0);
    let tv = vb - params.amp * (0.1 + 0.9 * t * t) * sin(params.phase - TAU * t);
    let tx = (0.5 - a - params.tex_u0) * l;
    let ty = (tv + params.tex_v) * l;
    if tx < 0.0 || ty < 0.0 || tx >= f32(params.tex_w - 1u) || ty >= f32(params.tex_h - 1u) {
        return;
    }
    let ix = u32(tx);
    let iy = u32(ty);
    let f = vec2<f32>(tx - f32(ix), ty - f32(iy));
    var v: vec4<f32>;
    var e: f32;
    if params.crisp == 1u {
        let j = u32(ty + 0.5) * params.tex_w + u32(tx + 0.5);
        let beat = select(params.tex_b, params.tex_a, params.blend < 0.5);
        v = texels[beat + j];
        e = edges[beat + j];
    } else {
        let top = mix(texel(ix, iy), texel(ix + 1u, iy), f.x);
        let bottom = mix(texel(ix, iy + 1u), texel(ix + 1u, iy + 1u), f.x);
        v = mix(top, bottom, f.y);
        e = mix(mix(edge(ix, iy), edge(ix + 1u, iy), f.x), mix(edge(ix, iy + 1u), edge(ix + 1u, iy + 1u), f.x), f.y);
    }
    if v.a < 0.5 / 255.0 {
        return;
    }
    let body = v.rgb / v.a;
    // The side of the body this pixel is on, in screen space, against the sun.
    let facing = dot(vec2<f32>(along.y, -along.x), params.sun) * select(1.0, -1.0, vb < 0.0);
    let dimmed = mix(mix(body, edge_color(body, facing), e), params.deep.rgb, DIVE_DIM * params.depth);
    let rgb = vec3<u32>(min(dimmed * 255.0, vec3<f32>(255.0)));
    var packed = rgb.r | (rgb.g << 8u) | (rgb.b << 16u);
    if params.lock_on == 1u {
        packed = lock[((rgb.r >> 3u) << 10u) | ((rgb.g >> 3u) << 5u) | (rgb.b >> 3u)] & 0xffffffu;
    }
    out[i] = packed | (u32(v.a * 255.0) << 24u);
}
