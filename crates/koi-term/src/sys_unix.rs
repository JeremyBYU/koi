//! The terminal and shared memory through rustix, and the quit signals through signal-hook.

use crate::QUIT;
use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::fs::Mode as Permissions;
use rustix::mm::{MapFlags, ProtFlags};
use rustix::shm;
use rustix::termios::{self, OptionalActions, Termios};
use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The terminal's settings before raw mode.
pub type Mode = Termios;

/// A copy of the real stderr while it points at /dev/null.
static STDERR: Mutex<Option<OwnedFd>> = Mutex::new(None);

/// Puts stdin in raw mode and returns its settings from before. Fails if it is not a terminal.
pub fn raw() -> io::Result<Mode> {
    let original = termios::tcgetattr(io::stdin())?;
    let mut raw = original.clone();
    raw.make_raw();
    termios::tcsetattr(io::stdin(), OptionalActions::Now, &raw)?;
    Ok(original)
}

/// Puts back settings `raw` returned.
pub fn set_mode(mode: &Mode) {
    let _ = termios::tcsetattr(io::stdin(), OptionalActions::Now, mode);
}

/// Sets `QUIT` on SIGINT, SIGTERM and SIGHUP.
pub fn catch_quit() {
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        let _ = signal_hook::flag::register(signal, Arc::clone(&QUIT));
    }
}

/// Points stderr at /dev/null until `restore_stderr`.
pub fn silence_stderr() {
    let (Ok(null), Ok(real)) = (OpenOptions::new().write(true).open("/dev/null"), io::stderr().as_fd().try_clone_to_owned()) else { return };
    if rustix::stdio::dup2_stderr(&null).is_ok() {
        *STDERR.lock().unwrap_or_else(|e| e.into_inner()) = Some(real);
    }
}

/// Points stderr back where it was before `silence_stderr`.
pub fn restore_stderr() {
    if let Some(real) = STDERR.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = rustix::stdio::dup2_stderr(&real);
    }
}

/// Writes straight to fd 1, bypassing std's stdout lock and buffer, so the panic hook can use it.
pub fn write_fd(mut bytes: &[u8]) {
    while !bytes.is_empty() {
        match rustix::io::write(io::stdout(), bytes) {
            Ok(n) if n > 0 => bytes = &bytes[n..],
            Err(rustix::io::Errno::INTR) => {}
            _ => return,
        }
    }
}

/// Columns, rows, and the window size in pixels (0 if the terminal does not say).
pub fn winsize() -> (usize, usize, usize, usize) {
    termios::tcgetwinsize(io::stdout()).map_or((0, 0, 0, 0), |ws| (ws.ws_col.into(), ws.ws_row.into(), ws.ws_xpixel.into(), ws.ws_ypixel.into()))
}

/// Waits up to `timeout` for input on stdin and returns up to 4 KiB of it. A signal ends the
/// wait early.
pub fn read_input(timeout: Duration) -> Vec<u8> {
    let stdin = io::stdin();
    let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
    // Rounded up to whole milliseconds, as poll(2) waited, so a wait for a frame deadline
    // never wakes before it.
    let ms = timeout.as_micros().div_ceil(1000);
    let wait = Timespec { tv_sec: (ms / 1000).try_into().unwrap_or(i64::MAX), tv_nsec: ((ms % 1000) * 1_000_000).try_into().unwrap_or(0) };
    if !matches!(rustix::event::poll(&mut fds, Some(&wait)), Ok(n) if n > 0) {
        return Vec::new();
    }
    let mut buf = vec![0u8; 4096];
    let n = rustix::io::read(&stdin, &mut buf).unwrap_or(0);
    buf.truncate(n);
    buf
}

/// Creates the shm object `name` holding `data`.
pub fn write_shm(name: &str, data: &[u8]) -> io::Result<()> {
    let fd = shm::open(name, shm::OFlags::CREATE | shm::OFlags::EXCL | shm::OFlags::RDWR, Permissions::from_raw_mode(0o600))?;
    let written = rustix::fs::ftruncate(&fd, u64::try_from(data.len()).map_err(io::Error::other)?).and_then(|()| {
        // macOS shm objects can only be written through a mapping.
        // SAFETY: a fresh shared mapping of the object's `data.len()` bytes, which nothing else
        // in this process maps, written once and unmapped before it is used again.
        unsafe {
            let ptr = rustix::mm::mmap(std::ptr::null_mut(), data.len(), ProtFlags::READ | ProtFlags::WRITE, MapFlags::SHARED, &fd, 0)?;
            std::slice::from_raw_parts_mut(ptr.cast::<u8>(), data.len()).copy_from_slice(data);
            rustix::mm::munmap(ptr, data.len())
        }
    });
    if written.is_err() {
        let _ = shm::unlink(name);
    }
    Ok(written?)
}

/// Removes the shm object `name`.
pub fn unlink(name: &str) {
    let _ = shm::unlink(name);
}
