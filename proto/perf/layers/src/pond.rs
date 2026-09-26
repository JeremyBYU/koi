use std::f32::consts::PI;

pub const DT: f32 = 1.0 / 60.0;
const DAMP: f32 = 0.992;

pub struct Fish {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    wander: f32,
    pub phase: f32,
    len: f32,
    base: [f32; 3],
    spot: [f32; 3],
    spot_threshold: f32,
    seed: f32,
}

const BODY_U: (f32, f32) = (-0.59, 0.51);
const BODY_V: f32 = 0.21;
const MAX_BEND: f32 = 0.085;

pub struct Body {
    w: usize,
    h: usize,
    l: f32,
    rgba: Vec<u8>,
}

impl Body {
    /// The pixel box, relative to the fish centre, that holds the body at `angle` with any
    /// tail bend: (left, top, right, bottom), inclusive.
    pub fn bounds(&self, angle: f32) -> (i32, i32, i32, i32) {
        let l = self.l;
        let (c, s) = (angle.cos(), angle.sin());
        let v_max = BODY_V + MAX_BEND;
        let corners = [(BODY_U.0, -v_max), (BODY_U.0, v_max), (BODY_U.1, -v_max), (BODY_U.1, v_max)].map(|(u, v)| ((u * c - v * s) * l, (u * s + v * c) * l));
        (
            corners.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as i32,
            corners.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32,
            corners.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i32,
            corners.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32,
        )
    }

    /// The body rotated to `angle` with the tail wave at `phase`, as straight-alpha RGBA
    /// covering `w`x`h` pixels whose top-left is (`x0`, `y0`) relative to the fish centre.
    pub fn pose(&self, angle: f32, phase: f32, x0: i32, y0: i32, w: usize, h: usize, buf: &mut Vec<u8>) {
        let l = self.l;
        let (c, s) = (angle.cos(), angle.sin());
        let bend: Vec<f32> = (0..self.w)
            .map(|tx| {
                let u = BODY_U.0 + tx as f32 / l;
                (phase - u * 5.0).sin() * 0.07 * (0.5 - u).max(0.0).powi(2) * l
            })
            .collect();
        buf.clear();
        buf.resize(w * h * 4, 0);
        let (max_x, max_y) = ((self.w - 1) as f32, (self.h - 1) as f32);
        let v_max = (BODY_V + MAX_BEND) * l;
        let span = |a: f32, b: f32, lo: f32, hi: f32| {
            if a.abs() < 1e-6 {
                if (lo..=hi).contains(&b) {
                    (f32::MIN, f32::MAX)
                } else {
                    (f32::MAX, f32::MIN)
                }
            } else {
                let (p, q) = ((lo - b) / a, (hi - b) / a);
                (p.min(q), p.max(q))
            }
        };
        for row in 0..h {
            let py = (y0 + row as i32) as f32;
            let along = span(c, py * s - BODY_U.0 * l, 0.0, max_x);
            let across = span(-s, py * c, -v_max, v_max);
            let first = along.0.max(across.0).max(x0 as f32).ceil();
            let last = along.1.min(across.1).min((x0 + w as i32 - 1) as f32).floor();
            if first > last {
                continue;
            }
            for px in first as i32..=last as i32 {
                let tx = (px as f32 * c + py * s - BODY_U.0 * l).clamp(0.0, max_x - 0.001);
                let ty = -px as f32 * s + py * c + BODY_V * l - bend[tx as usize];
                if !(0.0..max_y).contains(&ty) {
                    continue;
                }
                let (ix, iy) = (tx as usize, ty as usize);
                let (fx, fy) = (tx - ix as f32, ty - iy as f32);
                let top = &self.rgba[(iy * self.w + ix) * 4..][..8];
                let bottom = &self.rgba[((iy + 1) * self.w + ix) * 4..][..8];
                if top[3] | top[7] | bottom[3] | bottom[7] == 0 {
                    continue;
                }
                let lerp = |ch: usize| {
                    let upper = top[ch] as f32 + (top[ch + 4] as f32 - top[ch] as f32) * fx;
                    let lower = bottom[ch] as f32 + (bottom[ch + 4] as f32 - bottom[ch] as f32) * fx;
                    upper + (lower - upper) * fy
                };
                let alpha = lerp(3);
                if alpha < 0.5 {
                    continue;
                }
                let i = (row * w + (px - x0) as usize) * 4;
                for ch in 0..3 {
                    buf[i + ch] = (lerp(ch) * 255.0 / alpha).min(255.0) as u8;
                }
                buf[i + 3] = alpha as u8;
            }
        }
    }
}

