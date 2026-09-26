mod pond;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use flate2::{write::ZlibEncoder, Compression};
use pond::Pond;
use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Medium {
    Shm,
    Ring,
    TempFile,
    Direct,
    DirectZlib,
}

impl Medium {
    const ALL: [Medium; 5] = [Medium::Shm, Medium::Ring, Medium::TempFile, Medium::Direct, Medium::DirectZlib];

    fn name(self) -> &'static str {
        match self {
            Medium::Shm => "shm",
            Medium::Ring => "ring",
            Medium::TempFile => "file",
            Medium::Direct => "direct",
            Medium::DirectZlib => "zlib",
        }
    }
}

fn push_seq(out: &mut Vec<u8>, seq: &[u8], tmux: bool) {
    if !tmux {
        out.extend_from_slice(seq);
        return;
    }
    out.extend_from_slice(b"\x1bPtmux;");
    for &b in seq {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
}

fn shm_name(id: u64) -> String {
    format!("/koi-{}-{}", std::process::id(), id)
}

const RING: u64 = 3;

fn ring_name(frame_id: u64) -> String {
    format!("/koi-{}-ring{}", std::process::id(), frame_id % RING)
}

/// Persistent shm objects that frames cycle through. They are sent as plain files (`t=f`),
/// which the terminal reads without deleting, so no page is allocated or faulted after setup.
fn create_ring(len: usize) -> io::Result<Vec<ShmFrame>> {
    (0..RING).map(|slot| ShmFrame::create(&ring_name(slot), len)).collect()
}

fn unlink_ring() {
    for slot in 0..RING {
        unsafe { libc::shm_unlink(CString::new(ring_name(slot)).unwrap().as_ptr()) };
    }
}

fn temp_path(id: u64) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tty-graphics-protocol-koi-{}-{}.rgba", std::process::id(), id))
}

/// A POSIX shm object mapped into our memory, so a frame renders straight into it.
struct ShmFrame {
    ptr: *mut u8,
    len: usize,
}

impl ShmFrame {
    fn create(name: &str, len: usize) -> io::Result<ShmFrame> {
        let cname = CString::new(name).unwrap();
        let fd = unsafe { libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(len as u64)?;
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED | libc::MAP_POPULATE, fd, 0) };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(ShmFrame { ptr: ptr.cast(), len })
    }
}

impl std::ops::Deref for ShmFrame {
    type Target = [u32];
    fn deref(&self) -> &[u32] {
        unsafe { std::slice::from_raw_parts(self.ptr.cast(), self.len / 4) }
    }
}

impl std::ops::DerefMut for ShmFrame {
    fn deref_mut(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast(), self.len / 4) }
    }
}

impl Drop for ShmFrame {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr.cast(), self.len) };
    }
}

