//! The terminal side: raw mode and its restore, reading and parsing input, sending images over
//! the Kitty graphics protocol through a ring of shared-memory files, and inside tmux, the
//! passthrough wrapping and Unicode placeholder cells that let an image show in a pane.

#![warn(missing_docs)]

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// Set by SIGINT, SIGTERM and SIGHUP. The frame loop exits through the normal cleanup.
pub static QUIT: AtomicBool = AtomicBool::new(false);

static ORIGINAL: OnceLock<libc::termios> = OnceLock::new();

/// The image shown through placeholder cells in tmux, deleted on restore. 0 outside tmux.
static TMUX_IMAGE: AtomicU32 = AtomicU32::new(0);

/// Numbers each ring, so a new ring built on resize never shares a name with the one it replaces.
static RINGS: AtomicU64 = AtomicU64::new(0);

const SHM_PREFIX: &str = "koi-pond-";

extern "C" fn on_signal(_: libc::c_int) {
    QUIT.store(true, Ordering::Relaxed);
}

/// Raw mode, alternate screen, focus reporting and (optionally) mouse reporting, undone on drop
/// and by a panic on the thread that entered.
pub struct Terminal;

impl Terminal {
    /// Puts the terminal in raw mode on the alternate screen with focus reporting, and SGR
    /// mouse reporting if `mouse`. Installs the signal handlers that set `QUIT` and a panic
    /// hook that restores the terminal when the calling thread panics. `tmux_image` is the
    /// image a tmux pane shows through placeholder cells, which the restore deletes. Fails if
    /// stdin is not a terminal.
    pub fn enter(mouse: bool, tmux_image: Option<u8>) -> io::Result<Terminal> {
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(0, &mut original) } != 0 {
            return Err(io::Error::other(format!("stdin is not a terminal ({})", io::Error::last_os_error())));
        }
        let _ = ORIGINAL.set(original);
        TMUX_IMAGE.store(tmux_image.map_or(0, u32::from), Ordering::Relaxed);
        let mut raw = original;
        unsafe {
            libc::cfmakeraw(&mut raw);
            libc::tcsetattr(0, libc::TCSANOW, &raw);
        }

        let handler: extern "C" fn(libc::c_int) = on_signal;
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            unsafe { libc::signal(signal, handler as libc::sighandler_t) };
        }
        let default_hook = std::panic::take_hook();
        let main = std::thread::current().id();
        std::panic::set_hook(Box::new(move |info| {
            // A panic on another thread (the audio decoding a bad file, say) leaves the pond
            // running, so the terminal must stay as the pond needs it.
            if std::thread::current().id() == main {
                restore();
            }
            default_hook(info);
        }));

        let mut setup = b"\x1b[?1049h\x1b[?25l\x1b[?1004h\x1b[2J".to_vec();
        if mouse {
            setup.extend_from_slice(b"\x1b[?1000h\x1b[?1006h");
        }
        write_fd(&setup);
        Ok(Terminal)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        restore();
    }
}

/// Writes straight to fd 1, bypassing std's stdout lock and buffer, so the panic hook can use it.
fn write_fd(mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(1, bytes.as_ptr().cast(), bytes.len()) };
        match usize::try_from(n) {
            Ok(n) if n > 0 => bytes = &bytes[n..],
            _ if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
            _ => return,
        }
    }
}

fn restore() {
    // tmux would take a bare Kitty command for a pane title.
    let tmux_image = TMUX_IMAGE.load(Ordering::Relaxed);
    let mut bytes = Vec::new();
    if tmux_image == 0 {
        bytes.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\");
    } else {
        tmux_wrap(&mut bytes, format!("\x1b_Ga=d,d=I,i={tmux_image},q=2\x1b\\").as_bytes());
    }
    bytes.extend_from_slice(b"\x1b[?2026l\x1b[?1004l\x1b[?1003l\x1b[?1006l\x1b[?1000l\x1b[0m\x1b[?25h\x1b[?1049l");
    write_fd(&bytes);
    if let Some(original) = ORIGINAL.get() {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, original) };
    }
}

/// Turns on reports of pointer motion with no button held (mode 1003), for the HUD's hover
/// reveal. Motion arrives as `Input::Move`. Call after `Terminal::enter` with mouse on; the
/// restore on exit turns it off again.
pub fn report_motion() {
    write_fd(b"\x1b[?1003h");
}

/// Appends `escape` to `out` inside a tmux passthrough (`ESC P tmux; ... ESC \`), with every
/// ESC in it doubled, so tmux hands it to the terminal outside. tmux drops it unless
/// `allow-passthrough` is on.
pub fn tmux_wrap(out: &mut Vec<u8>, escape: &[u8]) {
    out.extend_from_slice(b"\x1bPtmux;");
    for &byte in escape {
        if byte == 0x1b {
            out.push(0x1b);
        }
        out.push(byte);
    }
    out.extend_from_slice(b"\x1b\\");
}

