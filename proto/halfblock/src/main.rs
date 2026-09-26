use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use crossterm::{cursor, execute, terminal};
use std::f32::consts::{PI, TAU};
use std::io::{self, Write};
use std::time::{Duration, Instant};

const DT: f32 = 1.0 / 60.0;
const DAMPING: f32 = 0.988;
const KOI_LEN: f32 = 6.0;
const KOI_WIDTH: f32 = 2.3;
const TAIL_LEN: f32 = 4.5;
// A cell is only re-sent once a channel drifts more than this from what is on screen.
// Fading ripples otherwise touch nearly every cell by 1-2 levels each frame.
const COLOR_TOLERANCE: u8 = 2;

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    ch: char,
    fg: [u8; 3],
    bg: [u8; 3],
}

struct Rng(u64);

impl Rng {
    fn f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

struct Koi {
    x: f32,
    y: f32,
    heading: f32,
    speed: f32,
    wander: f32,
    phase: f32,
    body: [f32; 3],
    patch: [f32; 3],
    seed: f32,
}

struct Food {
    x: f32,
    y: f32,
    age: f32,
}

struct Pond {
    w: usize,
    h: usize,
    cur: Vec<f32>,
    prev: Vec<f32>,
    base: Vec<[f32; 3]>,
    layer: Vec<[f32; 3]>,
    px: Vec<[u8; 3]>,
    koi: Vec<Koi>,
    food: Vec<Food>,
    rng: Rng,
}

impl Pond {
    fn new(w: usize, h: usize, seed: u64) -> Pond {
        let deep = [14.0, 52.0, 66.0];
        let shallow = [30.0, 92.0, 98.0];
        let mut base = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32, y as f32);
                let t = 0.5 + 0.25 * ((fx * 0.045 + fy * 0.03).sin() + (fy * 0.065 - fx * 0.02).sin());
                let edge = fx.min(fy).min(w as f32 - 1.0 - fx).min(h as f32 - 1.0 - fy);
                let shade = 0.7 + 0.3 * (edge / 10.0).min(1.0);
                base.push([0, 1, 2].map(|c| (deep[c] + (shallow[c] - deep[c]) * t) * shade));
            }
        }
        let mut rng = Rng(seed.max(1));
        let palettes = [
            ([235.0, 232.0, 222.0], [214.0, 72.0, 30.0]),
            ([232.0, 118.0, 32.0], [240.0, 236.0, 226.0]),
            ([238.0, 178.0, 60.0], [245.0, 214.0, 120.0]),
            ([235.0, 232.0, 222.0], [30.0, 30.0, 34.0]),
            ([220.0, 84.0, 28.0], [250.0, 160.0, 60.0]),
        ];
        let koi = palettes
            .iter()
            .map(|&(body, patch)| Koi {
                x: w as f32 * (0.2 + 0.6 * rng.f32()),
                y: h as f32 * (0.2 + 0.6 * rng.f32()),
                heading: rng.f32() * TAU,
                speed: 8.0,
                wander: rng.f32() * TAU,
                phase: rng.f32() * TAU,
                body,
                patch,
                seed: rng.f32() * 100.0,
            })
            .collect();
        Pond {
            w,
            h,
            cur: vec![0.0; w * h],
            prev: vec![0.0; w * h],
            layer: base.clone(),
            base,
            px: vec![[0; 3]; w * h],
            koi,
            food: Vec::new(),
            rng,
        }
    }

    fn splash(&mut self, x: f32, y: f32, strength: f32, radius: f32) {
        let r = radius.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (x as i32 + dx, y as i32 + dy);
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                if d < radius && px > 0 && py > 0 && (px as usize) < self.w - 1 && (py as usize) < self.h - 1 {
                    self.cur[py as usize * self.w + px as usize] -= strength * 0.5 * (1.0 + (PI * d / radius).cos());
                }
            }
        }
    }

    fn drop_food(&mut self, x: f32, y: f32) {
        for _ in 0..4 {
            let fx = (x + (self.rng.f32() - 0.5) * 6.0).clamp(1.0, self.w as f32 - 2.0);
            let fy = (y + (self.rng.f32() - 0.5) * 6.0).clamp(1.0, self.h as f32 - 2.0);
            self.food.push(Food { x: fx, y: fy, age: 0.0 });
        }
        self.splash(x, y, 10.0, 3.0);
    }

    fn step(&mut self) {
        let w = self.w;
        for y in 1..self.h.saturating_sub(1) {
            for x in 1..w - 1 {
                let i = y * w + x;
                let sum = self.cur[i - 1] + self.cur[i + 1] + self.cur[i - w] + self.cur[i + w];
                self.prev[i] = (sum * 0.5 - self.prev[i]) * DAMPING;
            }
        }
        std::mem::swap(&mut self.cur, &mut self.prev);

        let (fw, fh) = (w as f32, self.h as f32);
        for k in 0..self.koi.len() {
            let (x, y) = (self.koi[k].x, self.koi[k].y);
            let mut sep = (0.0, 0.0);
            for (j, other) in self.koi.iter().enumerate() {
                let (dx, dy) = (x - other.x, y - other.y);
                let d2 = dx * dx + dy * dy;
                if j != k && d2 < 144.0 && d2 > 0.01 {
                    sep.0 += dx / d2;
                    sep.1 += dy / d2;
                }
            }
            let nearest = self
                .food
                .iter()
                .enumerate()
                .map(|(i, f)| (i, (f.x - x).hypot(f.y - y)))
                .filter(|&(_, d)| d < 70.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));

            self.koi[k].wander += (self.rng.f32() - 0.5) * 0.12;
            let koi = &mut self.koi[k];
            let (mut dx, mut dy, target_speed, turn_rate) = match nearest {
                Some((i, d)) => ((self.food[i].x - x) / d, (self.food[i].y - y) / d, 22.0, 3.5),
                None => (koi.wander.cos(), koi.wander.sin(), 8.0, 1.2),
            };
            let margin = 16.0;
            dx += ((margin - x).max(0.0) - (x - (fw - margin)).max(0.0)) / margin * 2.0 + sep.0 * 6.0;
            dy += ((margin - y).max(0.0) - (y - (fh - margin)).max(0.0)) / margin * 2.0 + sep.1 * 6.0;
            if nearest.is_none() && (x < margin || y < margin || x > fw - margin || y > fh - margin) {
                koi.wander = dy.atan2(dx);
            }
            let diff = (dy.atan2(dx) - koi.heading + PI).rem_euclid(TAU) - PI;
            koi.heading += diff.clamp(-turn_rate * DT, turn_rate * DT);
            koi.speed += (target_speed - koi.speed) * (1.0 - (-2.0 * DT).exp());
            koi.x = (koi.x + koi.heading.cos() * koi.speed * DT).clamp(1.0, fw - 2.0);
            koi.y = (koi.y + koi.heading.sin() * koi.speed * DT).clamp(1.0, fh - 2.0);
            koi.phase += DT * (4.0 + koi.speed * 0.35);

            let tail = (koi.x - koi.heading.cos() * KOI_LEN, koi.y - koi.heading.sin() * KOI_LEN);
            let head = (koi.x + koi.heading.cos() * KOI_LEN, koi.y + koi.heading.sin() * KOI_LEN);
            let wake = 0.012 * koi.speed;
            if tail.0 > 1.0 && tail.1 > 1.0 && tail.0 < fw - 2.0 && tail.1 < fh - 2.0 {
                self.cur[tail.1 as usize * w + tail.0 as usize] -= wake;
            }
            if let Some((i, _)) = nearest {
                if (self.food[i].x - head.0).hypot(self.food[i].y - head.1) < 2.5 {
                    self.food.swap_remove(i);
                    self.splash(head.0, head.1, 2.0, 2.0);
                }
            }
        }

        for f in &mut self.food {
            f.age += DT;
        }
        self.food.retain(|f| f.age < 25.0);
    }

    fn draw(&mut self) {
        let (w, h) = (self.w, self.h);
        self.layer.copy_from_slice(&self.base);
        for koi in &self.koi {
            let (c, s) = (koi.heading.cos(), koi.heading.sin());
            let r = (KOI_LEN + TAIL_LEN + 3.0) as i32;
            for py in (koi.y as i32 - r).max(0)..(koi.y as i32 + r).min(h as i32) {
                for px in (koi.x as i32 - r).max(0)..(koi.x as i32 + r).min(w as i32) {
                    let (dx, dy) = (px as f32 + 0.5 - koi.x, py as f32 + 0.5 - koi.y);
                    let u = dx * c + dy * s;
                    let bend_t = ((KOI_LEN * 0.4 - u).max(0.0) / (KOI_LEN * 1.4 + TAIL_LEN)).powi(2);
                    let v = -dx * s + dy * c - 3.0 * bend_t * (koi.phase + u * 0.5).sin();
                    let (half, opacity) = if u.abs() < KOI_LEN {
                        (KOI_WIDTH * (1.0 - (u / KOI_LEN).powi(2)).sqrt(), 0.92)
                    } else if u < -KOI_LEN + 1.0 && u > -KOI_LEN - TAIL_LEN {
                        (0.7 + 1.8 * (-KOI_LEN - u) / TAIL_LEN, 0.7)
                    } else {
                        continue;
                    };
                    let cover = (half - v.abs() + 0.5).clamp(0.0, 1.0) * opacity;
                    if cover <= 0.0 {
                        continue;
                    }
                    let pattern = (u * 0.8 + koi.seed).sin() * (v * 1.1 + koi.seed * 2.0).sin() + (u * 0.35 + koi.seed * 3.0).sin();
                    let color = if pattern > 0.4 { koi.patch } else { koi.body };
                    let dst = &mut self.layer[py as usize * w + px as usize];
                    *dst = [0, 1, 2].map(|i| dst[i] + (color[i] - dst[i]) * cover);
                }
            }
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let sx = self.cur[if x > 0 { i - 1 } else { i }] - self.cur[if x + 1 < w { i + 1 } else { i }];
                let sy = self.cur[if y > 0 { i - w } else { i }] - self.cur[if y + 1 < h { i + w } else { i }];
                let light = sx * 0.6 + sy * 0.8;
                let spec = (light - 1.2).max(0.0) * 40.0;
                let col = self.layer[i];
                self.px[i] = [
                    (col[0] + light * 9.0 + spec * 0.9).clamp(0.0, 255.0) as u8,
                    (col[1] + light * 11.0 + spec).clamp(0.0, 255.0) as u8,
                    (col[2] + light * 11.0 + spec * 0.95).clamp(0.0, 255.0) as u8,
                ];
            }
        }
        for f in &self.food {
            let fade = (1.0 - (f.age - 20.0).max(0.0) / 5.0).clamp(0.0, 1.0);
            let dst = &mut self.px[f.y as usize * w + f.x as usize];
            *dst = [0, 1, 2].map(|i| (dst[i] as f32 + ([200.0, 162.0, 98.0][i] - dst[i] as f32) * fade) as u8);
        }
    }
}

