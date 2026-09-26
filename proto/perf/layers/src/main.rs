mod pond;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use pond::{Body, Pond};
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::FromRawFd;
use std::time::{Duration, Instant};

const CACHE_PER_FISH: usize = 48;
const FADE_LEVELS: usize = 8;
const WATER_ID: u32 = 1;
const FIRST_PELLET_ID: u32 = 2;
const FIRST_EDGE_ID: u32 = 20;
const FIRST_SPRITE_ID: u32 = 100;

struct Sprite {
    angle: usize,
    phase: usize,
    id: u32,
    cols: i32,
    rows: i32,
    used: u64,
}

/// Cell column and row, pixel offset inside that cell, and size in cells (0 = native).
type Spot = [i32; 6];

/// Terminal-side state. The water is one image scaled up from a coarse grid. Each koi is
/// its own image at `fish_px` pixels per cell width, posed from a cache keyed by heading
/// and tail phase, so most frames only move placements. Food pellets are tiny native images.
struct Layers {
    cols: usize,
    rows: usize,
    cell_w: usize,
    cell_h: usize,
    fish_px: usize,
    angles: usize,
    phases: usize,
    scale_x: f32,
    scale_y: f32,
    pellet_size: usize,
    next_id: u32,
    bodies: Vec<Body>,
    sprites: Vec<Vec<Sprite>>,
    placed: Vec<Option<(u32, Spot)>>,
    pellets: Vec<(u32, Spot)>,
    shm_seq: u64,
    shm_live: VecDeque<u64>,
    buf: Vec<u8>,
    shm_bytes: usize,
    lookups: usize,
    renders: usize,
}

fn shm_name(seq: u64) -> String {
    format!("/koi-layers-{}-{}", std::process::id(), seq)
}

