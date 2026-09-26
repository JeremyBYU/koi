use std::f32::consts::PI;

pub const DT: f32 = 1.0 / 60.0;

pub struct Fish {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    wander: f32,
    pub phase: f32,
    pub len: f32,
    pub base: [f32; 3],
    pub spot: [f32; 3],
    pub spot_threshold: f32,
    pub seed: f32,
}

pub struct Food {
    pub x: f32,
    pub y: f32,
    pub age: f32,
}

pub struct Pond {
    pub w: usize,
    pub h: usize,
    pub fish: Vec<Fish>,
    pub food: Vec<Food>,
    pub splashes: Vec<[f32; 4]>,
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

        Pond { w, h, fish, food: Vec::new(), splashes: Vec::new(), rng }
    }

    fn splash(&mut self, x: f32, y: f32, radius: f32, amount: f32) {
        self.splashes.push([x, y, radius, amount]);
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
}