/// Transmits `rgba` as image id 1 and places it at the top-left over `cols`x`rows` cells,
/// replacing the previous frame's image and placement in one synchronized update.
/// For `Medium::Shm` and `Medium::Ring`, `rgba` must already live in the shm object named
/// `shm_name(frame_id)` or `ring_name(frame_id)`.
fn encode_frame(out: &mut Vec<u8>, rgba: &[u32], w: usize, h: usize, cols: usize, rows: usize, medium: Medium, frame_id: u64, tmux: bool, overlay: Option<&str>) -> io::Result<()> {
    out.clear();
    push_seq(out, b"\x1b[?2026h\x1b[H", tmux);
    let rgba: &[u8] = bytemuck::cast_slice(rgba);
    let keys = format!("a=T,f=32,s={w},v={h},i=1,p=1,c={cols},r={rows},C=1,z=-1,q=2");
    match medium {
        Medium::Shm => {
            let seq = format!("\x1b_G{keys},t=s,S={};{}\x1b\\", rgba.len(), B64.encode(shm_name(frame_id)));
            push_seq(out, seq.as_bytes(), tmux);
        }
        Medium::Ring => {
            // No S=: Ghostty's limited file read fails when the file is exactly S bytes long.
            let seq = format!("\x1b_G{keys},t=f;{}\x1b\\", B64.encode(format!("/dev/shm{}", ring_name(frame_id))));
            push_seq(out, seq.as_bytes(), tmux);
        }
        Medium::TempFile => {
            let path = temp_path(frame_id);
            std::fs::write(&path, rgba)?;
            let seq = format!("\x1b_G{keys},t=t,S={};{}\x1b\\", rgba.len(), B64.encode(path.to_str().unwrap()));
            push_seq(out, seq.as_bytes(), tmux);
        }
        Medium::Direct | Medium::DirectZlib => {
            let payload = if medium == Medium::DirectZlib {
                let mut z = ZlibEncoder::new(Vec::new(), Compression::fast());
                z.write_all(rgba)?;
                B64.encode(z.finish()?)
            } else {
                B64.encode(rgba)
            };
            let chunks: Vec<&[u8]> = payload.as_bytes().chunks(4096).collect();
            for (n, chunk) in chunks.iter().enumerate() {
                let more = if n + 1 < chunks.len() { 1 } else { 0 };
                let mut seq = if n == 0 {
                    let zlib = if medium == Medium::DirectZlib { ",o=z" } else { "" };
                    format!("\x1b_G{keys}{zlib},m={more};").into_bytes()
                } else {
                    format!("\x1b_Gm={more};").into_bytes()
                };
                seq.extend_from_slice(chunk);
                seq.extend_from_slice(b"\x1b\\");
                push_seq(out, &seq, tmux);
            }
        }
    }
    if tmux {
        push_seq(out, b"\x1b[?2026l", true);
    }
    out.extend_from_slice(b"\x1b[1;1H\x1b[2K");
    if let Some(text) = overlay {
        out.extend_from_slice(format!("\x1b[48;2;8;24;30m\x1b[38;2;200;228;224m {text} \x1b[0m").as_bytes());
    }
    if !tmux {
        out.extend_from_slice(b"\x1b[?2026l");
    }
    Ok(())
}

fn image_size(cols: usize, rows: usize, xpixel: usize, ypixel: usize, px: usize) -> (usize, usize) {
    let aspect = if xpixel > 0 && ypixel > 0 { (ypixel as f32 / rows as f32) / (xpixel as f32 / cols as f32) } else { 2.0 };
    (cols * px, (rows as f32 * px as f32 * aspect).round() as usize)
}

fn percentile(samples: &[f32], p: f32) -> f32 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

fn bench(frames: usize, cols: usize, rows: usize, px: usize, water: usize, threads: usize, only: Option<Medium>, dump: Option<&str>) -> io::Result<()> {
    let (w, h) = image_size(cols, rows, 0, 0, px);
    println!("{cols}x{rows} cells -> {w}x{h} px image (--px {px} --water {water} --threads {threads}), {frames} frames, RGBA {} KB", w * h * 4 / 1024);
    println!("medium  sim ms  render ms  encode+write avg/p99 ms  total avg/p99 ms  bytes/frame  MB/s at 60fps");
    let mut sink = File::create("/dev/null")?;
    let mut out = Vec::new();
    let mut frame_buf = vec![0u32; w * h];
    for medium in Medium::ALL.into_iter().filter(|m| only.is_none_or(|o| o == *m)) {
        let mut pond = Pond::new(w, h, 5, 7, water, threads);
        let mut ring = if medium == Medium::Ring { create_ring(w * h * 4)? } else { Vec::new() };
        let (mut sim, mut render, mut encode, mut total, mut bytes) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0usize);
        for frame in 0..frames as u64 {
            if frame % 45 == 0 {
                pond.drop_food_random();
            }
            let t0 = Instant::now();
            pond.step();
            let t1 = Instant::now();
            let mut shm;
            let rgba: &mut [u32] = match medium {
                Medium::Shm => {
                    shm = ShmFrame::create(&shm_name(frame), w * h * 4)?;
                    &mut shm
                }
                Medium::Ring => &mut ring[(frame % RING) as usize],
                _ => &mut frame_buf,
            };
            pond.render(rgba);
            let t2 = Instant::now();
            encode_frame(&mut out, rgba, w, h, cols, rows, medium, frame, false, None)?;
            sink.write_all(&out)?;
            let t3 = Instant::now();
            match medium {
                Medium::Shm => unsafe {
                    libc::shm_unlink(CString::new(shm_name(frame)).unwrap().as_ptr());
                },
                Medium::TempFile => std::fs::remove_file(temp_path(frame))?,
                _ => {}
            }
            sim.push((t1 - t0).as_secs_f32() * 1000.0);
            render.push((t2 - t1).as_secs_f32() * 1000.0);
            encode.push((t3 - t2).as_secs_f32() * 1000.0);
            total.push((t3 - t0).as_secs_f32() * 1000.0);
            bytes += out.len();
        }
        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let per_frame = bytes / frames;
        println!(
            "{:<6}  {:>6.2}  {:>9.2}  {:>10.2} / {:<10.2}  {:>6.2} / {:<7.2}  {:>11}  {:>13.1}",
            medium.name(), avg(&sim), avg(&render), avg(&encode), percentile(&encode, 0.99), avg(&total), percentile(&total, 0.99), per_frame, per_frame as f32 * 60.0 / 1e6
        );
        unlink_ring();
        if let Some(path) = dump {
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            pond.render(&mut frame_buf);
            ppm.extend(frame_buf.iter().flat_map(|p| p.to_le_bytes().into_iter().take(3)));
            std::fs::write(path, ppm)?;
        }
    }
    Ok(())
}

