//! The terminal side: raw mode and its restore, asking the terminal what it can draw, reading
//! and parsing input, sending images over the Kitty graphics protocol through a ring of
//! shared-memory files (or inline, zlib-compressed, where shared memory cannot reach), and
//! inside tmux, the passthrough wrapping and Unicode placeholder cells that let an image show
//! in a pane.

#![warn(missing_docs)]

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
#[cfg(target_os = "linux")]
use rustix::mm::{MapFlags, ProtFlags};
use std::collections::VecDeque;
#[cfg(target_os = "linux")]
use std::fs::OpenOptions;
use std::io;
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

// The same few functions per platform: raw mode, quit signals, stderr, the raw write, the
// window size, the input wait, and shm objects.
#[cfg_attr(unix, path = "sys_unix.rs")]
#[cfg_attr(windows, path = "sys_windows.rs")]
mod sys;

/// Set by SIGINT, SIGTERM and SIGHUP, or on Windows by Ctrl-Break and closing the console.
/// The frame loop exits through the normal cleanup. Shared, since signal-hook sets it through
/// its own handle.
pub static QUIT: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

static ORIGINAL: OnceLock<sys::Mode> = OnceLock::new();

/// What the restore writes before leaving the alternate screen, such as deleting the images.
static RESTORE: OnceLock<Vec<u8>> = OnceLock::new();

/// Numbers each ring, so a new ring built on resize never shares a name with the one it replaces.
static RINGS: AtomicU64 = AtomicU64::new(0);

const SHM_PREFIX: &str = "koi-pond-";

/// Raw mode, alternate screen, focus reporting, (optionally) mouse reporting and stderr sent to
/// /dev/null (NUL on Windows), undone on drop
/// and by a panic on the thread that entered.
pub struct Terminal;

