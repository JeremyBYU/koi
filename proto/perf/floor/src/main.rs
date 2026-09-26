mod pond;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use pond::Pond;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::FromRawFd;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Tiny,
    Static,
    Place,
    Once,
    Blank,
    Text,
    Pond,
}

impl Mode {
    const ALL: [Mode; 7] = [Mode::Tiny, Mode::Static, Mode::Place, Mode::Once, Mode::Blank, Mode::Text, Mode::Pond];

    fn name(self) -> &'static str {
        match self {
            Mode::Tiny => "tiny",
            Mode::Static => "static",
            Mode::Place => "place",
            Mode::Once => "once",
            Mode::Blank => "blank",
            Mode::Text => "text",
            Mode::Pond => "pond",
        }
    }
}

fn shm_name(id: u64) -> String {
    format!("/koi-floor-{}-{}", std::process::id(), id)
}

fn unlink_shm(id: u64) {
    unsafe { libc::shm_unlink(CString::new(shm_name(id)).unwrap().as_ptr()) };
}

/// Appends an a=T upload of `rgb` (w x h) through a fresh shm object, placed at the top-left over `cols` x `rows` cells.
fn push_upload(out: &mut Vec<u8>, rgb: &[u8], w: usize, h: usize, cols: usize, rows: usize, frame_id: u64) -> io::Result<()> {
    let name = shm_name(frame_id);
    let cname = CString::new(name.as_str()).unwrap();
    let fd = unsafe { libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe { File::from_raw_fd(fd) }.write_all(rgb)?;
    out.extend_from_slice(b"\x1b[H");
    out.extend_from_slice(format!("\x1b_Ga=T,f=24,s={w},v={h},i=1,p=1,c={cols},r={rows},C=1,z=-1,q=2,t=s,S={};{}\x1b\\", rgb.len(), B64.encode(&name)).as_bytes());
    Ok(())
}

fn image_size(cols: usize, rows: usize, xpixel: usize, ypixel: usize, px: usize) -> (usize, usize) {
    let aspect = if xpixel > 0 && ypixel > 0 { (ypixel as f32 / rows as f32) / (xpixel as f32 / cols as f32) } else { 2.0 };
    (cols * px, (rows as f32 * px as f32 * aspect).round() as usize)
}

fn winsize() -> (usize, usize, usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) };
    (ws.ws_col.into(), ws.ws_row.into(), ws.ws_xpixel.into(), ws.ws_ypixel.into())
}