pub struct Food {
    pub x: f32,
    pub y: f32,
    pub age: f32,
}

/// The water grid is coarse (a few pixels per cell) and is scaled up by the terminal.
/// `ratio` is water pixels per pixel of the reference `--px 8` pond; wave speed,
/// refraction and glint are rescaled by it so ripples move and shine as they did there.
pub struct Pond {
    pub w: usize,
    pub h: usize,
    height: Vec<f32>,
    prev_height: Vec<f32>,
    floor: Vec<u8>,
    layer: Vec<u8>,
    pub out: Vec<u8>,
    pub fish: Vec<Fish>,
    pub food: Vec<Food>,
    rng: u64,
    wave: f32,
    refract: f32,
    glint: f32,
}

fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

impl Pond {
    pub fn new(w: usize, h: usize, ratio: f32, fish_count: usize, seed: u64) -> Pond {
        let mut floor = vec![0u8; w * h * 3];
        let deep = [12.0, 58.0, 72.0];
        let shallow = [46.0, 112.0, 104.0];
        for y in 0..h {
            for x in 0..w {
                let nx = x as f32 / w as f32 - 0.5;
                let ny = y as f32 / h as f32 - 0.5;
                let r = ((nx * nx + ny * ny).sqrt() * 1.6).min(1.0);
                let t = r * r * (3.0 - 2.0 * r);
                let (fx, fy) = (x as f32 / h as f32, y as f32 / h as f32);
                let mottle = (fx * 9.0 + (fy * 7.0).sin() * 1.5).sin() * (fy * 11.0 - fx * 4.0).cos();
                let light = 1.0 + 0.07 * mottle;
                let i = (y * w + x) * 3;
                for c in 0..3 {
                    floor[i + c] = ((deep[c] + (shallow[c] - deep[c]) * t) * light).clamp(0.0, 255.0) as u8;
                }
            }
        }

        let varieties = [
            ([240.0, 236.0, 226.0], [226.0, 84.0, 38.0], 0.1),
            ([238.0, 112.0, 40.0], [245.0, 240.0, 232.0], 0.7),
            ([244.0, 176.0, 70.0], [250.0, 212.0, 130.0], 0.9),
            ([236.0, 232.0, 222.0], [34.0, 32.0, 36.0], 0.8),
            ([230.0, 96.0, 44.0], [240.0, 236.0, 226.0], 0.3),
        ];
        let mut rng = seed | 1;
        let mut fish = Vec::new();
        for k in 0..fish_count {
            let (base, spot, spot_threshold) = varieties[k % varieties.len()];
            let angle = rand(&mut rng) * 2.0 * PI;
            fish.push(Fish {
                x: w as f32 * (0.2 + 0.6 * rand(&mut rng)),
                y: h as f32 * (0.2 + 0.6 * rand(&mut rng)),
                vx: angle.cos(),
                vy: angle.sin(),
                wander: angle,
                phase: rand(&mut rng) * 6.0,
                len: h as f32 * (0.14 + 0.05 * rand(&mut rng)),
                base,
                spot,
                spot_threshold,
                seed: rand(&mut rng) * 10.0,
            });
        }

        Pond {
            w,
            h,
            height: vec![0.0; w * h],
            prev_height: vec![0.0; w * h],
            layer: floor.clone(),
            out: vec![255; w * h * 4],
            floor,
            fish,
            food: Vec::new(),
            rng,
            wave: 0.5 * ratio * ratio,
            refract: 6.0 * ratio * ratio,
            glint: 40.0 * ratio,
        }
    }

