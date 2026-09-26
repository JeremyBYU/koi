//! The terminal and shared memory through libc.

use crate::QUIT;
use std::ffi::CString;
use std::io;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

/// The terminal's settings before raw mode.
pub type Mode = libc::termios;

/// A copy of the real stderr while it points at /dev/null. -1 when it is not redirected.
static STDERR: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(_: libc::c_int) {
    QUIT.store(true, Ordering::Relaxed);
}

/// Puts stdin in raw mode and returns its settings from before. Fails if it is not a terminal.
pub fn raw() -> io::Result<Mode> {
    let mut original: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(0, &mut original) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut raw = original;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }
    Ok(original)
}

/// Puts back settings `raw` returned.
pub fn set_mode(mode: &Mode) {
    unsafe { libc::tcsetattr(0, libc::TCSANOW, mode) };
}

/// Sets `QUIT` on SIGINT, SIGTERM and SIGHUP.
pub fn catch_quit() {
    let handler: extern "C" fn(libc::c_int) = on_signal;
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe { libc::signal(signal, handler as libc::sighandler_t) };
    }
}

/// Points stderr at /dev/null until `restore_stderr`.
pub fn silence_stderr() {
    let null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY) };
    if null >= 0 {
        STDERR.store(unsafe { libc::dup(2) }, Ordering::Relaxed);
        unsafe {
            libc::dup2(null, 2);
            libc::close(null);
        }
    }
}

/// Points stderr back where it was before `silence_stderr`.
pub fn restore_stderr() {
    let stderr = STDERR.swap(-1, Ordering::Relaxed);
    if stderr >= 0 {
        unsafe {
            libc::dup2(stderr, 2);
            libc::close(stderr);
        }
    }
}

/// Writes straight to fd 1, bypassing std's stdout lock and buffer, so the panic hook can use it.
pub fn write_fd(mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(1, bytes.as_ptr().cast(), bytes.len()) };
        match usize::try_from(n) {
            Ok(n) if n > 0 => bytes = &bytes[n..],
            _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
            _ => return,
        }
    }
}

/// Columns, rows, and the window size in pixels (0 if the terminal does not say).
pub fn winsize() -> (usize, usize, usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) };
    (ws.ws_col.into(), ws.ws_row.into(), ws.ws_xpixel.into(), ws.ws_ypixel.into())
}

/// Waits up to `timeout` for input on stdin and returns up to 4 KiB of it. A signal ends the
/// wait early.
pub fn read_input(timeout: Duration) -> Vec<u8> {
    let mut fds = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    // Rounded up, so a wait for a frame deadline never wakes before it.
    let ms = libc::c_int::try_from(timeout.as_micros().div_ceil(1000)).unwrap_or(libc::c_int::MAX);
    if unsafe { libc::poll(&mut fds, 1, ms) } <= 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; 4096];
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    buf.truncate(usize::try_from(n).unwrap_or(0));
    buf
}

/// Creates the shm object `name` holding `data`.
pub fn write_shm(name: &str, data: &[u8]) -> io::Result<()> {
    let c_name = CString::new(name).map_err(io::Error::other)?;
    let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_CREAT | libc::O_EXCL | libc::O_RDWR, 0o600u32) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let len = libc::off_t::try_from(data.len()).map_err(io::Error::other)?;
    let ptr = if unsafe { libc::ftruncate(fd, len) } == 0 { unsafe { libc::mmap(std::ptr::null_mut(), data.len(), libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0) } } else { libc::MAP_FAILED };
    let result = if ptr == libc::MAP_FAILED {
        let e = io::Error::last_os_error();
        unsafe { libc::shm_unlink(c_name.as_ptr()) };
        Err(e)
    } else {
        unsafe { std::slice::from_raw_parts_mut(ptr.cast::<u8>(), data.len()) }.copy_from_slice(data);
        unsafe { libc::munmap(ptr, data.len()) };
        Ok(())
    };
    unsafe { libc::close(fd) };
    result
}

/// Removes the shm object `name`.
pub fn unlink(name: &str) {
    if let Ok(name) = CString::new(name) {
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
}
