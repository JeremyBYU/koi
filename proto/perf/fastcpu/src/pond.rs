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
    water: usize,
    ww: usize,
    wh: usize,
    wave_c2: f32,
    threads: usize,
    height: Vec<f32>,
    prev_height: Vec<f32>,
    floor: Vec<u32>,
    layer: Vec<u32>,
    drawn: Vec<[usize; 4]>,
    fish: Vec<Fish>,
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
    /// `water` is how many image pixels one water-simulation cell covers on each axis,
    /// rounded up to a power of two.
    pub fn new(w: usize, h: usize, fish_count: usize, seed: u64, water: usize, threads: usize) -> Pond {
        let mut floor = vec![0u32; w * h];
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
                let mut rgba = [255u8; 4];
                for c in 0..3 {
                    rgba[c] = ((deep[c] + (shallow[c] - deep[c]) * t) * light).clamp(0.0, 255.0) as u8;
                }
                floor[y * w + x] = u32::from_le_bytes(rgba);
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

        let water = water.max(1).next_power_of_two();
        let (ww, wh) = (w / water + 4, h / water + 4);
        Pond {
            w,
            h,
            water,
            ww,
            wh,
            // The stock scheme moves waves 1/sqrt(2) cells per step. Scaling c^2 by 1/water^2
            // keeps that speed in image pixels, so a coarse grid ripples like the full-res one.
            wave_c2: 0.5 / (water * water) as f32,
            threads: threads.max(1),
            height: vec![0.0; ww * wh],
            prev_height: vec![0.0; ww * wh],
            layer: floor.clone(),
            drawn: Vec::new(),
            floor,
            fish,
            food: Vec::new(),
            rng,
        }
    }

    fn splash(&mut self, x: f32, y: f32, radius: f32, amount: f32) {
        let k = self.water as f32;
        let (x, y, radius) = (x / k + 1.0, y / k + 1.0, radius / k);
        let (w, h) = (self.ww as i32, self.wh as i32);
        let r = radius.ceil() as i32;
        for py in (y as i32 - r).max(1)..=(y as i32 + r).min(h - 2) {
            for px in (x as i32 - r).max(1)..=(x as i32 + r).min(w - 2) {
                let d = ((px as f32 - x).powi(2) + (py as f32 - y).powi(2)).sqrt();
                if d < radius {
                    self.height[py as usize * self.ww + px as usize] += amount * 0.5 * (1.0 + (PI * d / radius).cos());
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
        let (w, c2) = (self.ww, self.wave_c2);
        for y in 1..self.wh - 1 {
            let (up, here, down) = (&self.height[(y - 1) * w..y * w], &self.height[y * w..(y + 1) * w], &self.height[(y + 1) * w..(y + 2) * w]);
            let next = &mut self.prev_height[y * w..(y + 1) * w];
            for x in 1..w - 1 {
                let sum = here[x - 1] + here[x + 1] + up[x] + down[x];
                next[x] = (2.0 * here[x] - next[x] + c2 * (sum - 4.0 * here[x])) * DAMP;
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
            f.phase = (f.phase + DT * (3.0 + 5.0 * speed / cruise)) % (2.0 * PI);
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

    /// Renders the frame into `out` (w * h RGBA pixels), splitting rows across `threads`.
    pub fn render(&mut self, out: &mut [u32]) {
        let (w, h) = (self.w, self.h);
        let band = h.div_ceil(self.threads);
        // Branch-free sine (error below 5e-4) so the fish loop vectorizes; libm sinf does not.
        let sin = |a: f32| {
            let x = a - (a * (0.5 / PI)).round() * (2.0 * PI);
            let x2 = x * x;
            x * (1.0 + x2 * (-1.0 / 6.0 + x2 * (1.0 / 120.0 + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362880.0 + x2 * (-1.0 / 39916800.0))))))
        };

        // Every part of the koi (body, bent tail, fins) lies inside a box in fish coordinates; the
        // shadow lies inside the same box shifted by the shadow offset. Only these pixel boxes
        // differ from the floor, so the next frame restores just them.
        let shapes: Vec<(f32, f32, [(f32, f32); 4], [usize; 4])> = self
            .fish
            .iter()
            .map(|f| {
                let speed = (f.vx * f.vx + f.vy * f.vy).sqrt().max(0.001);
                let (c, s, l) = (f.vx / speed, f.vy / speed, f.len);
                let corners = [(-0.6, -0.26), (0.52, -0.26), (0.52, 0.26), (-0.6, 0.26)].map(|(u, v): (f32, f32)| (f.x + (u * c - v * s) * l, f.y + (u * s + v * c) * l));
                let (xs, ys) = (corners.map(|p| p.0), corners.map(|p| p.1));
                let (left, right) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max) + l * 0.06);
                let (top, bottom) = (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max) + l * 0.1);
                let x_range = [((left.floor() - 1.0).max(0.0) as usize).min(w), ((right.ceil() + 2.0).max(0.0) as usize).min(w)];
                let y_range = [((top.floor() - 1.0).max(0.0) as usize).min(h), ((bottom.ceil() + 2.0).max(0.0) as usize).min(h)];
                (c, s, corners, [x_range[0], x_range[1], y_range[0], y_range[1]])
            })
            .collect();

        std::thread::scope(|scope| {
            for (b, (layer, floor)) in self.layer.chunks_mut(band * w).zip(self.floor.chunks(band * w)).enumerate() {
                let (fish, shapes, drawn) = (&self.fish, &shapes, &self.drawn);
                scope.spawn(move || {
                    let (band_y0, band_y1) = (b * band, b * band + layer.len() / w);
                    for &[x0, x1, y0, y1] in drawn {
                        for y in y0.max(band_y0)..y1.min(band_y1) {
                            let row = (y - band_y0) * w;
                            layer[row + x0..row + x1].copy_from_slice(&floor[row + x0..row + x1]);
                        }
                    }
                    for (f, &(c, s, corners, [_, _, y0, y1])) in fish.iter().zip(shapes) {
                        let (l, inv_l) = (f.len, 1.0 / f.len);
                        let (shadow_x, shadow_y) = (l * 0.06, l * 0.1);
                        for py in y0.max(band_y0)..y1.min(band_y1) {
                            let (mut xa, mut xb) = (f32::MAX, f32::MIN);
                            for (ox, oy) in [(0.0, 0.0), (shadow_x, shadow_y)] {
                                for e in 0..4 {
                                    let (p, q) = (corners[e], corners[(e + 1) % 4]);
                                    let (p, q) = ((p.0 + ox, p.1 + oy), (q.0 + ox, q.1 + oy));
                                    let (lo, hi) = (p.1.min(q.1), p.1.max(q.1));
                                    let (row_lo, row_hi) = ((py as f32 - 0.5).max(lo), (py as f32 + 0.5).min(hi));
                                    if row_lo > row_hi {
                                        continue;
                                    }
                                    let (x_lo, x_hi) = if hi > lo {
                                        (p.0 + (q.0 - p.0) * (row_lo - p.1) / (q.1 - p.1), p.0 + (q.0 - p.0) * (row_hi - p.1) / (q.1 - p.1))
                                    } else {
                                        (p.0, q.0)
                                    };
                                    xa = xa.min(x_lo.min(x_hi));
                                    xb = xb.max(x_lo.max(x_hi));
                                }
                            }
                            let x0 = (xa.floor() - 1.0).max(0.0) as usize;
                            let x1 = ((xb.ceil() + 2.0).max(0.0) as usize).min(w);
                            if x0 >= x1 {
                                continue;
                            }
                            let row = &mut layer[(py - band_y0) * w..(py - band_y0 + 1) * w];
                            for (i, pixel) in row[x0..x1].iter_mut().enumerate() {
                                let dx = (x0 + i) as f32 - f.x;
                                let dy = py as f32 - f.y;

                                let (sdx, sdy) = (dx - shadow_x, dy - shadow_y);
                                let su = (sdx * c + sdy * s) * inv_l;
                                let sv = (sdy * c - sdx * s) * inv_l;
                                let shadow_r = ((su - 0.1) * (1.0 / 0.42)).powi(2) + (sv * (1.0 / 0.15)).powi(2);
                                let shadow = ((1.0 - shadow_r) * 2.0).clamp(0.0, 1.0) * 0.35;

                                let u = (dx * c + dy * s) * inv_l;
                                let v = (dy * c - dx * s) * inv_l;
                                let bend = sin(f.phase - u * 5.0) * 0.07 * (0.5 - u).max(0.0).powi(2);
                                let vb = v - bend;
                                let body_r = ((u - 0.1) * (1.0 / 0.4)).powi(2) + (vb * (1.0 / 0.13)).powi(2);
                                let body = ((1.0 - body_r) * l * 0.07).clamp(0.0, 1.0);
                                let tail_t = (-0.25 - u) * (1.0 / 0.33);
                                let tail_value = ((0.03 + 0.13 * tail_t - vb.abs()) * l).clamp(0.0, 1.0) * (0.85 - 0.4 * tail_t);
                                let tail = if (0.0..1.0).contains(&tail_t) { tail_value } else { 0.0 };
                                let fin_r = ((u - 0.18) * (1.0 / 0.07)).powi(2) + ((vb.abs() - 0.15) * (1.0 / 0.05)).powi(2);
                                let fin = ((1.0 - fin_r) * 2.0).clamp(0.0, 1.0) * 0.55;

                                let pattern = sin(u * 9.0 + f.seed) + sin(v * 14.0 + f.seed * 1.7 + 0.5 * PI) * 0.8;
                                let spotted = pattern > f.spot_threshold;
                                let rounding = 1.0 - 0.3 * body_r.min(1.0);
                                let mut packed = 0xff00_0000u32;
                                for ch in 0..3 {
                                    let color = if spotted { f.spot[ch] } else { f.base[ch] };
                                    let mut value = ((*pixel >> (8 * ch)) & 0xff) as f32 * (1.0 - shadow);
                                    value += (color * 0.9 - value) * fin;
                                    value += (color - value) * tail;
                                    value += (color * rounding - value) * body;
                                    packed |= (value as u32) << (8 * ch);
                                }
                                *pixel = packed;
                            }
                        }
                    }
                });
            }
        });

        self.drawn = shapes.iter().map(|shape| shape.3).collect();
        let (k, ww) = (self.water, self.ww);
        let (cells, shift, inv_k) = (w.div_ceil(k), k.trailing_zeros(), 1.0 / k as f32);
        let offset_scale = 6.0 / k as f32;
        let (height, layer) = (&self.height, &self.layer);
        std::thread::scope(|scope| {
            for (b, out) in out.chunks_mut(band * w).enumerate() {
                scope.spawn(move || {
                    let mut row_ox = vec![0.0f32; ww];
                    let mut row_oy = vec![0.0f32; ww];
                    let mut calm = vec![false; ww];
                    for (dy, out_row) in out.chunks_exact_mut(w).enumerate() {
                        let y = b * band + dy;
                        let (r, fy) = (y / k + 1, (y % k) as f32 * inv_k);
                        let (up, here, down, below) = (&height[(r - 1) * ww..], &height[r * ww..], &height[(r + 1) * ww..], &height[(r + 2) * ww..]);
                        for xl in 1..ww - 1 {
                            let gx0 = here[xl + 1] - here[xl - 1];
                            let gx1 = down[xl + 1] - down[xl - 1];
                            let gy0 = down[xl] - up[xl];
                            let gy1 = below[xl] - here[xl];
                            row_ox[xl] = (gx0 + (gx1 - gx0) * fy) * offset_scale;
                            row_oy[xl] = (gy0 + (gy1 - gy0) * fy) * offset_scale;
                            // Offsets truncate toward zero and glint is -(ox + oy) * 40/6, so pixels
                            // between calm nodes are copies of the layer, off by at most one level of
                            // glint.
                            calm[xl] = row_ox[xl].abs() < 1.0 && row_oy[xl].abs() < 1.0 && (row_ox[xl] + row_oy[xl]).abs() < 0.3;
                        }
                        let layer_row = &layer[y * w..(y + 1) * w];
                        let mut cell = 0;
                        while cell < cells {
                            let start = cell;
                            let run_calm = calm[cell + 1] && calm[cell + 2];
                            while cell < cells && (calm[cell + 1] && calm[cell + 2]) == run_calm {
                                cell += 1;
                            }
                            let (xs, xe) = (start << shift, (cell << shift).min(w));
                            if run_calm {
                                out_row[xs..xe].copy_from_slice(&layer_row[xs..xe]);
                                continue;
                            }
                            for (x, pixel) in (xs..xe).zip(&mut out_row[xs..xe]) {
                                let (c, frac) = ((x >> shift) + 1, (x & (k - 1)) as f32 * inv_k);
                                let ox = row_ox[c] + (row_ox[c + 1] - row_ox[c]) * frac;
                                let oy = row_oy[c] + (row_oy[c + 1] - row_oy[c]) * frac;
                                let sx = (x as i32 + ox as i32).clamp(0, w as i32 - 1) as usize;
                                let sy = (y as i32 + oy as i32).clamp(0, h as i32 - 1) as usize;
                                let glint = (-(ox + oy) * (40.0 / 6.0)) as i32;
                                let [r, g, b, _] = layer[sy * w + sx].to_le_bytes();
                                *pixel = u32::from_le_bytes([
                                    (r as i32 + glint).clamp(0, 255) as u8,
                                    (g as i32 + glint).clamp(0, 255) as u8,
                                    (b as i32 + glint).clamp(0, 255) as u8,
                                    255,
                                ]);
                            }
                        }
                    }
                });
            }
        });

        let radius = h as f32 * 0.011;
        for food in &self.food {
            let fade = (1.0 - (food.age - 20.0) / 5.0).clamp(0.0, 1.0);
            let r = radius.ceil() as i32 + 1;
            for py in (food.y as i32 - r).max(0)..=(food.y as i32 + r).min(h as i32 - 1) {
                for px in (food.x as i32 - r).max(0)..=(food.x as i32 + r).min(w as i32 - 1) {
                    let d = ((px as f32 - food.x).powi(2) + (py as f32 - food.y).powi(2)).sqrt();
                    let a = (radius - d + 0.5).clamp(0.0, 1.0) * fade;
                    let i = py as usize * w + px as usize;
                    let pellet = [206.0, 158.0, 92.0];
                    let mut rgba = out[i].to_le_bytes();
                    for ch in 0..3 {
                        let value = rgba[ch] as f32;
                        rgba[ch] = (value + (pellet[ch] * (1.0 - 0.3 * d / radius) - value) * a) as u8;
                    }
                    out[i] = u32::from_le_bytes(rgba);
                }
            }
        }
    }
}