fn build_frame(pond: &mut Pond, cols: usize, overlay: Option<&str>, cells: &mut Vec<Cell>, prev: &mut Vec<Cell>, out: &mut Vec<u8>) {
    pond.draw();
    cells.clear();
    for r in 0..pond.h / 2 {
        for c in 0..cols {
            cells.push(Cell { ch: '▀', fg: pond.px[2 * r * cols + c], bg: pond.px[(2 * r + 1) * cols + c] });
        }
    }
    if let Some(text) = overlay {
        for (i, ch) in text.chars().take(cols).enumerate() {
            cells[i] = Cell { ch, fg: [235, 235, 225], bg: [8, 28, 34] };
        }
    }

    out.clear();
    out.extend_from_slice(b"\x1b[?2026h");
    if prev.len() != cells.len() {
        prev.clear();
        prev.resize(cells.len(), Cell { ch: '\0', fg: [0; 3], bg: [0; 3] });
        out.extend_from_slice(b"\x1b[0m\x1b[2J");
    }
    let (mut at, mut fg, mut bg) = (usize::MAX, None, None);
    let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|k| a[k].abs_diff(b[k]) <= COLOR_TOLERANCE);
    for (i, (cell, old)) in cells.iter().zip(prev.iter_mut()).enumerate() {
        if cell.ch == old.ch && near(cell.fg, old.fg) && near(cell.bg, old.bg) {
            continue;
        }
        if at != i {
            write!(out, "\x1b[{};{}H", i / cols + 1, i % cols + 1).unwrap();
        }
        if fg != Some(cell.fg) {
            write!(out, "\x1b[38;2;{};{};{}m", cell.fg[0], cell.fg[1], cell.fg[2]).unwrap();
            fg = Some(cell.fg);
        }
        if bg != Some(cell.bg) {
            write!(out, "\x1b[48;2;{};{};{}m", cell.bg[0], cell.bg[1], cell.bg[2]).unwrap();
            bg = Some(cell.bg);
        }
        let mut utf8 = [0; 4];
        out.extend_from_slice(cell.ch.encode_utf8(&mut utf8).as_bytes());
        at = if (i + 1) % cols == 0 { usize::MAX } else { i + 1 };
        *old = *cell;
    }
    out.extend_from_slice(b"\x1b[?2026l");
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn run_bench(frames: usize, cols: usize, rows: usize) {
    let mut pond = Pond::new(cols, rows * 2, 7);
    let (mut cells, mut prev, mut out) = (Vec::new(), Vec::new(), Vec::new());
    let mut sink = io::sink();
    let mut times = Vec::with_capacity(frames);
    let mut bytes = Vec::with_capacity(frames);
    for f in 0..frames {
        let start = Instant::now();
        if f % 120 == 0 {
            let (x, y) = (pond.rng.f32() * cols as f32, pond.rng.f32() * rows as f32 * 2.0);
            pond.drop_food(x, y);
        }
        pond.step();
        build_frame(&mut pond, cols, None, &mut cells, &mut prev, &mut out);
        sink.write_all(&out).unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        bytes.push(out.len());
    }
    let avg_ms = times.iter().sum::<f64>() / frames as f64;
    times.sort_by(f64::total_cmp);
    let steady = &bytes[1..];
    println!("size {cols}x{rows} cells ({}x{} px), {frames} frames", cols, rows * 2);
    println!("frame build: avg {avg_ms:.3} ms  p50 {:.3} ms  p99 {:.3} ms  max {:.3} ms", percentile(&times, 0.5), percentile(&times, 0.99), times[frames - 1]);
    println!(
        "bytes/frame: first (full repaint) {}  avg after first {}  max after first {}  => {:.1} MB/s at 60 fps",
        bytes[0],
        steady.iter().sum::<usize>() / steady.len().max(1),
        steady.iter().max().unwrap_or(&0),
        steady.iter().sum::<usize>() as f64 / steady.len().max(1) as f64 * 60.0 / 1e6
    );
}

