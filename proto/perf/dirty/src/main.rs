mod pond;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use pond::Pond;
use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::FromRawFd;
use std::time::{Duration, Instant};

/// One Kitty image covering a fixed block of cells. Only tiles whose pixels changed are re-sent.
struct Tile {
    id: usize,
    col: usize,
    row: usize,
    cols: usize,
    rows: usize,
    rect: [usize; 4],
    sent: Vec<u8>,
    calm: bool,
    settled: bool,
}

fn make_tiles(cols: usize, rows: usize, px: usize, ppr: usize, (tile_cols, tile_rows): (usize, usize)) -> Vec<Tile> {
    let mut tiles = Vec::new();
    for row in (0..rows).step_by(tile_rows) {
        for col in (0..cols).step_by(tile_cols) {
            let (end_col, end_row) = ((col + tile_cols).min(cols), (row + tile_rows).min(rows));
            tiles.push(Tile {
                id: tiles.len() + 1,
                col,
                row,
                cols: end_col - col,
                rows: end_row - row,
                rect: [col * px, row * ppr, end_col * px, end_row * ppr],
                sent: Vec::new(),
                calm: false,
                settled: false,
            });
        }
    }
    tiles
}

fn shm_name(frame_id: u64, tile_id: usize) -> String {
    format!("/koi-dirty-{}-{frame_id}-{tile_id}", std::process::id())
}