fn write_shm(name: &str, data: &[u8]) -> io::Result<()> {
    let cname = CString::new(name).unwrap();
    let fd = unsafe { libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe { File::from_raw_fd(fd) }.write_all(data)
}

fn water_size(cols: usize, rows: usize, cell_w: usize, cell_h: usize, px: usize) -> (usize, usize) {
    (cols * px, (rows * px * cell_h + cell_w / 2) / cell_w)
}

#[derive(Clone, Copy)]
struct Settings {
    water_px: usize,
    water_every: u64,
    fish_px: usize,
    angles: usize,
    phases: usize,
}

impl Layers {
    fn new(out: &mut Vec<u8>, pond: &Pond, cols: usize, rows: usize, cell_w: usize, cell_h: usize, settings: &Settings, shm_seq: u64) -> io::Result<Layers> {
        let fish_px = if settings.fish_px == 0 { cell_w } else { settings.fish_px };
        let scale_x = (cols * cell_w) as f32 / pond.w as f32;
        let mut layers = Layers {
            cols,
            rows,
            cell_w,
            cell_h,
            fish_px,
            angles: settings.angles,
            phases: settings.phases,
            scale_x,
            scale_y: (rows * cell_h) as f32 / pond.h as f32,
            pellet_size: 0,
            next_id: FIRST_SPRITE_ID,
            bodies: (0..pond.fish.len()).map(|k| pond.render_body(k, scale_x * fish_px as f32 / cell_w as f32)).collect(),
            sprites: pond.fish.iter().map(|_| Vec::new()).collect(),
            placed: pond.fish.iter().map(|_| None).collect(),
            pellets: Vec::new(),
            shm_seq,
            shm_live: VecDeque::new(),
            buf: Vec::new(),
            shm_bytes: 0,
            lookups: 0,
            renders: 0,
        };
        let mut pixels = Vec::new();
        for level in 1..=FADE_LEVELS {
            let size = pond.render_pellet(level as f32 / FADE_LEVELS as f32, scale_x, &mut pixels);
            layers.pellet_size = size;
            let id = FIRST_PELLET_ID + level as u32 - 1;
            layers.send(out, &pixels, &format!("a=t,f=32,s={size},v={size},i={id}"))?;
        }
        Ok(layers)
    }

    fn send(&mut self, out: &mut Vec<u8>, data: &[u8], keys: &str) -> io::Result<()> {
        self.shm_seq += 1;
        let name = shm_name(self.shm_seq);
        write_shm(&name, data)?;
        self.shm_live.push_back(self.shm_seq);
        self.shm_bytes += data.len();
        out.extend_from_slice(format!("\x1b_G{keys},t=s,S={},q=2;{}\x1b\\", data.len(), B64.encode(&name)).as_bytes());
        Ok(())
    }

    /// The terminal unlinks each shared-memory object once it reads it. This removes the
    /// ones it never read, keeping the newest `keep`.
    fn unlink_shm(&mut self, keep: usize) {
        while self.shm_live.len() > keep {
            let seq = self.shm_live.pop_front().unwrap();
            unsafe { libc::shm_unlink(CString::new(shm_name(seq)).unwrap().as_ptr()) };
        }
    }

    /// Poses fish `k` into `buf` on a canvas that starts at (`left`, `top`) sprite pixels from
    /// its centre and is padded to whole cells, so a c/r placement scales it without
    /// stretching. Transmits it as image `id` and returns its size in cells.
    fn transmit_pose(&mut self, out: &mut Vec<u8>, k: usize, id: u32, angle: f32, phase: f32, left: i32, top: i32, right: i32, bottom: i32) -> io::Result<(i32, i32)> {
        let screen_per_px = self.cell_w as f32 / self.fish_px as f32;
        let cols = (((right - left + 1) as f32 * screen_per_px) / self.cell_w as f32).ceil().max(1.0);
        let rows = (((bottom - top + 1) as f32 * screen_per_px) / self.cell_h as f32).ceil().max(1.0);
        let (w, h) = (cols as usize * self.fish_px, (rows * self.cell_h as f32 / screen_per_px).round() as usize);
        let mut pixels = std::mem::take(&mut self.buf);
        self.bodies[k].pose(angle, phase, left, top, w, h, &mut pixels);
        self.send(out, &pixels, &format!("a=t,f=32,s={w},v={h},i={id}"))?;
        self.buf = pixels;
        self.renders += 1;
        Ok((cols as i32, rows as i32))
    }

    fn spot(&self, x: i32, y: i32, cols: i32, rows: i32) -> Spot {
        let (x, y, cw, ch) = (x.max(0), y.max(0), self.cell_w as i32, self.cell_h as i32);
        [x / cw, y / ch, x % cw, y % ch, cols, rows]
    }

    fn encode(&mut self, out: &mut Vec<u8>, pond: &mut Pond, frame: u64, send_water: bool, overlay: Option<&str>) -> io::Result<()> {
        out.extend_from_slice(b"\x1b[?2026h");
        if send_water {
            pond.render_water();
            out.extend_from_slice(b"\x1b[H");
            let keys = format!("a=T,f=32,s={},v={},i={WATER_ID},p=1,c={},r={},z=-1000,C=1", pond.w, pond.h, self.cols, self.rows);
            self.send(out, &pond.out, &keys)?;
        }

        let screen_per_px = self.cell_w as f32 / self.fish_px as f32;
        for k in 0..pond.fish.len() {
            let f = &pond.fish[k];
            let angle = (f.vy.atan2(f.vx) / TAU * self.angles as f32).round().rem_euclid(self.angles as f32) as usize;
            let phase = (f.phase / TAU * self.phases as f32).round().rem_euclid(self.phases as f32) as usize;
            let (angle_rad, phase_rad) = (angle as f32 / self.angles as f32 * TAU, phase as f32 / self.phases as f32 * TAU);
            let (cx, cy) = (f.x * self.scale_x, f.y * self.scale_y);
            let (left, top, right, bottom) = self.bodies[k].bounds(angle_rad);
            let (x, y) = ((cx + left as f32 * screen_per_px).round() as i32, (cy + top as f32 * screen_per_px).round() as i32);
            self.lookups += 1;

            let (id, spot) = if x < 0 || y < 0 {
                let skip_x = (-x as f32 / screen_per_px).ceil().max(0.0) as i32;
                let skip_y = (-y as f32 / screen_per_px).ceil().max(0.0) as i32;
                let id = FIRST_EDGE_ID + k as u32;
                let (cols, rows) = self.transmit_pose(out, k, id, angle_rad, phase_rad, left + skip_x, top + skip_y, right, bottom)?;
                let (x, y) = ((cx + (left + skip_x) as f32 * screen_per_px).round() as i32, (cy + (top + skip_y) as f32 * screen_per_px).round() as i32);
                (id, self.spot(x, y, cols, rows))
            } else {
                let slot = match self.sprites[k].iter().position(|s| s.angle == angle && s.phase == phase) {
                    Some(slot) => slot,
                    None => {
                        let id = self.next_id;
                        self.next_id += 1;
                        let (cols, rows) = self.transmit_pose(out, k, id, angle_rad, phase_rad, left, top, right, bottom)?;
                        let sprite = Sprite { angle, phase, id, cols, rows, used: frame };
                        if self.sprites[k].len() < CACHE_PER_FISH {
                            self.sprites[k].push(sprite);
                            self.sprites[k].len() - 1
                        } else {
                            let oldest = (0..CACHE_PER_FISH).min_by_key(|&i| self.sprites[k][i].used).unwrap();
                            out.extend_from_slice(format!("\x1b_Ga=d,d=I,i={},q=2\x1b\\", self.sprites[k][oldest].id).as_bytes());
                            self.sprites[k][oldest] = sprite;
                            oldest
                        }
                    }
                };
                self.sprites[k][slot].used = frame;
                let Sprite { id, cols, rows, .. } = self.sprites[k][slot];
                (id, self.spot(x, y, cols, rows))
            };

            if Some((id, spot)) == self.placed[k] {
                continue;
            }
            if let Some((old, _)) = self.placed[k] {
                if old != id {
                    out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={old},p=1,q=2\x1b\\").as_bytes());
                }
            }
            out.extend_from_slice(
                format!("\x1b[{};{}H\x1b_Ga=p,i={id},p=1,X={},Y={},c={},r={},z={},C=1,q=2\x1b\\", spot[1] + 1, spot[0] + 1, spot[2], spot[3], spot[4], spot[5], -100 + k as i32)
                    .as_bytes(),
            );
            self.placed[k] = Some((id, spot));
        }

        let half = self.pellet_size as i32 / 2;
        let pellets: Vec<(u32, Spot)> = pond
            .food
            .iter()
            .filter_map(|food| {
                let fade = (1.0 - (food.age - 20.0) / 5.0).clamp(0.0, 1.0);
                let level = (fade * FADE_LEVELS as f32).ceil() as u32;
                let (x, y) = ((food.x * self.scale_x).round() as i32 - half, (food.y * self.scale_y).round() as i32 - half);
                (level > 0).then_some((FIRST_PELLET_ID + level - 1, self.spot(x, y, 0, 0)))
            })
            .collect();
        if pellets != self.pellets {
            for id in FIRST_PELLET_ID..FIRST_PELLET_ID + FADE_LEVELS as u32 {
                out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\").as_bytes());
            }
            for (n, &(id, spot)) in pellets.iter().enumerate() {
                out.extend_from_slice(format!("\x1b[{};{}H\x1b_Ga=p,i={id},p={},X={},Y={},z=-10,C=1,q=2\x1b\\", spot[1] + 1, spot[0] + 1, n + 1, spot[2], spot[3]).as_bytes());
            }
            self.pellets = pellets;
        }

        out.extend_from_slice(b"\x1b[1;1H\x1b[2K");
        if let Some(text) = overlay {
            out.extend_from_slice(format!("\x1b[48;2;8;24;30m\x1b[38;2;200;228;224m {text} \x1b[0m").as_bytes());
        }
        out.extend_from_slice(b"\x1b[?2026l");
        Ok(())
    }
}

fn percentile(samples: &[f32], p: f32) -> f32 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn bench(frames: usize, cols: usize, rows: usize, settings: &Settings) -> io::Result<()> {
    let (cell_w, cell_h) = (12, 26);
    let Settings { water_px, water_every, fish_px, angles, phases } = *settings;
    let (w, h) = water_size(cols, rows, cell_w, cell_h, water_px);
    println!("{cols}x{rows} cells of {cell_w}x{cell_h} px, water {w}x{h} (--water-px {water_px}) every {water_every} frames, fish {fish_px} px/cell, {angles} angles x {phases} phases, {frames} frames");
    let mut sink = File::create("/dev/null")?;
    let mut out = Vec::new();
    let mut pond = Pond::new(w, h, water_px as f32 / 8.0, 5, 7);
    let mut layers = Layers::new(&mut out, &pond, cols, rows, cell_w, cell_h, settings, 0)?;
    layers.unlink_shm(0);
    let (mut sim, mut encode, mut bytes) = (Vec::new(), Vec::new(), 0usize);
    let (shm_start, lookups_start, renders_start) = (layers.shm_bytes, layers.lookups, layers.renders);
    for frame in 0..frames as u64 {
        if frame % 45 == 0 {
            pond.drop_food_random();
        }
        let t0 = Instant::now();
        pond.step();
        let t1 = Instant::now();
        out.clear();
        layers.encode(&mut out, &mut pond, frame, frame % water_every == 0, None)?;
        sink.write_all(&out)?;
        let t2 = Instant::now();
        layers.unlink_shm(0);
        sim.push((t1 - t0).as_secs_f32() * 1000.0);
        encode.push((t2 - t1).as_secs_f32() * 1000.0);
        bytes += out.len();
    }
    let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let seconds = frames as f32 / 60.0;
    let (lookups, renders) = (layers.lookups - lookups_start, layers.renders - renders_start);
    println!("sim avg {:.3} ms | render+encode avg {:.3} ms p99 {:.3} ms", avg(&sim), avg(&encode), percentile(&encode, 0.99));
    println!(
        "pty {} B/frame | shm {:.1} MB/s at 60 fps | sprite renders {:.1}/s, cache hits {:.0}%",
        bytes / frames,
        (layers.shm_bytes - shm_start) as f32 / seconds / 1e6,
        renders as f32 / seconds,
        100.0 * (lookups - renders) as f32 / lookups as f32
    );
    Ok(())
}

fn winsize() -> (usize, usize, usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) };
    (ws.ws_col as usize, ws.ws_row as usize, ws.ws_xpixel as usize, ws.ws_ypixel as usize)
}

