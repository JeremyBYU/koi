mod gpu;
mod pond;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use gpu::Gpu;
use pond::Pond;
use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static HANGUP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_hangup(_: libc::c_int) {
    HANGUP.store(true, Ordering::Relaxed);
}

fn shm_name(id: u64) -> String {
    format!("/koi-{}-{}", std::process::id(), id)
}

const RING: usize = 8;

/// Frame pages that stay faulted in: each frame is copied into the next of RING shm files, which is
/// then hard-linked under that frame's name. Ghostty unlinks the frame name after reading, but the
/// ring's own name keeps the file alive, so no frame pays for zeroing and faulting fresh pages.
struct ShmRing {
    slots: Vec<(String, *mut u8)>,
    len: usize,
}

impl ShmRing {
    fn new(len: usize) -> io::Result<ShmRing> {
        let mut slots = Vec::new();
        for k in 0..RING {
            let path = format!("/dev/shm/koi-ring-{}-{len}-{k}", std::process::id());
            let file = OpenOptions::new().read(true).write(true).create(true).truncate(true).mode(0o600).open(&path)?;
            file.set_len(u64::try_from(len).unwrap())?;
            let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED | libc::MAP_POPULATE, file.as_raw_fd(), 0) };
            if ptr == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            slots.push((path, ptr.cast()));
        }
        Ok(ShmRing { slots, len })
    }

    fn write(&self, frame: u64, name: &str, data: &[u8]) -> io::Result<()> {
        let (path, ptr) = &self.slots[frame as usize % RING];
        unsafe { std::slice::from_raw_parts_mut(*ptr, self.len) }.copy_from_slice(data);
        std::fs::hard_link(path, format!("/dev/shm{name}"))
    }
}

impl Drop for ShmRing {
    fn drop(&mut self) {
        for (path, ptr) in &self.slots {
            unsafe { libc::munmap(ptr.cast(), self.len) };
            let _ = std::fs::remove_file(path);
        }
    }
}

fn image_size(cols: usize, rows: usize, xpixel: usize, ypixel: usize, px: usize) -> (usize, usize) {
    let aspect = if xpixel > 0 && ypixel > 0 { (ypixel as f32 / rows as f32) / (xpixel as f32 / cols as f32) } else { 2.0 };
    ((cols * px).next_multiple_of(4), (rows as f32 * px as f32 * aspect).round() as usize)
}

