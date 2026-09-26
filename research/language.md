# Language and libraries

## Recommendation

Use **Rust**, with **crossterm** for raw mode, input and mouse, and a small
hand-written renderer on top: our own RGB framebuffer, diffed against the last
frame, written as one buffered write per frame wrapped in synchronized output
(`CSI ? 2026 h` / `l`). For the Kitty graphics path, write the escape sequences
ourselves; the protocol is small. Use **rodio** for local audio, and drive
**mpv** over its IPC socket for streamed music. Pull in **ratatui** only if we
want text widgets (menus, a now-playing line). It is not needed for the pond.

Why: no GC, predictable frame times, full control over the bytes going to the
terminal, mature crates for every piece we need, and cargo is already on the
machine. Go would work, but its main TUI framework (Bubble Tea) is built for
apps that redraw on events, not for a 60 fps pixel loop, and we would end up
bypassing most of it anyway.

The language choice matters less than two tmux facts. Plan for them now:

1. tmux 3.4 (installed here) does not understand Kitty graphics. Images only
   work through `allow-passthrough` plus Unicode placeholders, and tmux does
   not clip or move them.
2. tmux sits between us and Ghostty and re-renders everything we send, so
   every byte costs more inside tmux. Keep frames small.

## Constraints from the project

- 60 fps means a 16.7 ms budget per frame for simulation, rendering and the
  write to the terminal.
- Ghostty supports 24-bit color, the Kitty graphics protocol (including
  Unicode placeholders), synchronized output and SGR-Pixels mouse (mode 1016).
- The user often runs under tmux 3.4 with `TERM=tmux-256color`.
- Installed: Rust (cargo 1.93), Zig 0.14, gcc. Go is not installed.

## Comparison