impl Terminal {
    /// Puts the terminal in raw mode on the alternate screen with focus reporting, and SGR
    /// mouse reporting of presses, releases and drags if `mouse`. Installs the signal handlers that set `QUIT` and a panic
    /// hook that restores the terminal when the calling thread panics. The restore writes
    /// `before_restore` first, to delete what only the pond's protocol knows about (Kitty images, say),
    /// so a terminal without that protocol never sees it. Fails if stdin is not a terminal.
    pub fn enter(mouse: bool, before_restore: Vec<u8>) -> io::Result<Terminal> {
        let original = sys::raw().map_err(|e| io::Error::other(format!("stdin is not a terminal ({e})")))?;
        let _ = ORIGINAL.set(original);
        let _ = RESTORE.set(before_restore);

        sys::catch_quit();
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

        // Libraries (ALSA especially) print to stderr, which would land on the pond.
        sys::silence_stderr();

        let mut setup = b"\x1b[?1049h\x1b[?25l\x1b[?1004h\x1b[2J".to_vec();
        if mouse {
            setup.extend_from_slice(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
        }
        sys::write_fd(&setup);
        Ok(Terminal)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    let mut bytes = RESTORE.get().cloned().unwrap_or_default();
    bytes.extend_from_slice(b"\x1b[?2026l\x1b[?1004l\x1b[?1003l\x1b[?1002l\x1b[?1006l\x1b[?1000l\x1b[0m\x1b[?25h\x1b[?1049l");
    sys::write_fd(&bytes);
    sys::restore_stderr();
    if let Some(original) = ORIGINAL.get() {
        sys::set_mode(original);
    }
}

/// Turns on reports of pointer motion with no button held (mode 1003), for the HUD's hover
/// reveal. Motion arrives as `Input::Move`. Call after `Terminal::enter` with mouse on; the
/// restore on exit turns it off again.
pub fn report_motion() {
    sys::write_fd(b"\x1b[?1003h");
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
    '\u{0305}',
    '\u{030D}',
    '\u{030E}',
    '\u{0310}',
    '\u{0312}',
    '\u{033D}',
    '\u{033E}',
    '\u{033F}',
    '\u{0346}',
    '\u{034A}',
    '\u{034B}',
    '\u{034C}',
    '\u{0350}',
    '\u{0351}',
    '\u{0352}',
    '\u{0357}',
    '\u{035B}',
    '\u{0363}',
    '\u{0364}',
    '\u{0365}',
    '\u{0366}',
    '\u{0367}',
    '\u{0368}',
    '\u{0369}',
    '\u{036A}',
    '\u{036B}',
    '\u{036C}',
    '\u{036D}',
    '\u{036E}',
    '\u{036F}',
    '\u{0483}',
    '\u{0484}',
    '\u{0485}',
    '\u{0486}',
    '\u{0487}',
    '\u{0592}',
    '\u{0593}',
    '\u{0594}',
    '\u{0595}',
    '\u{0597}',
    '\u{0598}',
    '\u{0599}',
    '\u{059C}',
    '\u{059D}',
    '\u{059E}',
    '\u{059F}',
    '\u{05A0}',
    '\u{05A1}',
    '\u{05A8}',
    '\u{05A9}',
    '\u{05AB}',
    '\u{05AC}',
    '\u{05AF}',
    '\u{05C4}',
    '\u{0610}',
    '\u{0611}',
    '\u{0612}',
    '\u{0613}',
    '\u{0614}',
    '\u{0615}',
    '\u{0616}',
    '\u{0617}',
    '\u{0657}',
    '\u{0658}',
    '\u{0659}',
    '\u{065A}',
    '\u{065B}',
    '\u{065D}',
    '\u{065E}',
    '\u{06D6}',
    '\u{06D7}',
    '\u{06D8}',
    '\u{06D9}',
    '\u{06DA}',
    '\u{06DB}',
    '\u{06DC}',
    '\u{06DF}',
    '\u{06E0}',
    '\u{06E1}',
    '\u{06E2}',
    '\u{06E4}',
    '\u{06E7}',
    '\u{06E8}',
    '\u{06EB}',
    '\u{06EC}',
    '\u{0730}',
    '\u{0732}',
    '\u{0733}',
    '\u{0735}',
    '\u{0736}',
    '\u{073A}',
    '\u{073D}',
    '\u{073F}',
    '\u{0740}',
    '\u{0741}',
    '\u{0743}',
    '\u{0745}',
    '\u{0747}',
    '\u{0749}',
    '\u{074A}',
    '\u{07EB}',
    '\u{07EC}',
    '\u{07ED}',
    '\u{07EE}',
    '\u{07EF}',
    '\u{07F0}',
    '\u{07F1}',
    '\u{07F3}',
    '\u{0816}',
    '\u{0817}',
    '\u{0818}',
    '\u{0819}',
    '\u{081B}',
    '\u{081C}',
    '\u{081D}',
    '\u{081E}',
    '\u{081F}',
    '\u{0820}',
    '\u{0821}',
    '\u{0822}',
    '\u{0823}',
    '\u{0825}',
    '\u{0826}',
    '\u{0827}',
    '\u{0829}',
    '\u{082A}',
    '\u{082B}',
    '\u{082C}',
    '\u{082D}',
    '\u{0951}',
    '\u{0953}',
    '\u{0954}',
    '\u{0F82}',
    '\u{0F83}',
    '\u{0F86}',
    '\u{0F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
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

/// Asks the terminal what it can draw, in one write: a Kitty image read from shared memory
/// (id 31, only if a shm object can be made), a Kitty image sent inline (id 32), the sixel
/// colour registers and largest image (XTSMGRAPHICS), the cell size in pixels (`CSI 16 t`)
/// and the terminal's name (XTVERSION), then the device attributes, which every terminal
/// answers after the others, so they end the wait. In tmux (`tmux`) the Kitty queries go
/// through the passthrough to the terminal outside, whose answers come after tmux's own
/// device attributes, so the wait goes on a little for them. Returns what came back within
/// `timeout`, for `Caps::parse`: nothing at all from a terminal that answers nothing.
pub fn probe(timeout: Duration, tmux: bool) -> Vec<u8> {
    let Ok(original) = sys::raw() else {
        return Vec::new();
    };
    let name = format!("/{SHM_PREFIX}{}-probe", std::process::id());
    let shm = sys::write_shm(&name, &[0; 4]).is_ok();
    let mut kitty = Vec::new();
    if shm {
        kitty.extend_from_slice(format!("\x1b_Gi=31,a=q,t=s,f=32,s=1,v=1,S=4;{}\x1b\\", B64.encode(&name)).as_bytes());
    }
    kitty.extend_from_slice(b"\x1b_Gi=32,a=q,t=d,f=32,s=1,v=1;AAAAAA==\x1b\\");
    let mut query = Vec::new();
    if tmux {
        tmux_wrap(&mut query, &kitty);
    } else {
        query.extend_from_slice(&kitty);
    }
    query.extend_from_slice(b"\x1b[?1;1;0S\x1b[?2;4;0S\x1b[16t\x1b[>0q\x1b[c");

    sys::write_fd(&query);
    let start = std::time::Instant::now();
    let mut attributes: Option<std::time::Instant> = None;
    let mut reply = Vec::new();
    loop {
        let text = String::from_utf8_lossy(&reply);
        // The device attributes answer, `CSI ? ... c`, comes last.
        if attributes.is_none() && text.split('\x1b').any(|s| s.starts_with("[?") && s.ends_with('c')) {
            attributes = Some(std::time::Instant::now());
        }
        let kitty_done = (!shm || text.contains("\x1b_Gi=31;")) && text.contains("\x1b_Gi=32;");
        let done = attributes.is_some_and(|at| !tmux || kitty_done || at.elapsed() >= Duration::from_millis(300));
        let left = timeout.saturating_sub(start.elapsed());
        if done || left.is_zero() {
            break;
        }
        reply.extend(read_input(if tmux && attributes.is_some() { left.min(Duration::from_millis(50)) } else { left }));
    }
    sys::set_mode(&original);
    // Only a terminal that did not read the file leaves it behind.
    if shm {
        sys::unlink(&name);
    }
    reply
}

/// What the terminal can draw, from its answers to `probe`.
#[derive(Debug, Default, PartialEq)]
pub struct Caps {
    /// Kitty graphics read from shared memory (`t=s`), the pond's usual way.
    pub kitty_shm: bool,
    /// Kitty graphics sent inline (`t=d`), which also work over SSH.
    pub kitty_direct: bool,
    /// Sixel graphics: attribute 4 in the device attributes.
    pub sixel: Option<Sixel>,
    /// The cell size in pixels, width then height, from `CSI 16 t`.
    pub cell_px: Option<(usize, usize)>,
    /// 24-bit colour: `COLORTERM` says so, or it is Windows Terminal or xterm, or it has
    /// Kitty graphics.
    pub truecolor: bool,
    /// The name and version from XTVERSION, for the stats line.
    pub name: Option<String>,
}

/// A terminal's sixel limits.
#[derive(Debug, PartialEq)]
pub struct Sixel {
    /// Colour registers: the most colours one image can use. 256 when the terminal does not say.
    pub colors: usize,
    /// The largest image, width then height in pixels, when the terminal says.
    pub max: Option<(usize, usize)>,
}

impl Caps {
    /// Reads the answers `probe` returned. `colorterm` is `$COLORTERM`, and `windows_terminal`
    /// whether `$WT_SESSION` is set: Windows Terminal has 24-bit colour but does not say so.
    pub fn parse(reply: &[u8], colorterm: Option<&str>, windows_terminal: bool) -> Caps {
        let mut caps = Caps::default();
        let (mut sixel, mut colors, mut max) = (false, None, None);
        let text = String::from_utf8_lossy(reply);
        // Every answer starts with ESC, and a string answer's ST is an ESC too.
        for answer in text.split('\x1b') {
            let numbers = |s: &str| s.split(';').map(|n| n.parse::<usize>().ok()).collect::<Option<Vec<usize>>>();
            if let Some((keys, message)) = answer.strip_prefix("_G").and_then(|a| a.split_once(';')) {
                caps.kitty_shm |= keys == "i=31" && message == "OK";
                caps.kitty_direct |= keys == "i=32" && message == "OK";
            } else if let Some(name) = answer.strip_prefix("P>|") {
                caps.name = Some(name.to_string());
            } else if let Some(list) = answer.strip_prefix("[?").and_then(|a| a.strip_suffix('c')) {
                sixel = list.split(';').any(|a| a == "4");
            } else if let Some(values) = answer.strip_prefix("[?").and_then(|a| a.strip_suffix('S')).and_then(numbers) {
                // Item, status (0 is success), then the values.
                match values[..] {
                    [1, 0, n] => colors = Some(n),
                    [2, 0, w, h] => max = Some((w, h)),
                    _ => {}
                }
            } else if let Some(values) = answer.strip_prefix("[6;").and_then(|a| a.strip_suffix('t')).and_then(numbers)
                && let [h, w] = values[..]
                && w > 0
                && h > 0
            {
                caps.cell_px = Some((w, h));
            }
        }
        if sixel {
            caps.sixel = Some(Sixel { colors: colors.unwrap_or(256), max });
        }
        // xterm has had 24-bit colour for years without setting COLORTERM.
        let xterm = caps.name.as_deref().is_some_and(|name| name.starts_with("XTerm("));
        caps.truecolor = matches!(colorterm, Some("truecolor" | "24bit")) || windows_terminal || xterm || caps.kitty_shm || caps.kitty_direct;
        caps
    }
}

/// Columns, rows, and the window size in pixels (0 if the terminal does not say, as the
/// Windows console never does).
pub fn winsize() -> (usize, usize, usize, usize) {
    sys::winsize()
}

/// Waits up to `timeout` for input and returns what arrived, up to 4 KiB. A signal ends the
/// wait early. An escape sequence can be split across two reads.
pub fn read_input(timeout: Duration) -> Vec<u8> {
    sys::read_input(timeout)
}

/// Removes shm objects left by koi processes that no longer exist (killed with SIGKILL, say).
/// Only Linux can list them, in /dev/shm; elsewhere this does nothing.
pub fn remove_stale_shm() {
    #[cfg(target_os = "linux")]
    for entry in std::fs::read_dir("/dev/shm").into_iter().flatten().flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix(SHM_PREFIX)).and_then(|rest| rest.split('-').next()).and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !std::path::Path::new("/proc").join(pid.to_string()).exists() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Shared-memory images sent with `t=s`, each under a fresh name that the terminal unlinks
/// after reading it. On Linux the pages stay mapped and faulted in: each image is copied into
/// the next slot, a file in /dev/shm, which is then hard-linked under the fresh name, and the
/// slot's own name keeps the pages alive. On macOS each image is a new shm object. Every name
/// is removed on drop. On Windows no terminal reads the pond's shared memory, so the first
/// image sent fails: only a direct ring works there.
///
/// A direct ring (`ShmRing::direct`) has no files and sends each image inline instead, for a
/// terminal that cannot read the pond's shared memory (over SSH, say).
///
/// One image in 60 asks for the terminal's answer, so a terminal that keeps failing to show
/// them shows up as `Input::Graphics` errors; the rest are sent quietly (`q=2`).
pub struct ShmRing {
    #[cfg(target_os = "linux")]
    slots: Vec<(PathBuf, *mut u8)>,
    /// How many sent names to keep before unlinking the oldest, which the terminal has read.
    keep: usize,
    prefix: String,
    capacity: usize,
    #[cfg(target_os = "linux")]
    next: usize,
    seq: u64,
    links: VecDeque<String>,
    /// Image bytes sent through the ring so far, before compression, for the stats line.
    pub bytes: usize,
    /// Sends inline (`t=d`), zlib-compressed, instead of through shared memory.
    direct: bool,
}

impl ShmRing {
    /// Creates `slots` slots of `capacity` bytes each. An image sent later must fit one slot.
    /// The terminal must have read an image before `slots` more are sent after it.
    pub fn new(slots: usize, capacity: usize) -> io::Result<ShmRing> {
        let prefix = format!("{SHM_PREFIX}{}-r{}", std::process::id(), RINGS.fetch_add(1, Ordering::Relaxed));
        let ring = ShmRing {
            #[cfg(target_os = "linux")]
            slots: Vec::new(),
            keep: slots.max(1),
            prefix,
            capacity,
            #[cfg(target_os = "linux")]
            next: 0,
            seq: 0,
            links: VecDeque::new(),
            bytes: 0,
            direct: false,
        };
        #[cfg(target_os = "linux")]
        let mut ring = ring;
        #[cfg(target_os = "linux")]
        for k in 0..slots {
            let path = PathBuf::from(format!("/dev/shm/{}-slot{k}", ring.prefix));
            let file = OpenOptions::new().read(true).write(true).create(true).truncate(true).mode(0o600).open(&path)?;
            ring.slots.push((path, std::ptr::null_mut()));
            file.set_len(u64::try_from(capacity).expect("slot size fits u64"))?;
            // Mapped for the ring's life and written in place, which is several times faster
            // than writing the file each frame. SAFETY: a fresh shared mapping of the whole slot
            // file, which only this ring maps, unmapped in `drop`.
            let ptr = unsafe {
                rustix::mm::mmap(std::ptr::null_mut(), capacity, ProtFlags::READ | ProtFlags::WRITE, MapFlags::SHARED | MapFlags::POPULATE, &file, 0)
            }?;
            ring.slots[k].1 = ptr.cast();
        }
        Ok(ring)
    }

    /// A ring that sends every image inline, compressed, in chunks of at most 4096 base64
    /// bytes, as a terminal must accept them. It has no size limit and no files.
    pub fn direct() -> ShmRing {
        ShmRing {
            #[cfg(target_os = "linux")]
            slots: Vec::new(),
            keep: 0,
            prefix: String::new(),
            capacity: usize::MAX,
            #[cfg(target_os = "linux")]
            next: 0,
            seq: 0,
            links: VecDeque::new(),
            bytes: 0,
            direct: true,
        }
    }

    /// Appends to `out` a Kitty graphics command with `keys` (for example `a=T,f=32,s=..`)
    /// that transmits `data` through the next slot. Fails if `data` is larger than a slot.
    pub fn transmit(&mut self, out: &mut Vec<u8>, data: &[u8], keys: &str) -> io::Result<()> {
        if data.len() > self.capacity {
            return Err(io::Error::other(format!("image of {} bytes does not fit a {} byte ring slot", data.len(), self.capacity)));
        }
        self.seq += 1;
        self.bytes += data.len();
        let quiet = if self.seq.is_multiple_of(60) { "" } else { ",q=2" };
        if self.direct {
            // Level 1 compresses the water about as well as 6, several times faster.
            let text = B64.encode(miniz_oxide::deflate::compress_to_vec_zlib(data, 1));
            let chunks: Vec<&[u8]> = text.as_bytes().chunks(4096).collect();
            for (k, chunk) in chunks.iter().enumerate() {
                let more = u8::from(k + 1 < chunks.len());
                let head = if k == 0 { format!("\x1b_G{keys},t=d,o=z{quiet},m={more};") } else { format!("\x1b_Gm={more}{quiet};") };
                out.extend_from_slice(head.as_bytes());
                out.extend_from_slice(chunk);
                out.extend_from_slice(b"\x1b\\");
            }
            return Ok(());
        }
        let name = format!("/{}-{}", self.prefix, self.seq);
        #[cfg(target_os = "linux")]
        {
            let (path, ptr) = &self.slots[self.next];
            // SAFETY: the slot's mapping is `capacity` bytes, at least `data.len()`, and lives
            // until `drop`; this ring is its only writer.
            unsafe { std::slice::from_raw_parts_mut(*ptr, data.len()) }.copy_from_slice(data);
            std::fs::hard_link(path, format!("/dev/shm{name}"))?;
            self.next = (self.next + 1) % self.slots.len();
        }
        #[cfg(not(target_os = "linux"))]
        sys::write_shm(&name, data)?;
        out.extend_from_slice(format!("\x1b_G{keys},t=s,S={}{quiet};{}\x1b\\", data.len(), B64.encode(&name)).as_bytes());
        self.links.push_back(name);
        // The slot this link points at is about to be overwritten, so Ghostty has long read it.
        while self.links.len() > self.keep {
            if let Some(old) = self.links.pop_front() {
                sys::unlink(&old);
            }
        }
        Ok(())
    }
}

impl Drop for ShmRing {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        for (path, ptr) in &self.slots {
            if !ptr.is_null() {
                // SAFETY: the mapping `new` made, `capacity` bytes, not used after this.
                let _ = unsafe { rustix::mm::munmap(ptr.cast(), self.capacity) };
            }
            let _ = std::fs::remove_file(path);
        }
        for name in &self.links {
            sys::unlink(name);
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
    /// The left button let go at a 1-based cell column and row.
    Release {
        /// 1-based cell column.
        col: usize,
        /// 1-based cell row.
        row: usize,
    },
    /// The pointer moved to a 1-based cell column and row with the left button held.
    Drag {
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
    /// A Kitty graphics answer (`ESC _ G i=..;OK ESC \`): `ok` unless it reports an error.
    Graphics {
        /// The answer is `OK`.
        ok: bool,
    },
    /// Any other escape sequence, including other mouse reports.
    Other,
}

/// Splits raw input into events: focus reports (`CSI I`, `CSI O`), SGR mouse reports
/// (`CSI < b;x;y M`, or `m` for a release), other CSI sequences, Kitty graphics answers, a
/// late XTVERSION answer, a lone Esc, Esc chords, and single-byte keys.
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
                ([0, x, y], b'm') => Input::Release { col: *x, row: *y },
                ([32, x, y], b'M') => Input::Drag { col: *x, row: *y },
                ([b @ (64 | 65), x, y], b'M') => Input::Scroll { col: *x, row: *y, up: *b == 64 },
                ([35, x, y], b'M') => Input::Move { col: *x, row: *y },
                _ => Input::Other,
            });
            i += end + 1;
        } else if rest.starts_with(b"\x1b[") {
            let Some(end) = rest[2..].iter().position(|b| (0x40..=0x7e).contains(b)) else { break };
            i += end + 3;
            events.push(Input::Other);
        } else if let Some(prefix) = [b"\x1b_Gi=".as_slice(), b"\x1bP>|"].into_iter().find(|p| rest.len() > 1 && (rest.starts_with(p) || p.starts_with(rest))) {
            // A string up to ST. Only these two starts are taken for one, and only printable
            // text inside, so Alt with `_` or `P` stays a chord.
            let end = rest.windows(2).position(|w| w == b"\x1b\\");
            let inside = &rest[1..end.unwrap_or(rest.len())];
            // A read can end between the ST's two bytes.
            let printable = inside.strip_suffix(b"\x1b").unwrap_or(inside).iter().all(|b| (0x20..0x7f).contains(b));
            match end {
                Some(end) if printable && end >= prefix.len() => {
                    let body = &rest[2..end];
                    events.push(if rest[1] == b'_' { Input::Graphics { ok: body.split(|&b| b == b';').nth(1) == Some(b"OK") } } else { Input::Other });
                    i += end + 2;
                }
                None if printable && rest.len() < 512 => break,
                _ => {
                    i += 2;
                    events.push(Input::Other);
                }
            }
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

    /// A sent image is a shm object under a fresh name, holding its bytes until the terminal
    /// unlinks it, and dropping the ring removes every name the terminal left.
    #[cfg(unix)]
    #[test]
    fn ring_sends_shm_objects() {
        let mut ring = ShmRing::new(2, 16).expect("ring");
        let mut out = Vec::new();
        for image in [[1u8; 8], [2; 8], [3; 8]] {
            ring.transmit(&mut out, &image, "a=t").expect("transmit");
        }
        assert!(ring.transmit(&mut out, &[0; 17], "a=t").is_err(), "an image larger than a slot is refused");
        let names: Vec<String> = ring.links.iter().cloned().collect();
        assert_eq!(names.len(), 2, "the oldest name is unlinked once two newer ones are out");
        // macOS shm objects can only be read through a mapping.
        let read = |name: &str| -> Option<Vec<u8>> {
            use rustix::{mm, shm};
            let fd = shm::open(name, shm::OFlags::RDONLY, rustix::fs::Mode::empty()).ok()?;
            // SAFETY: a fresh read-only mapping of the object's first 8 bytes, copied out and
            // unmapped at once.
            unsafe {
                let ptr = mm::mmap(std::ptr::null_mut(), 8, mm::ProtFlags::READ, mm::MapFlags::SHARED, &fd, 0).ok()?;
                let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), 8).to_vec();
                mm::munmap(ptr, 8).ok()?;
                Some(bytes)
            }
        };
        assert_eq!(read(&names[1]), Some(vec![3; 8]));
        let text = String::from_utf8(out).expect("UTF-8");
        assert!(text.contains(&format!("t=s,S=8,q=2;{}", B64.encode(&names[1]))), "{text}");
        drop(ring);
        assert!(names.iter().all(|n| read(n).is_none()), "drop unlinks what is left");
    }