fn read_input(timeout_ms: i32) -> Vec<u8> {
    let mut fds = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let mut buf = vec![0u8; 4096];
    if unsafe { libc::poll(&mut fds, 1, timeout_ms) } <= 0 {
        return Vec::new();
    }
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    buf.truncate(usize::try_from(n).unwrap_or(0));
    buf
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut mode = Mode::Pond;
    let mut px = 4;
    let mut fps = 60.0f32;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--mode" => match it.next().and_then(|s| Mode::ALL.into_iter().find(|m| m.name() == s)) {
                Some(m) => mode = m,
                None => {
                    eprintln!("--mode takes tiny|static|place|once|blank|text|pond");
                    std::process::exit(2);
                }
            },
            "--px" => px = it.next().and_then(|s| s.parse().ok()).unwrap_or(px),
            "--fps" => fps = it.next().and_then(|s| s.parse().ok()).unwrap_or(fps),
            _ => {
                eprintln!("usage: koi-floor [--mode tiny|static|place|once|blank|text|pond] [--px N] [--fps N]");
                std::process::exit(2);
            }
        }
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
    stdout.flush()?;
    let (mut cols, mut rows, xpixel, ypixel) = winsize();
    let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    let mut pond = Pond::new(w, h, 5, now.as_secs() ^ u64::from(now.subsec_nanos()));
    pond.render();
    let mut tiny = [0u8; 16 * 16 * 3];
    let mut out = Vec::new();
    let mut show_overlay = false;
    let mut needs_upload = true;
    let mut needs_draw = true;
    let (mut measured_fps, mut fps_frames, mut fps_since) = (0.0, 0, Instant::now());
    let sim_step = Duration::from_secs_f32(pond::DT);
    let frame_step = Duration::from_secs_f32(1.0 / fps);
    let mut sim_time = Instant::now();
    let mut deadline = Instant::now();
    let idle = mode == Mode::Once || mode == Mode::Blank;

    let mut frame_id = 0u64;
    'frames: loop {
        let input = read_input(if idle && !needs_draw { -1 } else { 0 });
        let mut i = 0;
        while i < input.len() {
            match input[i] {
                b'q' | 3 => break 'frames,
                b'd' => {
                    show_overlay = !show_overlay;
                    needs_draw = true;
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
            let (w, h) = image_size(cols, rows, xpixel, ypixel, px);
            pond = Pond::new(w, h, 5, frame_id);
            pond.render();
            stdout.write_all(b"\x1b[2J")?;
            needs_upload = true;
            needs_draw = true;
        }

        frame_id += 1;
        out.clear();
        out.extend_from_slice(b"\x1b[?2026h");
        match mode {
            Mode::Tiny => {
                let shade = u8::try_from(frame_id % 256).unwrap();
                for (n, px) in tiny.chunks_exact_mut(3).enumerate() {
                    px.copy_from_slice(&[shade, u8::try_from(n % 256).unwrap(), 255 - shade]);
                }
                push_upload(&mut out, &tiny, 16, 16, 2, 1, frame_id)?;
            }
            Mode::Static => push_upload(&mut out, &pond.out, pond.w, pond.h, cols, rows, frame_id)?,
            Mode::Place => {
                if needs_upload {
                    push_upload(&mut out, &pond.out, pond.w, pond.h, cols, rows, frame_id)?;
                    needs_upload = false;
                } else {
                    out.extend_from_slice(b"\x1b[H");
                    out.extend_from_slice(format!("\x1b_Ga=p,i=1,p=1,c={cols},r={rows},X={},C=1,z=-1,q=2\x1b\\", frame_id % 2).as_bytes());
                }
            }
            Mode::Once => {
                if needs_upload {
                    push_upload(&mut out, &pond.out, pond.w, pond.h, cols, rows, frame_id)?;
                    needs_upload = false;
                }
            }
            Mode::Blank => {}
            Mode::Text => out.extend_from_slice(format!("\x1b[2;1H{}", char::from(b'a' + u8::try_from(frame_id % 26).unwrap())).as_bytes()),
            Mode::Pond => {
                let started = Instant::now();
                let mut steps = 0;
                while sim_time + sim_step <= started && steps < 8 {
                    pond.step();
                    sim_time += sim_step;
                    steps += 1;
                }
                if steps == 8 {
                    sim_time = started;
                }
                pond.render();
                push_upload(&mut out, &pond.out, pond.w, pond.h, cols, rows, frame_id)?;
            }
        }
        if !idle || needs_draw {
            out.extend_from_slice(b"\x1b[1;1H\x1b[2K");
            if show_overlay {
                let fps_text = if idle { "idle".to_string() } else { format!("{measured_fps:.1} fps") };
                out.extend_from_slice(format!("\x1b[48;2;8;24;30m\x1b[38;2;200;228;224m {} | {fps_text} | {}x{} image \x1b[0m", mode.name(), pond.w, pond.h).as_bytes());
            }
            needs_draw = false;
        }
        out.extend_from_slice(b"\x1b[?2026l");
        stdout.write_all(&out)?;
        stdout.flush()?;
        if frame_id > 120 {
            unlink_shm(frame_id - 120);
        }

        if idle {
            continue;
        }
        fps_frames += 1;
        if fps_since.elapsed() >= Duration::from_secs(1) {
            measured_fps = fps_frames as f32 / fps_since.elapsed().as_secs_f32();
            (fps_frames, fps_since) = (0, Instant::now());
        }
        deadline += frame_step;
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else {
            deadline = now;
        }
    }

    stdout.write_all(b"\x1b_Ga=d,d=I,i=1,q=2\x1b\\\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l")?;
    stdout.flush()?;
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) };
    for id in frame_id.saturating_sub(120)..=frame_id {
        unlink_shm(id);
    }
    Ok(())
}