fn percentile(samples: &[f32], p: f32) -> f32 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn bench(frames: usize, cols: usize, rows: usize, px: usize, rgb: bool, dump: Option<&str>) -> io::Result<()> {
    let (w, h) = image_size(cols, rows, 0, 0, px);
    let mut gpu = Gpu::new(w, h, rgb);
    println!("{} | {cols}x{rows} cells -> {w}x{h} px image (--px {px}, f={}), {frames} frames", gpu.adapter, if rgb { 24 } else { 32 });
    let mut pond = Pond::new(w, h, 5, 7);
    let ring = ShmRing::new(w * h * if rgb { 3 } else { 4 })?;
    let (mut sim, mut wait, mut copy, mut total) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut last = Vec::new();
    for frame in 0..frames as u64 {
        if frame % 45 == 0 {
            pond.drop_food_random();
        }
        let t0 = Instant::now();
        pond.step();
        gpu.step(&pond.splashes);
        pond.splashes.clear();
        let t1 = Instant::now();
        let mut t2 = t1;
        let name = shm_name(frame);
        gpu.render(&pond, |data| {
            t2 = Instant::now();
            if dump.is_some() && frame + 1 == frames as u64 {
                last = data.to_vec();
            }
            ring.write(frame, &name, data)
        })?;
        let t3 = Instant::now();
        unsafe { libc::shm_unlink(CString::new(name).unwrap().as_ptr()) };
        sim.push((t1 - t0).as_secs_f32() * 1000.0);
        wait.push((t2 - t1).as_secs_f32() * 1000.0);
        copy.push((t3 - t2).as_secs_f32() * 1000.0);
        total.push((t3 - t0).as_secs_f32() * 1000.0);
    }
    let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    println!("step+submit ms  gpu shade+readback wait ms  copy to shm ms  total avg/p99 ms");
    println!("{:>14.3}  {:>26.3}  {:>14.3}  {:>6.3} / {:.3}", avg(&sim), avg(&wait), avg(&copy), avg(&total), percentile(&total, 0.99));
    if let Some(path) = dump {
        let rgb_bytes: Vec<u8> = if rgb { last } else { last.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect() };
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        ppm.extend_from_slice(&rgb_bytes);
        std::fs::write(path, ppm)?;
    }
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
    let mut rgb = true;
    let mut dump = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench" => bench_frames = it.next().and_then(|s| s.parse().ok()),
            "--size" => size = it.next().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))),
            "--px" => px = it.next().and_then(|s| s.parse().ok()).unwrap_or(px),
            "--fmt" => rgb = it.next().map(String::as_str) != Some("32"),
            "--dump" => dump = it.next().cloned(),
            _ => {
                eprintln!("usage: koi-gpu [--px N] [--fmt 24|32] [--bench N [--size WxH] [--dump out.ppm]]");
                std::process::exit(2);
            }
        }
    }
    if let Some(frames) = bench_frames {
        let (cols, rows) = size.unwrap_or((160, 45));
        return bench(frames, cols, rows, px, rgb, dump.as_deref());
    }

    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { libc::tcgetattr(0, &mut original) };
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }

    let handler: extern "C" fn(libc::c_int) = on_hangup;
    unsafe {
        libc::signal(libc::SIGHUP, handler as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
    }

    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[2J")?;
    let (mut cols, mut rows, xpixel, ypixel) = winsize();
    let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
    let format = if rgb { 24 } else { 32 };
    let mut gpu = Gpu::new(w, h, rgb);
    let mut ring = ShmRing::new(w * h * format / 8)?;
    let mut pond = Pond::new(w, h, 5, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64);
    let mut out = Vec::new();
    let mut show_overlay = false;
    let mut build_ms: VecDeque<f32> = VecDeque::new();
    let (mut fps, mut fps_frames, mut fps_since) = (0.0, 0, Instant::now());
    let step = Duration::from_secs_f32(pond::DT);
    let mut sim_time = Instant::now();
    let mut deadline = Instant::now();

    let mut frame_id = 0u64;
    'frames: loop {
        frame_id += 1;
        if HANGUP.load(Ordering::Relaxed) {
            break;
        }
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
            let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
            gpu = Gpu::new(w, h, rgb);
            ring = ShmRing::new(w * h * format / 8)?;
            pond = Pond::new(w, h, 5, frame_id);
            stdout.write_all(b"\x1b[2J")?;
        }

        let started = Instant::now();
        let mut steps = 0;
        while sim_time + step <= started && steps < 4 {
            pond.step();
            gpu.step(&pond.splashes);
            pond.splashes.clear();
            sim_time += step;
            steps += 1;
        }
        if steps == 4 {
            sim_time = started;
        }
        let overlay = show_overlay.then(|| {
            let samples: Vec<f32> = build_ms.iter().copied().collect();
            format!(
                "{fps:.1} fps | build p50 {:.2}ms p99 {:.2}ms | gpu {} f={} {}x{}",
                percentile(&samples, 0.5), percentile(&samples, 0.99), gpu.adapter, format, pond.w, pond.h
            )
        });
        let name = shm_name(frame_id);
        gpu.render(&pond, |data| ring.write(frame_id, &name, data))?;
        out.clear();
        out.extend_from_slice(
            format!("\x1b[?2026h\x1b[H\x1b_Ga=T,f={format},s={},v={},i=1,p=1,c={cols},r={rows},C=1,z=-1,q=2,t=s,S={};{}\x1b\\\x1b[1;1H\x1b[2K", pond.w, pond.h, ring.len, B64.encode(&name)).as_bytes(),
        );
        if let Some(text) = overlay {
            out.extend_from_slice(format!("\x1b[48;2;8;24;30m\x1b[38;2;200;228;224m {text} \x1b[0m").as_bytes());
        }
        out.extend_from_slice(b"\x1b[?2026l");
        stdout.write_all(&out)?;
        stdout.flush()?;
        if frame_id > 120 {
            unsafe { libc::shm_unlink(CString::new(shm_name(frame_id - 120)).unwrap().as_ptr()) };
        }

        build_ms.push_back(started.elapsed().as_secs_f32() * 1000.0);
        if build_ms.len() > 120 {
            build_ms.pop_front();
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

    let _ = stdout.write_all(b"\x1b_Ga=d,d=I,i=1,q=2\x1b\\\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l");
    let _ = stdout.flush();
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
    for id in frame_id.saturating_sub(120)..=frame_id {
        unsafe { libc::shm_unlink(CString::new(shm_name(id)).unwrap().as_ptr()) };
    }
    Ok(())
}
