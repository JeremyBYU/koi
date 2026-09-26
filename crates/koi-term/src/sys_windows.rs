//! The console through Win32. With virtual terminal input and processing on, the console
//! takes and gives the same escape sequences a Unix terminal does, so everything above this
//! module works unchanged.

use crate::QUIT;
use std::ffi::c_void;
use std::io;
use std::os::windows::io::IntoRawHandle;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, TRUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::WriteFile;
use windows_sys::Win32::System::Console::{
    CONSOLE_MODE, CONSOLE_SCREEN_BUFFER_INFO, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_EXTENDED_FLAGS, ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW, STD_ERROR_HANDLE,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler, SetConsoleMode, SetStdHandle,
};
use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows_sys::core::BOOL;

/// The console modes of stdin and stdout before raw mode.
pub type Mode = (CONSOLE_MODE, CONSOLE_MODE);

/// The real stderr while it points at NUL. Null when it is not redirected.
static STDERR: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "system" fn on_ctrl(_: u32) -> BOOL {
    QUIT.store(true, Ordering::Relaxed);
    TRUE
}

/// Puts the console in raw mode with virtual terminal input and output, and returns its modes
/// from before. Fails if stdin or stdout is not a console, or the console is too old for
/// virtual terminal sequences.
pub fn raw() -> io::Result<Mode> {
    let (input, output) = unsafe { (GetStdHandle(STD_INPUT_HANDLE), GetStdHandle(STD_OUTPUT_HANDLE)) };
    let (mut in_mode, mut out_mode) = (0, 0);
    if unsafe { GetConsoleMode(input, &mut in_mode) } == 0 || unsafe { GetConsoleMode(output, &mut out_mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Only virtual terminal input: no line editing, no echo, Ctrl-C as a key, and
    // ENABLE_EXTENDED_FLAGS without ENABLE_QUICK_EDIT_MODE so a click reaches the pond
    // instead of starting a text selection.
    let set = unsafe { SetConsoleMode(input, ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_EXTENDED_FLAGS) } != 0
        && unsafe { SetConsoleMode(output, out_mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN) } != 0;
    if !set {
        let e = io::Error::last_os_error();
        set_mode(&(in_mode, out_mode));
        return Err(e);
    }
    Ok((in_mode, out_mode))
}

/// Puts back modes `raw` returned.
pub fn set_mode(&(in_mode, out_mode): &Mode) {
    unsafe {
        SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), in_mode);
        SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), out_mode);
    }
}

/// Sets `QUIT` on Ctrl-Break and when the console closes. Ctrl-C is a key in raw mode.
pub fn catch_quit() {
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), TRUE) };
}

/// Points stderr at NUL until `restore_stderr`.
pub fn silence_stderr() {
    let original = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    if original.is_null() || original == INVALID_HANDLE_VALUE {
        return;
    }
    if let Ok(nul) = std::fs::OpenOptions::new().write(true).open("NUL") {
        STDERR.store(original, Ordering::Relaxed);
        unsafe { SetStdHandle(STD_ERROR_HANDLE, nul.into_raw_handle()) };
    }
}

/// Points stderr back where it was before `silence_stderr`.
pub fn restore_stderr() {
    let original = STDERR.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !original.is_null() {
        unsafe {
            let nul = GetStdHandle(STD_ERROR_HANDLE);
            SetStdHandle(STD_ERROR_HANDLE, original);
            CloseHandle(nul);
        }
    }
}

/// Writes straight to the stdout handle, bypassing std's stdout lock and buffer, so the panic
/// hook can use it.
pub fn write_fd(mut bytes: &[u8]) {
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    while !bytes.is_empty() {
        let mut n = 0;
        let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        if unsafe { WriteFile(output, bytes.as_ptr(), len, &mut n, std::ptr::null_mut()) } == 0 || n == 0 {
            return;
        }
        bytes = bytes.get(usize::try_from(n).unwrap_or(bytes.len())..).unwrap_or_default();
    }
}

/// Columns and rows of the visible window. The console does not give its size in pixels, so
/// those are 0.
pub fn winsize() -> (usize, usize, usize, usize) {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return (0, 0, 0, 0);
    }
    let span = |from: i16, to: i16| usize::try_from(i32::from(to) - i32::from(from) + 1).unwrap_or(0);
    (span(info.srWindow.Left, info.srWindow.Right), span(info.srWindow.Top, info.srWindow.Bottom), 0, 0)
}

/// Waits up to `timeout` for keys on stdin and returns them as UTF-8. With virtual terminal
/// input every byte of a sequence arrives as a key. The console also queues focus and menu
/// records whatever the mode, which are read and dropped, and the wait goes on.
pub fn read_input(timeout: Duration) -> Vec<u8> {
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let deadline = Instant::now() + timeout;
    loop {
        // Rounded up, so a wait for a frame deadline never wakes before it.
        let ms = u32::try_from(deadline.saturating_duration_since(Instant::now()).as_micros().div_ceil(1000)).unwrap_or(INFINITE - 1);
        if unsafe { WaitForSingleObject(input, ms) } != WAIT_OBJECT_0 {
            return Vec::new();
        }
        let mut records = [INPUT_RECORD::default(); 512];
        let mut n = 0;
        if unsafe { ReadConsoleInputW(input, records.as_mut_ptr(), 512, &mut n) } == 0 {
            return Vec::new();
        }
        let mut units = Vec::new();
        for record in records.iter().take(usize::try_from(n).unwrap_or(0)) {
            if u32::from(record.EventType) != KEY_EVENT {
                continue;
            }
            let key = unsafe { record.Event.KeyEvent };
            let unit = unsafe { key.uChar.UnicodeChar };
            if key.bKeyDown != 0 && unit != 0 {
                units.extend(std::iter::repeat_n(unit, usize::from(key.wRepeatCount.max(1))));
            }
        }
        if !units.is_empty() {
            return String::from_utf16_lossy(&units).into_bytes();
        }
    }
}

/// Always fails: no Windows terminal reads a POSIX shm object, so the probe does not offer
/// `t=s` and a forced `kitty` tier stops with this error.
pub fn write_shm(_: &str, _: &[u8]) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Kitty images through shared memory need Linux or macOS; use --protocol kitty-direct"))
}

/// Nothing to remove, since `write_shm` makes nothing.
pub fn unlink(_: &str) {}