    /// A direct ring sends the image zlib-compressed in base64 chunks of at most 4096 bytes:
    /// the keys on the first, `m=1` on all but the last, and the chunks inflate back to it.
    #[test]
    fn direct_ring_chunks_and_inflates() {
        let mut ring = ShmRing::direct();
        // Scrambled bytes compress badly, so they take many chunks.
        let image: Vec<u8> = (0..60_000u32).flat_map(|n| n.wrapping_mul(2_654_435_761).to_le_bytes()).collect();
        let mut out = Vec::new();
        ring.transmit(&mut out, &image, "a=T,f=32,s=400,v=150,i=7").expect("transmit");
        let text = String::from_utf8(out).expect("ASCII");
        let commands: Vec<&str> = text.split_terminator("\x1b\\").collect();
        assert!(commands.len() > 2, "{} chunks", commands.len());
        let mut payload = String::new();
        for (k, command) in commands.iter().enumerate() {
            let (keys, data) = command.strip_prefix("\x1b_G").and_then(|c| c.split_once(';')).expect("a graphics command");
            assert!(data.len() <= 4096 && (data.len() % 4 == 0 || k + 1 == commands.len()), "chunk {k} is {} bytes", data.len());
            assert_eq!(keys.starts_with("a=T,f=32,s=400,v=150,i=7,t=d,o=z,q=2,"), k == 0, "{keys}");
            assert!(keys.split(',').any(|key| key == if k + 1 < commands.len() { "m=1" } else { "m=0" }), "{keys}");
            payload.push_str(data);
        }
        let inflated = miniz_oxide::inflate::decompress_to_vec_zlib(&B64.decode(payload).expect("base64")).expect("zlib");
        assert_eq!(inflated, image);
        assert_eq!(ring.bytes, image.len());
    }