fn restore_terminal() {
    let _ = execute!(io::stdout(), event::DisableMouseCapture, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = io::stdout().write_all(b"\x1b[0m");
    let _ = terminal::disable_raw_mode();
}

fn run_interactive() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide, event::EnableMouseCapture)?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info)
    }));

    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
    let (c, r) = terminal::size()?;
    let (mut cols, mut pond) = (c as usize, Pond::new(c as usize, r as usize * 2, seed));
    let (mut cells, mut prev, mut out) = (Vec::new(), Vec::new(), Vec::new());
    let mut stdout = io::stdout().lock();
    let mut show_overlay = false;
    let mut work_ms: Vec<f64> = Vec::new();
    let (mut fps, mut frames_in_second, mut second_start) = (0.0, 0, Instant::now());
    let frame = Duration::from_secs_f32(DT);
    let mut last = Instant::now();
    let mut acc = Duration::ZERO;
    let mut next_frame = Instant::now();

    'main: loop {
        let now = Instant::now();
        if now < next_frame {
            if !event::poll(next_frame - now)? {
                continue;
            }
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break 'main,
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break 'main,
                    KeyCode::Char('d') => show_overlay = !show_overlay,
                    KeyCode::Char('f') => {
                        let (x, y) = (pond.rng.f32() * pond.w as f32, pond.rng.f32() * pond.h as f32);
                        pond.drop_food(x, y);
                    }
                    _ => {}
                },
                Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                    pond.drop_food(m.column as f32 + 0.5, m.row as f32 * 2.0 + 1.0);
                }
                Event::Resize(c, r) => {
                    let koi = std::mem::take(&mut pond.koi);
                    cols = c as usize;
                    pond = Pond::new(cols, r as usize * 2, seed);
                    for mut k in koi {
                        k.x = k.x.clamp(1.0, cols as f32 - 2.0);
                        k.y = k.y.clamp(1.0, r as f32 * 2.0 - 2.0);
                        pond.koi.push(k);
                    }
                    prev.clear();
                }
                _ => {}
            }
            continue;
        }

        next_frame = (next_frame + frame).max(now);
        let start = Instant::now();
        acc += start - last;
        last = start;
        let mut steps = 0;
        while acc >= frame && steps < 4 {
            pond.step();
            acc -= frame;
            steps += 1;
        }
        acc = acc.min(frame);

        let overlay = show_overlay.then(|| {
            let mut sorted = work_ms.clone();
            sorted.sort_by(f64::total_cmp);
            let (p50, p99) = if sorted.is_empty() { (0.0, 0.0) } else { (percentile(&sorted, 0.5), percentile(&sorted, 0.99)) };
            format!(" fps {fps:.1}  frame p50 {p50:.2}ms p99 {p99:.2}ms  bytes {}  koi {}  food {} ", out.len(), pond.koi.len(), pond.food.len())
        });
        build_frame(&mut pond, cols, overlay.as_deref(), &mut cells, &mut prev, &mut out);
        stdout.write_all(&out)?;
        stdout.flush()?;

        if work_ms.len() == 120 {
            work_ms.remove(0);
        }
        work_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        frames_in_second += 1;
        if second_start.elapsed() >= Duration::from_secs(1) {
            fps = frames_in_second as f64 / second_start.elapsed().as_secs_f64();
            (frames_in_second, second_start) = (0, Instant::now());
        }
    }
    drop(stdout);
    restore_terminal();
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut bench: Option<usize> = None;
    let (mut cols, mut rows) = (160, 45);
    for i in 1..args.len() {
        match args[i].as_str() {
            "--bench" => bench = args.get(i + 1).and_then(|n| n.parse().ok()),
            "--size" => {
                if let Some((w, h)) = args.get(i + 1).and_then(|s| s.split_once('x')) {
                    (cols, rows) = (w.parse().unwrap_or(cols), h.parse().unwrap_or(rows));
                }
            }
            _ => {}
        }
    }
    match bench {
        Some(n) => {
            run_bench(n.max(2), cols, rows);
            Ok(())
        }
        None => run_interactive(),
    }
}