| | Rust (crossterm, own renderer) | Go (Bubble Tea v2 / tcell) | Zig (libvaxis) | C (notcurses) |
|---|---|---|---|---|
| Frame pacing | Deterministic. No GC. `std::thread::sleep` plus a spin for the last ms, or vsync-free fixed step. | Good. GC pauses are typically well under 1 ms, but allocation-heavy code (Bubble Tea's string-per-frame `View`) raises GC frequency. | Deterministic, manual memory. | Deterministic. |
| Framework fit for 60 fps | crossterm is low level, which is what we want. ratatui diffs a cell buffer and is fine for half-blocks. | Bubble Tea v2 defaults to 60 fps, caps at 120. Model is Elm-style: build a string, renderer diffs it. Works, but wasteful for a full-screen pixel field. tcell gives a cell grid with no frame loop. | libvaxis gives a cell grid, Kitty images, sync output, pixel mouse. Good fit. | Built for this: blitters, planes, pixel graphics. |
| Kitty graphics | ratatui-image (Kitty, Sixel, iTerm2), or hand-written escapes. | Several small packages; commonly hand-written. | Built into libvaxis. | Built in (`NCBLIT_PIXEL`). |
| Audio | rodio 0.22 on cpal. Mature. | oto v3 / gopxl/beep. Mature. | Bind to C (miniaudio). | Bind miniaudio or libao. |
| Mouse | SGR 1006; 1016 pixel reports parse (same wire format) but crossterm exposes them as cells. | Bubble Tea and tcell both do SGR mouse. | SGR pixel mouse supported. | Supported. |
| Ecosystem risk | Low. | Low, but Go not installed. | Zig is pre-1.0; libvaxis tracks specific Zig versions, so upgrades break builds. | notcurses is a large C dependency with its own terminal detection that can fight tmux. |
| Verdict | **Pick this.** | Viable second choice. | Nice API, too much churn. | Powerful, heavier than we need. |

## Details

### Frame pacing and GC

A 60 fps loop needs each frame done in 16.7 ms, every time. Rust has no
collector, so the only jitter comes from our code, the OS scheduler and the
terminal. Go's concurrent collector has sub-millisecond pauses in normal
operation ([Gabor Koos, GC in Go](https://blog.gaborkoos.com/posts/2025-09-12-Garbage-Collection-In-Go.md/)).
The Green Tea collector (experimental in Go 1.25) changed mark CPU cost, not
pause times ([golang/go#73581](https://github.com/golang/go/issues/73581),
[DoltHub test](https://www.dolthub.com/blog/2025-09-26-greentea-gc-with-dolt/)).
So GC is not a disqualifier for Go. A 1 ms pause inside a 16.7 ms budget is
fine. The real Go cost is framework shape, below.

### Bubble Tea specifics

Bubble Tea v2 has a new "Cursed Renderer" based on the ncurses algorithm
([v2 discussion](https://github.com/charmbracelet/bubbletea/discussions/1374)).
`WithFPS` defaults to 60 and is capped at 120
([pkg.go.dev](https://pkg.go.dev/charm.land/bubbletea/v2)). Its model is:
update state, return a full `View()` string, let the renderer diff it. For a
pond that repaints most cells every frame, that means building and parsing a
large ANSI string 60 times a second. People have pushed it
([69 FPS gist](https://gist.github.com/meowgorithm/db4bbb2cffcf66205efd73f272a5364d)),
but a game would fight the architecture. In Go I would use tcell or raw
writes instead, which gives up most of what makes Go pleasant here.

### Terminal write throughput

This is the actual bottleneck, in any language.

- Half-block cells (`▀` with fg and bg truecolor) on a 200x60 terminal give a
  200x120 pixel field. A full repaint is about 12,000 cells at roughly 40
  bytes each, near 500 KB per frame or 30 MB/s at 60 fps. Diffing cuts that a
  lot, since still water barely changes, but ripples touch many cells.
  Mitigations: only emit SGR when color changes, quantize colors slightly so
  neighbors share codes, and use relative cursor moves.
- Kitty images: a 400x240 RGBA frame is 384 KB raw, about 512 KB after
  base64. Zlib (`o=z`) helps. Shared-memory transmission (`t=s`) skips base64
  and works on the same machine; whether Ghostty accepts it for fast
  per-frame updates needs testing in the prototype.
- Wrap each frame in synchronized output so Ghostty paints it atomically.
  Ghostty supports mode 2026. tmux started honoring the application's
  2026 in December 2025 ([tmux#4744](https://github.com/tmux/tmux/pull/4744)),
  after 3.4, so under the installed tmux 3.4 frames can tear.

Rust makes this easy: one `Vec<u8>` reused across frames, one `write_all`
to a locked `stdout`, no allocation in the hot path.

### Image protocol libraries and tmux

- [ratatui-image](https://github.com/ratatui/ratatui-image) handles Kitty,
  Sixel and iTerm2, queries font pixel size, and tracks placement. It is
  aimed at static images inside widgets, not 60 fps streaming, so for the
  pond I would write Kitty escapes by hand: transmit once per frame with a
  fixed image id and replace it, and keep one placement.
- Ghostty supports Kitty Unicode placeholders (U+10EEEE), which is the way
  to make images work inside tmux
  ([Mitchell Hashimoto](https://x.com/mitchellh/status/1818696111999299976)).
  The image data still has to go through tmux as DCS passthrough, which
  needs `set -g allow-passthrough on`
  ([ranger PR](https://github.com/ranger/ranger/pull/3086)).
- Native Kitty and Sixel support in tmux exists as an open PR
  ([tmux#5445](https://github.com/tmux/tmux/pull/5445), still open in
  September 2026; tracking issue
  [tmux#4902](https://github.com/tmux/tmux/issues/4902)). Do not plan on it.

Practical plan: build the half-block renderer first because it works
everywhere including tmux. Add the Kitty path as an option, detected at
startup, with placeholders when `$TMUX` is set.

### Mouse input

Clicking to drop food needs cell-accurate or better coordinates. SGR mouse
(1006) works in every option above. SGR-Pixels (1016) is supported by
Ghostty and uses the same wire format, so crossterm 0.29 parses it but puts
pixel values into the cell fields
([crossterm#873](https://github.com/crossterm-rs/crossterm/issues/873)).
We can enable 1016 ourselves and divide by the cell size. Under tmux, expect
cell coordinates only: tmux parses mouse itself and re-reports in cells
(verify in the prototype). With half-blocks, cell coordinates give 1x2
precision, which is enough to aim food.

### Audio

- Rust: [rodio](https://github.com/RustAudio/rodio) 0.22 on cpal, plays
  WAV/MP3/OGG/FLAC and can mix sources. Good for ambient sounds (plops,
  water) and local music files.
- Go: [oto v3](https://github.com/ebitengine/oto) and
  [gopxl/beep](https://github.com/gopxl/beep) cover the same ground.
- For YouTube playlists, any language would shell out to `mpv` (with
  yt-dlp) and control it over its JSON IPC socket. That is a separate
  research question, but it means music does not favor one language.

### Zig and C

- [libvaxis](https://github.com/rockorager/libvaxis) is the nicest API of
  the bunch for this: Kitty images, synchronized output, pixel mouse,
  capability detection by query instead of terminfo. Zig 0.14 is installed,
  but Zig is pre-1.0 and libvaxis pins Zig versions, so upgrades break
  builds. Audio would mean binding a C library.
- [notcurses](https://github.com/dankamongmen/notcurses) was built for
  exactly this kind of multimedia terminal work, with pixel blitters and
  Kitty support ([notcurses_visual](https://notcurses.com/notcurses_visual.3.html)).
  It is a large C dependency with its own terminal detection, and C gives
  no safety for the simulation code. More than we need.

## What to test in the 60 fps prototype

1. Half-block renderer: frame time and bytes per frame at full screen, in
   Ghostty directly and inside tmux 3.4.
2. Kitty path: replace-image-per-frame at 400x240, direct and through tmux
   passthrough with placeholders. Try `t=s` shared memory.
3. Whether tearing under tmux 3.4 is visible; if so, consider suggesting a
   newer tmux build.