/// Set by SIGHUP/SIGTERM (e.g. the terminal window closing) so the loop exits through cleanup
/// and the ring files do not outlive us in /dev/shm.
static HANGUP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_hangup(_: libc::c_int) {
    HANGUP.store(true, Ordering::Relaxed);
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

/// Asks the terminal which transmission media it accepts. Returns None when no graphics
/// reply arrives at all (tmux swallows replies; so do terminals without the protocol).
fn probe(tmux: bool) -> Option<[bool; 3]> {
    let pixel = [0u8, 0, 0, 255];
    if let Ok(mut shm) = ShmFrame::create(&shm_name(0), pixel.len()) {
        shm[0] = u32::from_le_bytes(pixel);
    }
    let _ = std::fs::write(temp_path(0), pixel);
    let mut out = Vec::new();
    let queries = [
        format!("\x1b_Ga=q,i=31,s=1,v=1,f=32,t=s,S=4;{}\x1b\\", B64.encode(shm_name(0))),
        format!("\x1b_Ga=q,i=32,s=1,v=1,f=32,t=t,S=4;{}\x1b\\", B64.encode(temp_path(0).to_str().unwrap())),
        format!("\x1b_Ga=q,i=33,s=1,v=1,f=32;{}\x1b\\", B64.encode(pixel)),
    ];
    for q in &queries {
        push_seq(&mut out, q.as_bytes(), tmux);
    }
    out.extend_from_slice(b"\x1b[c");
    io::stdout().write_all(&out).unwrap();
    io::stdout().flush().unwrap();

    let mut reply = String::new();
    let deadline = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < deadline {
        reply.push_str(&String::from_utf8_lossy(&read_input(50)));
        if let Some(da) = reply.find("\x1b[?") {
            if reply[da..].contains('c') {
                break;
            }
        }
    }
    unsafe { libc::shm_unlink(CString::new(shm_name(0)).unwrap().as_ptr()) };
    let _ = std::fs::remove_file(temp_path(0));
    if !reply.contains("\x1b_G") {
        return None;
    }
    Some([reply.contains("i=31;OK"), reply.contains("i=32;OK"), reply.contains("i=33;OK")])
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bench_frames = None;
    let mut size = None;
    let mut medium = None;
    let mut px = 4;
    let mut water = 2;
    let mut threads = 1;
    let mut dump = None;
    let mut probe_only = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench" => bench_frames = it.next().and_then(|s| s.parse().ok()),
            "--size" => size = it.next().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))),
            "--medium" => medium = it.next().and_then(|s| Medium::ALL.into_iter().find(|m| m.name() == s)),
            "--px" => px = it.next().and_then(|s| s.parse().ok()).unwrap_or(px),
            "--water" => water = it.next().and_then(|s| s.parse().ok()).unwrap_or(water),
            "--threads" => threads = it.next().and_then(|s| s.parse().ok()).unwrap_or(threads),
            "--dump" => dump = it.next().cloned(),
            "--probe" => probe_only = true,
            _ => {
                eprintln!("usage: koi-fastcpu [--medium shm|ring|file|direct|zlib] [--px N] [--water K] [--threads N] [--probe] [--bench N [--size WxH] [--dump out.ppm]]");
                std::process::exit(2);
            }
        }
    }
    if let Some(frames) = bench_frames {
        let (cols, rows) = size.unwrap_or((160, 45));
        return bench(frames, cols, rows, px, water, threads, medium, dump.as_deref());
    }

    let tmux = std::env::var_os("TMUX").is_some();
    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { libc::tcgetattr(0, &mut original) };
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }

    let probed = probe(tmux);
    if probe_only {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
        match probed {
            None => println!("no graphics reply (not a Kitty-protocol terminal, or tmux swallowed it)"),
            Some(ok) => println!("shm {}  file {}  direct {}", ok[0], ok[1], ok[2]),
        }
        return Ok(());
    }
    let medium = medium.unwrap_or(match probed {
        Some([true, _, _]) | None => Medium::Ring,
        Some([_, true, _]) => Medium::TempFile,
        _ => Medium::Direct,
    });

    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[2J")?;
    let (mut cols, mut rows, xpixel, ypixel) = winsize();
    let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
    let mut pond = Pond::new(w, h, 5, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64, water, threads);
    let mut frame_buf = vec![0u32; w * h];
    let mut ring = if medium == Medium::Ring { create_ring(w * h * 4)? } else { Vec::new() };
    let mut out = Vec::new();
    let mut show_overlay = false;
    let mut build_ms: VecDeque<f32> = VecDeque::new();
    let (mut fps, mut fps_frames, mut fps_since) = (0.0, 0, Instant::now());
    let mut last_bytes = 0;
    let step = Duration::from_secs_f32(pond::DT);
    let mut sim_time = Instant::now();
    let mut deadline = Instant::now();

    let mut frame_id = 0u64;
    unsafe {
        libc::signal(libc::SIGHUP, on_hangup as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_hangup as *const () as libc::sighandler_t);
    }
    'frames: loop {
        if HANGUP.load(Ordering::Relaxed) {
            break;
        }
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
            let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
            pond = Pond::new(w, h, 5, frame_id, water, threads);
            frame_buf = vec![0u32; w * h];
            if medium == Medium::Ring {
                ring = create_ring(w * h * 4)?;
            }
            stdout.write_all(b"\x1b[2J")?;
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
        let mut shm;
        let rgba: &mut [u32] = match medium {
            Medium::Shm => {
                shm = ShmFrame::create(&shm_name(frame_id), pond.w * pond.h * 4)?;
                &mut shm
            }
            Medium::Ring => &mut ring[(frame_id % RING) as usize],
            _ => &mut frame_buf,
        };
        pond.render(rgba);
        let overlay = show_overlay.then(|| {
            let samples: Vec<f32> = build_ms.iter().copied().collect();
            format!(
                "{fps:.1} fps | build p50 {:.2}ms p99 {:.2}ms | {last_bytes} B/frame | {} {}x{} water/{water} {threads}t{}",
                percentile(&samples, 0.5), percentile(&samples, 0.99), medium.name(), pond.w, pond.h, if tmux { " | tmux" } else { "" }
            )
        });
        encode_frame(&mut out, rgba, pond.w, pond.h, cols, rows, medium, frame_id, tmux, overlay.as_deref())?;
        if stdout.write_all(&out).and_then(|()| stdout.flush()).is_err() {
            break;
        }
        last_bytes = out.len();
        if frame_id > 120 {
            unsafe { libc::shm_unlink(CString::new(shm_name(frame_id - 120)).unwrap().as_ptr()) };
            let _ = std::fs::remove_file(temp_path(frame_id - 120));
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

    let mut restore = Vec::new();
    push_seq(&mut restore, b"\x1b_Ga=d,d=I,i=1,q=2\x1b\\", tmux);
    restore.extend_from_slice(b"\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l");
    let _ = stdout.write_all(&restore).and_then(|()| stdout.flush());
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
    unlink_ring();
    for id in frame_id.saturating_sub(120)..=frame_id {
        unsafe { libc::shm_unlink(CString::new(shm_name(id)).unwrap().as_ptr()) };
        let _ = std::fs::remove_file(temp_path(id));
    }
    Ok(())
}