/// The character a cell holds to show part of an image with a virtual placement (`U=1`).
const PLACEHOLDER: char = '\u{10EEEE}';

/// The combining marks that number a placeholder cell's image row and column, from kitty's
/// rowcolumn-diacritics.txt: mark `n` means row or column `n`.
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}', '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', 
    '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}', '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}', 
    '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}', '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', 
    '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}', 
    '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}', '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', 
    '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}', '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}', 
    '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}', '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}', 
    '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}', '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}', 
    '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}', '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}', 
    '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}', '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}', '\u{0822}', 
    '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}', '\u{082B}', '\u{082C}', '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', 
    '\u{0F82}', '\u{0F83}', '\u{0F86}', '\u{0F87}', '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}', 
    '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}', '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}', 
    '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}', '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}', 
    '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}', '\u{1DD1}', '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', 
    '\u{1DD6}', '\u{1DD7}', '\u{1DD8}', '\u{1DD9}', '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}', '\u{1DE0}', '\u{1DE1}', 
    '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}', '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}', 
    '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}', '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}', 
    '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}', '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', 
    '\u{2DEF}', '\u{2DF0}', '\u{2DF1}', '\u{2DF2}', '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}', '\u{2DF9}', '\u{2DFA}', 
    '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}', '\u{A67C}', '\u{A67D}', '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}', 
    '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}', '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}', 
    '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}', '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}', 
    '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}', '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}', '\u{1D187}', 
    '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}', '\u{1D242}', '\u{1D243}', '\u{1D244}'
];

/// Appends to `out` `n` placeholder cells showing image `id` from 0-based `row` and `col`:
/// the cursor move, the id as a 256-colour foreground, then each cell with its row and
/// column marks. A cell past the 297th column has no column mark, and the terminal counts it
/// on from the cell to its left.
pub fn placeholders(out: &mut Vec<u8>, id: u8, row: usize, col: usize, n: usize) {
    out.extend_from_slice(format!("\x1b[{};{}H\x1b[38;5;{id}m", row + 1, col + 1).as_bytes());
    let mut cell = [0u8; 4];
    for col in col..col + n {
        out.extend_from_slice(PLACEHOLDER.encode_utf8(&mut cell).as_bytes());
        for mark in [DIACRITICS.get(row), DIACRITICS.get(col)].into_iter().flatten() {
            out.extend_from_slice(mark.encode_utf8(&mut cell).as_bytes());
        }
    }
    out.extend_from_slice(b"\x1b[39m");
}

/// Columns, rows, and the window size in pixels (0 if the terminal does not say).
pub fn winsize() -> (usize, usize, usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) };
    (ws.ws_col.into(), ws.ws_row.into(), ws.ws_xpixel.into(), ws.ws_ypixel.into())
}

/// Waits up to `timeout` for input and returns what arrived, up to 4 KiB. A signal ends the
/// wait early. An escape sequence can be split across two reads.
pub fn read_input(timeout: Duration) -> Vec<u8> {
    let mut fds = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let ts = libc::timespec { tv_sec: timeout.as_secs().try_into().unwrap_or(libc::time_t::MAX), tv_nsec: timeout.subsec_nanos().into() };
    if unsafe { libc::ppoll(&mut fds, 1, &ts, std::ptr::null()) } <= 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; 4096];
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    buf.truncate(usize::try_from(n).unwrap_or(0));
    buf
}