fn read_input(timeout_ms: i32) -> Vec<u8> {
    let mut fds = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let mut buf = vec![0u8; 4096];
    if unsafe { libc::poll(&mut fds, 1, timeout_ms) } <= 0 {
        return Vec::new();
    }
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    buf.truncate(n.max(0) as usize);
    buf
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bench_frames = None;
    let mut size = None;
    let mut water_px = 2;
    let mut water_fps = 60;
    let mut fish_px = 8;
    let mut angles = 120;
    let mut phases = 12;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench" => bench_frames = it.next().and_then(|s| s.parse().ok()),
            "--size" => size = it.next().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))),
            "--water-px" => water_px = it.next().and_then(|s| s.parse().ok()).unwrap_or(water_px),
            "--water-fps" => water_fps = it.next().and_then(|s| s.parse().ok()).unwrap_or(water_fps),
            "--fish-px" => fish_px = it.next().and_then(|s| s.parse().ok()).unwrap_or(fish_px),
            "--angles" => angles = it.next().and_then(|s| s.parse().ok()).unwrap_or(angles),
            "--phases" => phases = it.next().and_then(|s| s.parse().ok()).unwrap_or(phases),
            _ => {
                eprintln!("usage: koi-layers [--water-px N] [--water-fps N] [--fish-px N (0 = native)] [--angles N] [--phases N] [--bench N [--size COLSxROWS]]");
                std::process::exit(2);
            }
        }
    }
    let settings = Settings { water_px, water_every: (60 / water_fps.clamp(1, 60)) as u64, fish_px, angles, phases };
    if let Some(frames) = bench_frames {
        let (cols, rows) = size.unwrap_or((155, 78));
        return bench(frames, cols, rows, &settings);
    }

    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { libc::tcgetattr(0, &mut original) };
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }

    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[2J")?;
    let mut out = Vec::new();
    let (mut cols, mut rows, xpixel, ypixel) = winsize();
    let cell = |cols: usize, rows: usize, xpixel: usize, ypixel: usize| if xpixel > 0 && ypixel > 0 { (xpixel / cols, ypixel / rows) } else { (10, 20) };
    let (cell_w, cell_h) = cell(cols, rows, xpixel, ypixel);
    let (w, h) = water_size(cols, rows, cell_w, cell_h, water_px);
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let mut pond = Pond::new(w, h, water_px as f32 / 8.0, 5, seed);
    let mut layers = Layers::new(&mut out, &pond, cols, rows, cell_w, cell_h, &settings, 0)?;
    stdout.write_all(&out)?;
    let mut show_overlay = false;
    let mut build_ms: VecDeque<f32> = VecDeque::new();
    let (mut fps, mut fps_frames, mut fps_since) = (0.0, 0, Instant::now());
    let (mut shm_rate, mut render_rate, mut shm_mark, mut render_mark) = (0.0, 0.0, 0, 0);
    let mut last_bytes = 0;
    let step = Duration::from_secs_f32(pond::DT);
    let mut sim_time = Instant::now();
    let mut deadline = Instant::now();

    let mut frame_id = 0u64;
    'frames: loop {
        frame_id += 1;
        let input = read_input(0);
        let mut i = 0;
        while i < input.len() {
            match input[i] {
                b'q' | 3 => break 'frames,
                b'd' => show_overlay = !show_overlay,
                b'f' => pond.drop_food_random(),
                0x1b if input[i..].starts_with(b"\x1b[<") => {
                    let end = input[i..].iter().position(|&b| b == b'M' || b == b'm').map_or(input.len(), |e| i + e);
                    let fields: Vec<usize> = String::from_utf8_lossy(&input[i + 3..end]).split(';').filter_map(|s| s.parse().ok()).collect();
                    if let [0, x, y] = fields[..] {
                        if input.get(end) == Some(&b'M') {
                            pond.drop_food((x as f32 - 0.5) * pond.w as f32 / cols as f32, (y as f32 - 0.5) * pond.h as f32 / rows as f32);
                        }
                    }
                    i = end;
                }
                _ => {}
            }
            i += 1;
        }

        let (new_cols, new_rows, xpixel, ypixel) = winsize();
        if (new_cols, new_rows) != (cols, rows) {
            (cols, rows) = (new_cols, new_rows);
            let (cell_w, cell_h) = cell(cols, rows, xpixel, ypixel);
            let (w, h) = water_size(cols, rows, cell_w, cell_h, water_px);
            pond = Pond::new(w, h, water_px as f32 / 8.0, 5, frame_id);
            out.clear();
            out.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[2J");
            layers.unlink_shm(0);
            layers = Layers::new(&mut out, &pond, cols, rows, cell_w, cell_h, &settings, layers.shm_seq)?;
            stdout.write_all(&out)?;
        }

        let started = Instant::now();
        let mut steps = 0;
        while sim_time + step <= started && steps < 4 {
            pond.step();
            sim_time += step;
            steps += 1;
        }
        if steps == 4 {
            sim_time = started;
        }
        let overlay = show_overlay.then(|| {
            let samples: Vec<f32> = build_ms.iter().copied().collect();
            format!(
                "{fps:.1} fps | build p50 {:.2}ms p99 {:.2}ms | {last_bytes} B/frame | shm {shm_rate:.1} MB/s | sprites {render_rate:.0}/s | water {}x{} every {}",
                percentile(&samples, 0.5), percentile(&samples, 0.99), pond.w, pond.h, settings.water_every
            )
        });
        out.clear();
        layers.encode(&mut out, &mut pond, frame_id, frame_id % settings.water_every == 0, overlay.as_deref())?;
        stdout.write_all(&out)?;
        stdout.flush()?;
        last_bytes = out.len();
        layers.unlink_shm(256);

        build_ms.push_back(started.elapsed().as_secs_f32() * 1000.0);
        if build_ms.len() > 120 {
            build_ms.pop_front();
        }
        fps_frames += 1;
        if fps_since.elapsed() >= Duration::from_secs(1) {
            let seconds = fps_since.elapsed().as_secs_f32();
            fps = fps_frames as f32 / seconds;
            shm_rate = (layers.shm_bytes - shm_mark) as f32 / seconds / 1e6;
            render_rate = (layers.renders - render_mark) as f32 / seconds;
            (shm_mark, render_mark) = (layers.shm_bytes, layers.renders);
            (fps_frames, fps_since) = (0, Instant::now());
        }

        deadline += step;
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else {
            deadline = now;
        }
    }

    stdout.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l")?;
    stdout.flush()?;
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
    layers.unlink_shm(0);
    Ok(())
}