    fn splash(&mut self, x: f32, y: f32, radius: f32, amount: f32) {
        let (w, h) = (self.w as i32, self.h as i32);
        let r = radius.ceil() as i32;
        for py in (y as i32 - r).max(1)..=(y as i32 + r).min(h - 2) {
            for px in (x as i32 - r).max(1)..=(x as i32 + r).min(w - 2) {
                let d = ((px as f32 - x).powi(2) + (py as f32 - y).powi(2)).sqrt();
                if d < radius {
                    self.height[py as usize * self.w + px as usize] += amount * 0.5 * (1.0 + (PI * d / radius).cos());
                }
            }
        }
    }

    pub fn drop_food(&mut self, x: f32, y: f32) {
        self.food.push(Food { x, y, age: 0.0 });
        self.splash(x, y, self.h as f32 * 0.035, 3.0);
    }

    pub fn drop_food_random(&mut self) {
        let x = self.w as f32 * (0.1 + 0.8 * rand(&mut self.rng));
        let y = self.h as f32 * (0.1 + 0.8 * rand(&mut self.rng));
        self.drop_food(x, y);
    }

    pub fn step(&mut self) {
        let w = self.w;
        for y in 1..self.h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                let sum = self.height[i - 1] + self.height[i + 1] + self.height[i - w] + self.height[i + w];
                let h = self.height[i];
                self.prev_height[i] = (2.0 * h - self.prev_height[i] + self.wave * (sum - 4.0 * h)) * DAMP;
            }
        }
        std::mem::swap(&mut self.height, &mut self.prev_height);

        if rand(&mut self.rng) < DT / 3.0 {
            let x = self.w as f32 * rand(&mut self.rng);
            let y = self.h as f32 * rand(&mut self.rng);
            self.splash(x, y, self.h as f32 * 0.012, 0.6);
        }

        for food in &mut self.food {
            food.age += DT;
        }
        self.food.retain(|f| f.age < 25.0);

        let positions: Vec<(f32, f32)> = self.fish.iter().map(|f| (f.x, f.y)).collect();
        let (w, h) = (self.w as f32, self.h as f32);
        let mut wakes = Vec::new();
        let mut eaten = Vec::new();
        for (k, f) in self.fish.iter_mut().enumerate() {
            let cruise = f.len * 0.6;
            f.wander += (rand(&mut self.rng) - 0.5) * 4.0 * DT;
            let mut want_x = f.wander.cos() * cruise;
            let mut want_y = f.wander.sin() * cruise;

            let margin = f.len * 1.2;
            if f.x < margin { want_x += (margin - f.x) / margin * cruise * 2.0; }
            if f.x > w - margin { want_x -= (f.x - (w - margin)) / margin * cruise * 2.0; }
            if f.y < margin { want_y += (margin - f.y) / margin * cruise * 2.0; }
            if f.y > h - margin { want_y -= (f.y - (h - margin)) / margin * cruise * 2.0; }

            let speed = (f.vx * f.vx + f.vy * f.vy).sqrt().max(0.001);
            let (head_x, head_y) = (f.x + f.vx / speed * f.len * 0.45, f.y + f.vy / speed * f.len * 0.45);
            let nearest = self
                .food
                .iter()
                .enumerate()
                .map(|(i, food)| (i, ((food.x - head_x).powi(2) + (food.y - head_y).powi(2)).sqrt()))
                .filter(|&(_, d)| d < w.max(h) * 0.35)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, d)) = nearest {
                if d < f.len * 0.15 {
                    eaten.push(i);
                }
                let dash = cruise * 2.2 * (d / f.len).clamp(0.6, 1.0);
                let (to_x, to_y) = (self.food[i].x - f.x, self.food[i].y - f.y);
                let to_len = (to_x * to_x + to_y * to_y).sqrt().max(0.01);
                want_x = to_x / to_len * dash;
                want_y = to_y / to_len * dash;
            }

            for (j, &(ox, oy)) in positions.iter().enumerate() {
                let (dx, dy) = (f.x - ox, f.y - oy);
                let d = (dx * dx + dy * dy).sqrt();
                if j != k && d < f.len && d > 0.01 {
                    let push = if nearest.is_some() { 0.4 } else { 1.0 } * cruise * (1.0 - d / f.len);
                    want_x += dx / d * push;
                    want_y += dy / d * push;
                }
            }

            let turn = (1.6 * DT).min(1.0);
            f.vx += (want_x - f.vx) * turn;
            f.vy += (want_y - f.vy) * turn;
            let speed = (f.vx * f.vx + f.vy * f.vy).sqrt().max(0.001);
            if nearest.is_none() {
                f.wander = f.vy.atan2(f.vx);
            }
            f.x += f.vx * DT;
            f.y += f.vy * DT;
            f.phase += DT * (3.0 + 5.0 * speed / cruise);
            wakes.push((f.x - f.vx / speed * f.len * 0.3, f.y - f.vy / speed * f.len * 0.3, f.len * 0.08, -0.05 * speed / cruise));
        }
        for (x, y, r, a) in wakes {
            self.splash(x, y, r, a);
        }
        eaten.sort_unstable();
        eaten.dedup();
        for &i in eaten.iter().rev() {
            let food = self.food.remove(i);
            self.splash(food.x, food.y, self.h as f32 * 0.02, 1.0);
        }
    }

    /// Floor, fish shadows and ripples into `out` as RGBA. Fish and food are separate sprites.
    pub fn render_water(&mut self) {
        let (w, h) = (self.w, self.h);
        self.layer.copy_from_slice(&self.floor);

        for f in &self.fish {
            let speed = (f.vx * f.vx + f.vy * f.vy).sqrt().max(0.001);
            let (c, s) = (f.vx / speed, f.vy / speed);
            let l = f.len;
            let (shadow_x, shadow_y) = (l * 0.06, l * 0.1);
            let reach = l * 0.62;
            let x0 = (f.x - reach).max(0.0) as usize;
            let x1 = ((f.x + reach + shadow_x) as usize).min(w - 1);
            let y0 = (f.y - reach).max(0.0) as usize;
            let y1 = ((f.y + reach + shadow_y) as usize).min(h - 1);
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let dx = px as f32 - f.x - shadow_x;
                    let dy = py as f32 - f.y - shadow_y;
                    let su = (dx * c + dy * s) / l;
                    let sv = (-dx * s + dy * c) / l;
                    let shadow_r = ((su - 0.1) / 0.42).powi(2) + (sv / 0.15).powi(2);
                    let shadow = ((1.0 - shadow_r) * 2.0).clamp(0.0, 1.0) * 0.35;
                    if shadow > 0.0 {
                        let i = (py * w + px) * 3;
                        for ch in 0..3 {
                            self.layer[i + ch] = (self.layer[i + ch] as f32 * (1.0 - shadow)) as u8;
                        }
                    }
                }
            }
        }

        let (max_x, max_y) = (((w - 1) * 256 - 1) as f32, ((h - 1) * 256 - 1) as f32);
        let (height, layer, (refract, glint_scale)) = (&self.height, &self.layer, (self.refract, self.glint));
        for (y, row) in self.out.chunks_exact_mut(w * 4).enumerate() {
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let (gx, gy) = if x > 0 && y > 0 && x < w - 1 && y < h - 1 {
                    (height[i + 1] - height[i - 1], height[i + w] - height[i - w])
                } else {
                    (0.0, 0.0)
                };
                let sx = ((x as f32 + gx * refract) * 256.0).clamp(0.0, max_x) as i32 as usize;
                let sy = ((y as f32 + gy * refract) * 256.0).clamp(0.0, max_y) as i32 as usize;
                let (fx, fy) = ((sx & 255) as i32, (sy & 255) as i32);
                let top = ((sy >> 8) * w + (sx >> 8)) * 3;
                let upper = &layer[top..top + 6];
                let lower = &layer[top + w * 3..top + w * 3 + 6];
                let glint = (-(gx + gy) * glint_scale) as i32;
                for ch in 0..3 {
                    let a = upper[ch] as i32 * (256 - fx) + upper[3 + ch] as i32 * fx;
                    let b = lower[ch] as i32 * (256 - fx) + lower[3 + ch] as i32 * fx;
                    pixel[ch] = (((a * (256 - fy) + b * fy) >> 16) + glint).clamp(0, 255) as u8;
                }
            }
        }
    }

    /// Fish `k` swimming straight along +u, as premultiplied RGBA with `scale` texels per
    /// water pixel. Posing it for a frame is then a resample instead of a full shading pass.
    pub fn render_body(&self, k: usize, scale: f32) -> Body {
        let f = &self.fish[k];
        let l = f.len * scale;
        let (w, h) = (((BODY_U.1 - BODY_U.0) * l).ceil() as usize + 1, (2.0 * BODY_V * l).ceil() as usize + 1);
        let mut rgba = vec![0u8; w * h * 4];
        for ty in 0..h {
            for tx in 0..w {
                let u = BODY_U.0 + tx as f32 / l;
                let v = -BODY_V + ty as f32 / l;
                let body_r = ((u - 0.1) / 0.4).powi(2) + (v / 0.13).powi(2);
                let body = ((1.0 - body_r) * l * 0.07).clamp(0.0, 1.0);
                let tail_t = (-0.25 - u) / 0.33;
                let tail = if (0.0..1.0).contains(&tail_t) {
                    ((0.03 + 0.13 * tail_t - v.abs()) * l).clamp(0.0, 1.0) * (0.85 - 0.4 * tail_t)
                } else {
                    0.0
                };
                let fin_r = ((u - 0.18) / 0.07).powi(2) + ((v.abs() - 0.15) / 0.05).powi(2);
                let fin = ((1.0 - fin_r) * 2.0).clamp(0.0, 1.0) * 0.55;
                let pattern = (u * 9.0 + f.seed).sin() + (v * 14.0 + f.seed * 1.7).cos() * 0.8;
                let color = if pattern > f.spot_threshold { f.spot } else { f.base };
                let rounding = 1.0 - 0.3 * body_r.min(1.0);
                let mut premul = [0.0f32; 3];
                let mut alpha = 0.0;
                for (shade, a) in [(0.9, fin), (1.0, tail), (rounding, body)] {
                    for ch in 0..3 {
                        premul[ch] += (color[ch] * shade - premul[ch]) * a;
                    }
                    alpha += (1.0 - alpha) * a;
                }
                let i = (ty * w + tx) * 4;
                for ch in 0..3 {
                    rgba[i + ch] = premul[ch] as u8;
                }
                rgba[i + 3] = (alpha * 255.0) as u8;
            }
        }
        Body { w, h, l, rgba }
    }

    /// A food pellet at `fade` opacity, `scale` sprite pixels per water pixel. Returns its
    /// side length; the pellet centre is the middle of the square.
    pub fn render_pellet(&self, fade: f32, scale: f32, buf: &mut Vec<u8>) -> usize {
        let radius = self.h as f32 * 0.011 * scale;
        let r = radius.ceil() as usize + 1;
        let size = 2 * r + 1;
        let pellet = [206.0, 158.0, 92.0];
        buf.clear();
        for py in 0..size {
            for px in 0..size {
                let d = ((px as f32 - r as f32).powi(2) + (py as f32 - r as f32).powi(2)).sqrt();
                let a = (radius - d + 0.5).clamp(0.0, 1.0) * fade;
                for c in pellet {
                    buf.push((c * (1.0 - 0.3 * d / radius)).clamp(0.0, 255.0) as u8);
                }
                buf.push((a * 255.0) as u8);
            }
        }
        size
    }
}