/// Removes shm objects left by koi processes that no longer exist (killed with SIGKILL, say).
pub fn remove_stale_shm() {
    let Ok(entries) = std::fs::read_dir("/dev/shm") else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix(SHM_PREFIX)).and_then(|rest| rest.split('-').next()).and_then(|pid| pid.parse::<libc::pid_t>().ok()) else {
            continue;
        };
        let gone = unsafe { libc::kill(pid, 0) } != 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        if gone {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Image pages that stay mapped and faulted in. Each image is copied into the next slot, which
/// is then hard-linked under a fresh name and sent with `t=s`. Ghostty unlinks that name after
/// reading it; the slot's own name keeps the pages alive. Every name is removed on drop.
pub struct ShmRing {
    slots: Vec<(PathBuf, *mut u8)>,
    prefix: String,
    capacity: usize,
    next: usize,
    seq: u64,
    links: VecDeque<String>,
    /// Image bytes sent through the ring so far, for the stats line.
    pub bytes: usize,
}

impl ShmRing {
    /// Creates `slots` files of `capacity` bytes each. An image sent later must fit one slot.
    /// Ghostty must have read an image before `slots` more are sent after it.
    pub fn new(slots: usize, capacity: usize) -> io::Result<ShmRing> {
        let prefix = format!("{SHM_PREFIX}{}-r{}", std::process::id(), RINGS.fetch_add(1, Ordering::Relaxed));
        let mut ring = ShmRing { slots: Vec::new(), prefix, capacity, next: 0, seq: 0, links: VecDeque::new(), bytes: 0 };
        for k in 0..slots {
            let path = PathBuf::from(format!("/dev/shm/{}-slot{k}", ring.prefix));
            let file = OpenOptions::new().read(true).write(true).create(true).truncate(true).mode(0o600).open(&path)?;
            ring.slots.push((path, std::ptr::null_mut()));
            file.set_len(u64::try_from(capacity).expect("slot size fits u64"))?;
            let ptr = unsafe { libc::mmap(std::ptr::null_mut(), capacity, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED | libc::MAP_POPULATE, file.as_raw_fd(), 0) };
            if ptr == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            ring.slots[k].1 = ptr.cast();
        }
        Ok(ring)
    }

    /// Appends to `out` a Kitty graphics command with `keys` (for example `a=T,f=32,s=..`)
    /// that transmits `data` through the next slot. Fails if `data` is larger than a slot.
    pub fn transmit(&mut self, out: &mut Vec<u8>, data: &[u8], keys: &str) -> io::Result<()> {
        if data.len() > self.capacity {
            return Err(io::Error::other(format!("image of {} bytes does not fit a {} byte ring slot", data.len(), self.capacity)));
        }
        let (path, ptr) = &self.slots[self.next];
        unsafe { std::slice::from_raw_parts_mut(*ptr, data.len()) }.copy_from_slice(data);
        self.seq += 1;
        let name = format!("/{}-{}", self.prefix, self.seq);
        std::fs::hard_link(path, format!("/dev/shm{name}"))?;
        out.extend_from_slice(format!("\x1b_G{keys},t=s,S={},q=2;{}\x1b\\", data.len(), B64.encode(&name)).as_bytes());
        self.links.push_back(name);
        // The slot this link points at is about to be overwritten, so Ghostty has long read it.
        while self.links.len() > self.slots.len() {
            if let Some(old) = self.links.pop_front() {
                unlink(&old);
            }
        }
        self.next = (self.next + 1) % self.slots.len();
        self.bytes += data.len();
        Ok(())
    }
}

fn unlink(name: &str) {
    if let Ok(name) = CString::new(name) {
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
}

impl Drop for ShmRing {
    fn drop(&mut self) {
        for (path, ptr) in &self.slots {
            if !ptr.is_null() {
                unsafe { libc::munmap(ptr.cast(), self.capacity) };
            }
            let _ = std::fs::remove_file(path);
        }
        for name in &self.links {
            unlink(name);
        }
    }
}

/// One event parsed from terminal input.
#[derive(Debug, PartialEq)]
pub enum Input {
    /// The window gained (`true`) or lost (`false`) focus.
    Focus(bool),
    /// A left-button press at a 1-based cell column and row.
    Click {
        /// 1-based cell column.
        col: usize,
        /// 1-based cell row.
        row: usize,
    },
    /// A wheel step at a 1-based cell column and row: SGR button 64 (`up`) or 65.
    Scroll {
        /// 1-based cell column.
        col: usize,
        /// 1-based cell row.
        row: usize,
        /// Wheel away from the user.
        up: bool,
    },
    /// The pointer moved to a 1-based cell column and row with no button held. Only reported
    /// after `report_motion`.
    Move {
        /// 1-based cell column.
        col: usize,
        /// 1-based cell row.
        row: usize,
    },
    /// A lone Esc: `\x1b` as the last byte of a read. Esc followed by another byte is an Alt
    /// chord or an SS3 key and comes out as `Other`.
    Escape,
    /// Any byte outside an escape sequence: a key, or a control character such as Tab (9) or
    /// Ctrl-C (3).
    Key(u8),
    /// Any other escape sequence, including other mouse reports.
    Other,
}

/// Splits raw input into events: focus reports (`CSI I`, `CSI O`), SGR mouse reports
/// (`CSI < b;x;y M`), other CSI sequences, a lone Esc, Esc chords, and single-byte keys.
/// Also returns how many bytes the events used. An escape sequence cut off at the end of
/// `input` is left over, for the caller to put in front of the next read, since a read can
/// end partway through one.
pub fn parse_input(input: &[u8]) -> (Vec<Input>, usize) {
    let mut events = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let rest = &input[i..];
        if rest.starts_with(b"\x1b[I") || rest.starts_with(b"\x1b[O") {
            events.push(Input::Focus(rest[2] == b'I'));
            i += 3;
        } else if rest.starts_with(b"\x1b[<") {
            let Some(end) = rest[3..].iter().position(|b| !b.is_ascii_digit() && *b != b';').map(|p| p + 3) else { break };
            let fields: Vec<usize> = String::from_utf8_lossy(&rest[3..end]).split(';').filter_map(|s| s.parse().ok()).collect();
            events.push(match (&fields[..], rest[end]) {
                ([0, x, y], b'M') => Input::Click { col: *x, row: *y },
                ([b @ (64 | 65), x, y], b'M') => Input::Scroll { col: *x, row: *y, up: *b == 64 },
                ([35, x, y], b'M') => Input::Move { col: *x, row: *y },
                _ => Input::Other,
            });
            i += end + 1;
        } else if rest.starts_with(b"\x1b[") {
            let Some(end) = rest[2..].iter().position(|b| (0x40..=0x7e).contains(b)) else { break };
            i += end + 3;
            events.push(Input::Other);
        } else if rest == b"\x1b" {
            events.push(Input::Escape);
            i += 1;
        } else if rest.starts_with(b"\x1bO") {
            if rest.len() < 3 {
                break;
            }
            i += 3;
            events.push(Input::Other);
        } else if rest[0] == 0x1b {
            i += 2;
            events.push(Input::Other);
        } else {
            events.push(Input::Key(rest[0]));
            i += 1;
        }
    }
    (events, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_input() {
        let input = b"\x1b[Of\x1b[<0;12;7M\x1b[<0;12;7m\x1b[<64;3;4M\x1b[<65;3;4M\x1b[<35;9;2M\x1b[<32;9;2M\x1b[Aq\x1bOP\x1bx\t?1\x1b[I\x03\x1b";
        let expected = [
            Input::Focus(false),
            Input::Key(b'f'),
            Input::Click { col: 12, row: 7 },
            Input::Other,
            Input::Scroll { col: 3, row: 4, up: true },
            Input::Scroll { col: 3, row: 4, up: false },
            Input::Move { col: 9, row: 2 },
            Input::Other,
            Input::Other,
            Input::Key(b'q'),
            Input::Other,
            Input::Other,
            Input::Key(b'\t'),
            Input::Key(b'?'),
            Input::Key(b'1'),
            Input::Focus(true),
            Input::Key(3),
            Input::Escape,
        ];
        assert_eq!(parse_input(input), (Vec::from(expected), input.len()));
    }

    /// Every ESC inside the passthrough is doubled, and the wrapper's own are not.
    #[test]
    fn tmux_wrap_doubles_escapes() {
        let mut out = Vec::new();
        tmux_wrap(&mut out, b"\x1b_Ga=d,i=7\x1b\\");
        assert_eq!(out, b"\x1bPtmux;\x1b\x1b_Ga=d,i=7\x1b\x1b\\\x1b\\");
    }

    /// Each cell is the placeholder, its row mark and its column mark, drawn in the image id as
    /// a 256-colour foreground. Past the table a cell keeps only the marks that exist.
    #[test]
    fn placeholder_cells_carry_row_column_and_id() {
        let mut out = Vec::new();
        placeholders(&mut out, 42, 1, 0, 2);
        assert_eq!(String::from_utf8(out).expect("UTF-8"), "\x1b[2;1H\x1b[38;5;42m\u{10EEEE}\u{030D}\u{0305}\u{10EEEE}\u{030D}\u{030D}\x1b[39m");
        let mut out = Vec::new();
        placeholders(&mut out, 200, 0, 296, 2);
        assert_eq!(String::from_utf8(out).expect("UTF-8"), "\x1b[1;297H\x1b[38;5;200m\u{10EEEE}\u{0305}\u{1D244}\u{10EEEE}\u{0305}\x1b[39m");
    }

    /// A read can end partway through a sequence. Carrying the leftover into the next read
    /// gives the same events as one read, wherever the split falls, except that a split right
    /// after an Esc reads as a lone Esc.
    #[test]
    fn split_reads_carry_over() {
        let input = b"f\x1b[<0;12;7M\x1b[<35;9;2M\x1b[A\x1bOPq\x1b[I";
        let (whole, _) = parse_input(input);
        for k in 1..input.len() {
            let (mut events, used) = parse_input(&input[..k]);
            let carried = [&input[used..k], &input[k..]].concat();
            events.extend(parse_input(&carried).0);
            if input[k - 1] != 0x1b {
                assert_eq!(events, whole, "split at {k}");
            }
        }
    }
}
