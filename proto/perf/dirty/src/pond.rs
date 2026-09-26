use std::f32::consts::PI;

pub const DT: f32 = 1.0 / 60.0;
const DAMP: f32 = 0.992;

struct Fish {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    wander: f32,
    phase: f32,
    len: f32,
    base: [f32; 3],
    spot: [f32; 3],
    spot_threshold: f32,
    seed: f32,
}

struct Food {
    x: f32,
    y: f32,
    age: f32,
}

pub struct Pond {
    pub w: usize,
    pub h: usize,
    height: Vec<f32>,
    prev_height: Vec<f32>,
    floor: Vec<u8>,
    layer: Vec<u8>,
    pub out: Vec<u8>,
    fish: Vec<Fish>,
    fish_rects: Vec<[usize; 4]>,
    food: Vec<Food>,
    rng: u64,
}

fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

impl Pond {
    pub fn new(w: usize, h: usize, fish_count: usize, seed: u64) -> Pond {
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
            out: floor.clone(),
            floor,
            fish,
            fish_rects: Vec::new(),
            food: Vec::new(),
            rng,
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

    pub fn step_water(&mut self) {
        let w = self.w;
        for y in 1..self.h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                let sum = self.height[i - 1] + self.height[i + 1] + self.height[i - w] + self.height[i + w];
                self.prev_height[i] = (sum * 0.5 - self.prev_height[i]) * DAMP;
            }
        }
        std::mem::swap(&mut self.height, &mut self.prev_height);
    }

    pub fn step_life(&mut self) {
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

    /// Redraws the koi into `layer`, restoring the floor under last frame's koi first.
    pub fn render_layer(&mut self) {
        let (w, h) = (self.w, self.h);
        for &[x0, y0, x1, y1] in &self.fish_rects {
            for y in y0..y1 {
                self.layer[(y * w + x0) * 3..(y * w + x1) * 3].copy_from_slice(&self.floor[(y * w + x0) * 3..(y * w + x1) * 3]);
            }
        }
        self.fish_rects.clear();

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
            self.fish_rects.push([x0, y0, x1 + 1, y1 + 1]);
            for py in y0..=y1 {
                // Columns of this row inside the koi's box (u in [-0.58, 0.5], |v| < 0.29) or its shadow's box.
                let (mut row_lo, mut row_hi) = (f32::MAX, f32::MIN);
                for (ox, oy, u0, u1, vmax) in [(0.0, 0.0, -0.58, 0.5, 0.29), (shadow_x, shadow_y, -0.32, 0.52, 0.15)] {
                    let ry = py as f32 - f.y - oy;
                    let (mut lo, mut hi) = (f32::MIN, f32::MAX);
                    for (k, m, a, b) in [(c, ry * s, u0 * l, u1 * l), (-s, ry * c, -vmax * l, vmax * l)] {
                        if k.abs() < 1e-6 {
                            if m < a || m > b {
                                (lo, hi) = (1.0, 0.0);
                            }
                            continue;
                        }
                        lo = lo.max(((a - m) / k).min((b - m) / k));
                        hi = hi.min(((a - m) / k).max((b - m) / k));
                    }
                    if lo <= hi {
                        row_lo = row_lo.min(lo + f.x + ox);
                        row_hi = row_hi.max(hi + f.x + ox);
                    }
                }
                if row_lo > row_hi {
                    continue;
                }
                for px in (row_lo.floor().max(x0 as f32) as usize)..=(row_hi.ceil().min(x1 as f32) as usize) {
                    let i = (py * w + px) * 3;
                    let dx = px as f32 - f.x;
                    let dy = py as f32 - f.y;

                    let su = ((dx - shadow_x) * c + (dy - shadow_y) * s) / l;
                    let sv = (-(dx - shadow_x) * s + (dy - shadow_y) * c) / l;
                    let shadow_r = ((su - 0.1) / 0.42).powi(2) + (sv / 0.15).powi(2);
                    let shadow = ((1.0 - shadow_r) * 2.0).clamp(0.0, 1.0) * 0.35;

                    let u = (dx * c + dy * s) / l;
                    let v = (-dx * s + dy * c) / l;
                    // Body, tail and fins all lie inside u in (-0.58, 0.5), |v| < 0.29 even at full bend.
                    if shadow == 0.0 && (u < -0.58 || u > 0.5 || v.abs() > 0.29) {
                        continue;
                    }
                    let bend = (f.phase - u * 5.0).sin() * 0.07 * (0.5 - u).max(0.0).powi(2);
                    let vb = v - bend;
                    let body_r = ((u - 0.1) / 0.4).powi(2) + (vb / 0.13).powi(2);
                    let body = ((1.0 - body_r) * l * 0.07).clamp(0.0, 1.0);
                    let tail_t = (-0.25 - u) / 0.33;
                    let tail = if (0.0..1.0).contains(&tail_t) {
                        ((0.03 + 0.13 * tail_t - vb.abs()) * l).clamp(0.0, 1.0) * (0.85 - 0.4 * tail_t)
                    } else {
                        0.0
                    };
                    let fin_r = ((u - 0.18) / 0.07).powi(2) + ((vb.abs() - 0.15) / 0.05).powi(2);
                    let fin = ((1.0 - fin_r) * 2.0).clamp(0.0, 1.0) * 0.55;

                    if shadow == 0.0 && body == 0.0 && tail == 0.0 && fin == 0.0 {
                        continue;
                    }
                    let pattern = (u * 9.0 + f.seed).sin() + (v * 14.0 + f.seed * 1.7).cos() * 0.8;
                    let color = if pattern > f.spot_threshold { f.spot } else { f.base };
                    let rounding = 1.0 - 0.3 * body_r.min(1.0);
                    for ch in 0..3 {
                        let mut value = self.layer[i + ch] as f32 * (1.0 - shadow);
                        value += (color[ch] * 0.9 - value) * fin;
                        value += (color[ch] - value) * tail;
                        value += (color[ch] * rounding - value) * body;
                        self.layer[i + ch] = value as u8;
                    }
                }
            }
        }

    }

    /// Koi and food footprints in image pixels, as [x0, y0, x1, y1) rects.
    pub fn busy_rects(&self) -> Vec<[usize; 4]> {
        let r = (self.h as f32 * 0.011).ceil() as usize + 2;
        let food = self.food.iter().map(|f| {
            let (x, y) = (f.x.max(0.0) as usize, f.y.max(0.0) as usize);
            [x.saturating_sub(r), y.saturating_sub(r), (x + r + 1).min(self.w), (y + r + 1).min(self.h)]
        });
        self.fish_rects.iter().copied().chain(food).collect()
    }

    /// True when no height in `rect` (plus the 1px gradient border) is big enough to
    /// move a pixel or change its glint once glint is quantized to steps of `quant`.
    pub fn calm(&self, [x0, y0, x1, y1]: [usize; 4], quant: i32) -> bool {
        let threshold = (quant as f32 / 160.0).min(1.0 / 12.0);
        let (x0, x1) = (x0.saturating_sub(1), (x1 + 1).min(self.w));
        (y0.saturating_sub(1)..(y1 + 1).min(self.h)).all(|y| self.height[y * self.w + x0..y * self.w + x1].iter().all(|v| v.abs() < threshold))
    }

    /// Shades the water over `rect` of `layer` into `out`, then draws food on top.
    /// A `calm` rect shades to exactly `layer`, so the refraction pass is skipped.
    pub fn render_rect(&mut self, [x0, y0, x1, y1]: [usize; 4], quant: i32, calm: bool) {
        let (w, h) = (self.w, self.h);
        for y in y0..y1 {
            self.out[(y * w + x0) * 3..(y * w + x1) * 3].copy_from_slice(&self.layer[(y * w + x0) * 3..(y * w + x1) * 3]);
        }
        if !calm {
            for y in y0.max(1)..y1.min(h - 1) {
                for x in x0.max(1)..x1.min(w - 1) {
                    let i = y * w + x;
                    let gx = self.height[i + 1] - self.height[i - 1];
                    let gy = self.height[i + w] - self.height[i - w];
                    let sx = (x as i32 + (gx * 6.0) as i32).clamp(0, w as i32 - 1) as usize;
                    let sy = (y as i32 + (gy * 6.0) as i32).clamp(0, h as i32 - 1) as usize;
                    let src = (sy * w + sx) * 3;
                    let glint = (-(gx + gy) * 40.0 / quant as f32) as i32 * quant;
                    for ch in 0..3 {
                        self.out[i * 3 + ch] = (self.layer[src + ch] as i32 + glint).clamp(0, 255) as u8;
                    }
                }
            }
        }

        let radius = h as f32 * 0.011;
        for food in &self.food {
            let fade = (1.0 - (food.age - 20.0) / 5.0).clamp(0.0, 1.0);
            let r = radius.ceil() as i32 + 1;
            for py in (food.y as i32 - r).max(y0 as i32)..=(food.y as i32 + r).min(y1 as i32 - 1) {
                for px in (food.x as i32 - r).max(x0 as i32)..=(food.x as i32 + r).min(x1 as i32 - 1) {
                    let d = ((px as f32 - food.x).powi(2) + (py as f32 - food.y).powi(2)).sqrt();
                    let a = (radius - d + 0.5).clamp(0.0, 1.0) * fade;
                    let i = (py as usize * w + px as usize) * 3;
                    let pellet = [206.0, 158.0, 92.0];
                    for ch in 0..3 {
                        let value = self.out[i + ch] as f32;
                        self.out[i + ch] = (value + (pellet[ch] * (1.0 - 0.3 * d / radius) - value) * a) as u8;
                    }
                }
            }
        }
    }
}