    /// Answers recorded from real terminals to `probe`, and silence.
    #[test]
    fn caps_from_recorded_answers() {
        let caps = |reply: &[u8], colorterm: Option<&str>| Caps::parse(reply, colorterm, false);
        let ghostty = caps(b"\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\\x1b[6;21;10t\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", Some("truecolor"));
        assert_eq!(
            ghostty,
            Caps { kitty_shm: true, kitty_direct: true, sixel: None, cell_px: Some((10, 21)), truecolor: true, name: Some("ghostty 1.3.1".to_string()) }
        );
        let wezterm = caps(b"\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\\x1b[?1;0;65536S\x1b[?2;0;1600;990S\x1b[6;22;10t\x1bP>|WezTerm 20260703-142320-59d94d19\x1b\\\x1b[?65;4;6;18;22;52c", Some("truecolor"));
        assert_eq!((wezterm.kitty_shm, wezterm.sixel, wezterm.cell_px), (true, Some(Sixel { colors: 65536, max: Some((1600, 990)) }), Some((10, 22))));
        let xterm = caps(b"\x1b[?1;0;256S\x1b[?2;0;2000;2000S\x1bP>|XTerm(390)\x1b\\\x1b[?63;1;2;4;6;9;15;16;22;28c", None);
        assert_eq!(
            xterm,
            Caps { sixel: Some(Sixel { colors: 256, max: Some((2000, 2000)) }), truecolor: true, name: Some("XTerm(390)".to_string()), ..Caps::default() }
        );
        let foot = caps(b"\x1b[?1;0;1024S\x1b[?2;0;10000;10000S\x1b[6;13;6t\x1bP>|foot(1.16.2)\x1b\\\x1b[?62;4;22c", Some("truecolor"));
        assert_eq!(
            (foot.kitty_direct, foot.sixel, foot.cell_px, foot.truecolor),
            (false, Some(Sixel { colors: 1024, max: Some((10000, 10000)) }), Some((6, 13)), true)
        );
        // mlterm fails the size query and still has sixel.
        let mlterm = caps(b"\x1b[?1;0;1024S\x1b[?1;3;0S\x1b[6;16;10t\x1bP>|mlterm(3.9.3)\x1b\\\x1b[?63;1;2;3;4;6;7;15;18;22;29c", Some("truecolor"));
        assert_eq!(mlterm.sixel, Some(Sixel { colors: 1024, max: None }));
        // tmux answers the sixel queries and the device attributes itself, before Ghostty's
        // Kitty answers come back through the passthrough.
        let tmux = caps(b"\x1b[?1;0;1024S\x1b[?2;3;0S\x1b[6;21;10t\x1bP>|tmux 3.4\x1b\\\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\", Some("truecolor"));
        assert_eq!((tmux.kitty_shm, tmux.kitty_direct, tmux.sixel), (true, true, Some(Sixel { colors: 1024, max: None })));
        // GNOME Terminal (VTE) without sixel refuses the sixel queries.
        let vte = caps(b"\x1b[?1;1S\x1b[?2;1S\x1bP>|VTE(7600)\x1b\\\x1b[?61;1;21;22c", Some("truecolor"));
        assert_eq!(vte, Caps { truecolor: true, name: Some("VTE(7600)".to_string()), ..Caps::default() });
        // Over SSH the terminal cannot open the pond's shared memory, but takes inline images.
        let ssh = caps(b"\x1b_Gi=31;ENOENT:Failed to open shared memory\x1b\\\x1b_Gi=32;OK\x1b\\\x1b[?62;52c", None);
        assert_eq!((ssh.kitty_shm, ssh.kitty_direct, ssh.truecolor), (false, true, true));
        assert_eq!(caps(b"", None), Caps::default());
        assert_eq!(Caps::parse(b"", None, true), Caps { truecolor: true, ..Caps::default() });
    }

    #[test]
    fn parses_mixed_input() {
        let input = b"\x1b[Of\x1b[<0;12;7M\x1b[<0;12;7m\x1b[<64;3;4M\x1b[<65;3;4M\x1b[<35;9;2M\x1b[<32;9;2M\x1b[Aq\x1bOP\x1bx\x1b_Gi=100;OK\x1b\\\x1b_Gi=31;ENOENT:gone\x1b\\\x1bP>|foot(1.16.2)\x1b\\\x1b_x\t?1\x1b[I\x03\x1b";
        let expected = [
            Input::Focus(false),
            Input::Key(b'f'),
            Input::Click { col: 12, row: 7 },
            Input::Release { col: 12, row: 7 },
            Input::Scroll { col: 3, row: 4, up: true },
            Input::Scroll { col: 3, row: 4, up: false },
            Input::Move { col: 9, row: 2 },
            Input::Drag { col: 9, row: 2 },
            Input::Other,
            Input::Key(b'q'),
            Input::Other,
            Input::Other,
            Input::Graphics { ok: true },
            Input::Graphics { ok: false },
            Input::Other,
            Input::Other,
            Input::Key(b'x'),
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
        let input = b"f\x1b[<0;12;7M\x1b[<32;10;7M\x1b[<0;10;7m\x1b[<35;9;2M\x1b[A\x1bOPq\x1b_Gi=7;EINVAL:bad\x1b\\t\x1b[I";
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

    /// Random bytes, as line noise or a confused terminal might send, never panic, and the
    /// parser never claims more bytes than it was given. Half are drawn from the bytes escape
    /// sequences are made of, so the parser's unfinished states are reached too.
    #[test]
    fn random_bytes_never_panic() {
        const PARTS: &[u8] = b"\x1b[<;0123456789MmABIO_Gi=\\~";
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let small = |n: u64| usize::try_from(n).expect("a small number fits usize");
        for _ in 0..5000 {
            let bytes: Vec<u8> = (0..small(next() % 48))
                .map(|_| if next() % 2 == 0 { PARTS[small(next()) % PARTS.len()] } else { u8::try_from(next() % 256).expect("below 256") })
                .collect();
            let (_, used) = parse_input(&bytes);
            assert!(used <= bytes.len(), "used {used} of {} bytes: {bytes:?}", bytes.len());
        }
    }
}