fn write_shm(name: &str, data: &[u8]) -> io::Result<()> {
    let cname = CString::new(name).unwrap();
    let fd = unsafe { libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe { File::from_raw_fd(fd) }.write_all(data)
}

/// Shades every tile that could have changed and appends a transmit command for each one
/// whose pixels differ from what the terminal already holds. Returns (ids of tiles sent, pixel bytes sent).
fn send_tiles(out: &mut Vec<u8>, pond: &mut Pond, tiles: &mut [Tile], busy: &[[usize; 4]], water_stepped: bool, quant: i32, rgba: bool, frame_id: u64) -> io::Result<(Vec<usize>, usize)> {
    let margin = 12;
    let (mut ids, mut bytes) = (Vec::new(), 0);
    let mut pixels = Vec::new();
    for t in tiles.iter_mut() {
        let [x0, y0, x1, y1] = t.rect;
        let near_busy = busy.iter().any(|b| b[0] < x1 + margin && x0 < b[2] + margin && b[1] < y1 + margin && y0 < b[3] + margin);
        if water_stepped || near_busy {
            t.calm = pond.calm(t.rect, quant);
        }
        if !(t.sent.is_empty() || near_busy || (water_stepped && !(t.calm && t.settled))) {
            continue;
        }
        t.settled = t.calm && !near_busy;
        pond.render_rect(t.rect, quant, t.calm);
        pixels.clear();
        for y in y0..y1 {
            let row = &pond.out[(y * pond.w + x0) * 3..(y * pond.w + x1) * 3];
            if rgba {
                for p in row.chunks_exact(3) {
                    pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            } else {
                pixels.extend_from_slice(row);
            }
        }
        if pixels == t.sent {
            continue;
        }
        let name = shm_name(frame_id, t.id);
        write_shm(&name, &pixels)?;
        let (w, h, f) = (x1 - x0, y1 - y0, if rgba { 32 } else { 24 });
        let seq = if t.sent.is_empty() {
            format!("\x1b[{};{}H\x1b_Ga=T,f={f},s={w},v={h},i={},p=1,c={},r={},C=1,z=-1,q=2,t=s,S={};{}\x1b\\", t.row + 1, t.col + 1, t.id, t.cols, t.rows, pixels.len(), B64.encode(&name))
        } else {
            format!("\x1b_Ga=t,f={f},s={w},v={h},i={},q=2,t=s,S={};{}\x1b\\", t.id, pixels.len(), B64.encode(&name))
        };
        out.extend_from_slice(seq.as_bytes());
        bytes += pixels.len();
        ids.push(t.id);
        std::mem::swap(&mut t.sent, &mut pixels);
    }
    Ok((ids, bytes))
}

fn cell_pixels(xpixel: usize, ypixel: usize, cols: usize, rows: usize, px: usize) -> usize {
    if xpixel > 0 && ypixel > 0 {
        (px as f32 * (ypixel * cols) as f32 / (xpixel * rows) as f32).round() as usize
    } else {
        px * 2
    }
}

fn percentile(samples: &[f32], p: f32) -> f32 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn bench(frames: usize, cols: usize, rows: usize, px: usize, tile: (usize, usize), quant: i32, rgba: bool, water_every: u64) -> io::Result<()> {
    let ppr = cell_pixels(0, 0, cols, rows, px);
    let (w, h) = (cols * px, rows * ppr);
    let mut pond = Pond::new(w, h, 5, 7);
    let mut tiles = make_tiles(cols, rows, px, ppr, tile);
    println!("{cols}x{rows} cells -> {w}x{h} px, {} tiles of {}x{} cells, quant {quant}, water every {water_every} ticks, {frames} frames", tiles.len(), tile.0, tile.1);
    let mut sink = File::create("/dev/null")?;
    let mut out = Vec::new();
    let (mut sim, mut render, mut sent, mut sent_bytes) = (Vec::new(), Vec::new(), 0, 0);
    let mut prev_busy = Vec::new();
    for frame in 1..=frames as u64 {
        if frame % 45 == 0 {
            pond.drop_food_random();
        }
        let t0 = Instant::now();
        pond.step_life();
        let water_stepped = frame % water_every == 0;
        if water_stepped {
            for _ in 0..water_every {
                pond.step_water();
            }
        }
        let t1 = Instant::now();
        pond.render_layer();
        let busy = pond.busy_rects();
        let mut near = busy.clone();
        near.extend_from_slice(&prev_busy);
        out.clear();
        let (ids, bytes) = send_tiles(&mut out, &mut pond, &mut tiles, &near, water_stepped, quant, rgba, frame)?;
        sink.write_all(&out)?;
        let t2 = Instant::now();
        for &id in &ids {
            unsafe { libc::shm_unlink(CString::new(shm_name(frame, id)).unwrap().as_ptr()) };
        }
        prev_busy = busy;
        sim.push((t1 - t0).as_secs_f32() * 1000.0);
        render.push((t2 - t1).as_secs_f32() * 1000.0);
        sent += ids.len();
        sent_bytes += bytes;
    }
    let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    println!(
        "sim avg {:.2} ms p99 {:.2} | render+send avg {:.2} ms p99 {:.2} | {:.1} of {} tiles/frame | {:.0} KB/frame (full image {} KB)",
        avg(&sim), percentile(&sim, 0.99), avg(&render), percentile(&render, 0.99), sent as f32 / frames as f32, tiles.len(), sent_bytes as f32 / frames as f32 / 1024.0, w * h * 3 / 1024
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
    let mut px = 8;
    let mut tile = (16, 8);
    let mut quant = 4;
    let mut water_hz = 30;
    let mut rgba = false;
    let mut it = args.iter();
    let parse_pair = |s: Option<&String>| s.and_then(|s| s.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench" => bench_frames = it.next().and_then(|s| s.parse().ok()),
            "--size" => size = parse_pair(it.next()),
            "--px" => px = it.next().and_then(|s| s.parse().ok()).unwrap_or(px),
            "--tile" => tile = parse_pair(it.next()).unwrap_or(tile),
            "--quant" => quant = it.next().and_then(|s| s.parse().ok()).unwrap_or(quant).max(1),
            "--rgba" => rgba = true,
            "--water-hz" => water_hz = it.next().and_then(|s| s.parse().ok()).unwrap_or(water_hz).clamp(1, 60),
            _ => {
                eprintln!("usage: koi-dirty [--px N] [--tile CxR] [--quant N] [--water-hz 60|30|20|15] [--rgba] [--bench N [--size WxH]]");
                std::process::exit(2);
            }
        }
    }
    let water_every = 60 / water_hz;
    if let Some(frames) = bench_frames {
        let (cols, rows) = size.unwrap_or((160, 45));
        return bench(frames, cols, rows, px, tile, quant, rgba, water_every);
    }

    // Closing the window hangs up the tty; ignoring SIGHUP turns that into a write error so the shm cleanup below still runs.
    unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { libc::tcgetattr(0, &mut original) };
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }

    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[2J")?;
    let (mut cols, mut rows, xpixel, ypixel) = winsize();
    let ppr = cell_pixels(xpixel, ypixel, cols, rows, px);
    let mut pond = Pond::new(cols * px, rows * ppr, 5, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64);
    let mut tiles = make_tiles(cols, rows, px, ppr, tile);
    let mut prev_busy = Vec::new();
    let mut out = Vec::new();
    let mut show_overlay = false;
    let mut build_ms: VecDeque<f32> = VecDeque::new();
    let mut sent_history: VecDeque<(usize, usize)> = VecDeque::new();
    let mut unread: VecDeque<(u64, Vec<usize>)> = VecDeque::new();
    let (mut fps, mut fps_frames, mut fps_since) = (0.0, 0, Instant::now());
    let step = Duration::from_secs_f32(pond::DT);
    let mut sim_time = Instant::now();
    let mut deadline = Instant::now();
    let mut ticks = 0u64;

    let mut frame_id = 0u64;
    'frames: loop {
        frame_id += 1;
        out.clear();
        let input = read_input(0);
        let mut i = 0;
        while i < input.len() {
            match input[i] {
                b'q' | 3 => break 'frames,
                b'd' => {
                    show_overlay = !show_overlay;
                    out.extend_from_slice(b"\x1b[1;1H\x1b[2K");
                }
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
            let ppr = cell_pixels(xpixel, ypixel, cols, rows, px);
            pond = Pond::new(cols * px, rows * ppr, 5, frame_id);
            tiles = make_tiles(cols, rows, px, ppr, tile);
            prev_busy.clear();
            out.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[2J");
        }

        let started = Instant::now();
        let mut steps = 0;
        let mut water_stepped = false;
        while sim_time + step <= started && steps < 4 {
            pond.step_life();
            ticks += 1;
            if ticks % water_every == 0 {
                for _ in 0..water_every {
                    pond.step_water();
                }
                water_stepped = true;
            }
            sim_time += step;
            steps += 1;
        }
        if steps == 4 {
            sim_time = started;
        }
        pond.render_layer();
        let busy = pond.busy_rects();
        let mut near = busy.clone();
        near.extend_from_slice(&prev_busy);
        prev_busy = busy;
        let (ids, bytes) = send_tiles(&mut out, &mut pond, &mut tiles, &near, water_stepped, quant, rgba, frame_id)?;
        sent_history.push_back((ids.len(), bytes));
        unread.push_back((frame_id, ids));
        if show_overlay {
            let samples: Vec<f32> = build_ms.iter().copied().collect();
            let (avg_tiles, avg_kb) = sent_history.iter().fold((0.0, 0.0), |(t, b), &(c, n)| (t + c as f32, b + n as f32 / 1024.0));
            let n = sent_history.len().max(1) as f32;
            let text = format!(
                "{fps:.1} fps | build p50 {:.2}ms p99 {:.2}ms | {:.1}/{} tiles {:.0} KB per frame | {}x{} quant {quant} water {water_hz} Hz",
                percentile(&samples, 0.5), percentile(&samples, 0.99), avg_tiles / n, tiles.len(), avg_kb / n, pond.w, pond.h
            );
            out.extend_from_slice(format!("\x1b[1;1H\x1b[2K\x1b[48;2;8;24;30m\x1b[38;2;200;228;224m {text} \x1b[0m").as_bytes());
        }
        if !out.is_empty() {
            out.splice(0..0, b"\x1b[?2026h".iter().copied());
            out.extend_from_slice(b"\x1b[?2026l");
            if stdout.write_all(&out).and_then(|()| stdout.flush()).is_err() {
                break;
            }
        }

        build_ms.push_back(started.elapsed().as_secs_f32() * 1000.0);
        if build_ms.len() > 120 {
            build_ms.pop_front();
            sent_history.pop_front();
            if let Some((old_frame, old_ids)) = unread.pop_front() {
                for id in old_ids {
                    unsafe { libc::shm_unlink(CString::new(shm_name(old_frame, id)).unwrap().as_ptr()) };
                }
            }
        }
        fps_frames += 1;
        if fps_since.elapsed() >= Duration::from_secs(1) {
            fps = fps_frames as f32 / fps_since.elapsed().as_secs_f32();
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

    let _ = stdout.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l").and_then(|()| stdout.flush());
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
    for (old_frame, old_ids) in unread {
        for id in old_ids {
            unsafe { libc::shm_unlink(CString::new(shm_name(old_frame, id)).unwrap().as_ptr()) };
        }
    }
    Ok(())
}
