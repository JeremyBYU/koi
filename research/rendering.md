# Drawing at 60 fps in the terminal

## Recommendation

1. **Composite every frame into one RGB pixel buffer in software.** Water, koi, food and ripples all go into the same framebuffer. The output step is the only part that knows about the terminal. At the resolutions below, this costs a few milliseconds of CPU per frame at most.
2. **Primary output: Kitty graphics over shared memory (`t=s`)**, drawn at reduced resolution (about 2 to 4 image pixels per cell width) and scaled by Ghostty to fill the window. Almost nothing goes through the PTY: about 100 bytes of escape code per frame. Ghostty 1.3.1 (installed here) supports `t=s`. It does not support the protocol's animation frames, so we re-send the whole frame each tick, which is what a game wants anyway.
3. **Fallback and first prototype: half-block (`▀`) cells in 24-bit color, with diffing, wrapped in synchronized output (mode 2026).** This works everywhere, including in tmux and over SSH, and it is simple enough to build in an afternoon. At 200x50 cells a full frame is about 360 KB. It will look good. It is also blocky: 2 vertical "pixels" per cell.
4. **Skip sextants, quadrants, octants and braille for the water.** They give more spatial resolution, but each cell still holds only 2 colors. Smooth gradients, which is what water is, turn into visible artifacts.
5. **tmux:** your tmux is 3.4 with `allow-passthrough off`. Half-blocks work in tmux, but tmux 3.4 ignores the app's mode 2026, so frames can tear, and tmux adds its own parse-and-redraw cost. Kitty images in tmux need `allow-passthrough on` plus Unicode placeholders, and the result is fragile. The simplest plan is to run the pond in its own Ghostty window or tab, outside tmux. Detect `$TMUX` and fall back to half-blocks there.

Build the 60 fps prototype with both outputs behind the same pixel buffer. Then compare them side by side in Ghostty, directly and in tmux, before choosing.

## The options

### 1. Half-block cells, 24-bit color

Each cell draws `▀` (U+2580). Foreground color is the top pixel and background color is the bottom pixel. A 200x50 cell window becomes a 200x100 pixel canvas with exact color per pixel.

- Looks: soft, clean, clearly "terminal art". Gradients stay smooth because every pixel has its own exact color. Pixels are square when the cell aspect ratio is about 1:2, which is typical.
- Resolution: low. A koi is maybe 12x4 pixels. Fine motion below one pixel has to come from color (antialiasing the fish edge into the water), not position.
- Ghostty draws block elements with its built-in sprite renderer, not the font, so cells meet with no gaps or seams ([Ghostty glyph rendering](https://deepwiki.com/ghostty-org/ghostty/5.5.3-glyph-rendering-and-atlases), [Are We Legacy Computing Yet?](https://arewelegacycomputingyet.com/)).
- Works in any truecolor terminal, in tmux, and over SSH.

### 2. Quadrants, sextants, octants, braille

| Glyph set | Sub-pixels per cell | Colors per cell |
|---|---|---|
| Half block `▀▄` | 1x2 | 2 |
| Quadrants `▘▝▖▗…` | 2x2 | 2 |
| Sextants (U+1FB00) | 2x3 | 2 |
| Octants (U+1CD00, Unicode 16) | 2x4 | 2 |
| Braille (U+2800) | 2x4 dots | 1 plus background, with gaps between dots |

Ghostty renders sextants and octants itself and does it well ([Ghostty discussion #4108](https://github.com/ghostty-org/ghostty/discussions/4108), [Are We Legacy Computing Yet?](https://arewelegacycomputingyet.com/)). The catch is the 2-color limit. For each cell you have to pick the 2 best colors and a mask, and on a gradient every choice is wrong somewhere. The result is visible banding and "crawling" edges as the mask changes frame to frame. That suits sharp shapes like line art, text and Doom ([octant demo](https://github.com/reynoldsme/octant)). It does not suit calm water with soft light. Braille is worse: the dots have gaps, so it reads as a pattern rather than a surface.

A hybrid is possible: half-blocks for the water, and sextants only on koi edges for a crisper outline. That adds complexity for a small gain. Leave it for later, if the half-block koi look too blocky.

### 3. Kitty graphics protocol in Ghostty

The program sends real pixel images. Ghostty draws them with the GPU, scaled into a rectangle of cells.

What the spec gives us ([Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)):

- **Transmission medium `t=`**: `d` direct (base64 inside the escape code, in chunks of at most 4096 bytes), `f` file, `t` temp file, `s` POSIX shared memory. For `s`, the name starts with `/` and "the terminal emulator must read the data from the memory object and then unlink and close it."
- **Formats `f=`**: `24` RGB, `32` RGBA, `100` PNG. `o=z` adds zlib compression.
- **Placement**: `c=`/`r=` set the size in cells, and the image is scaled to fit. `X=`/`Y=` give a pixel offset inside the first cell. Sending a placement again with the same image id and placement id "will replace the first. This can be used to resize or move placements around the screen, without flicker."
- **Re-sending image data**: "When re-transmitting image data for a specific id, the existing image and all its placements must be deleted." So each frame is transmit plus place (`a=T`), or you alternate between two image ids. Put the whole thing inside a mode 2026 block so Ghostty presents the frame all at once.
- **`q=2`** suppresses the terminal's replies. Without it, every frame produces an `OK` response that has to be read and thrown away.
- **z-index**: a negative `z=` draws the image under text, so a HUD or help text can sit on top of the pond as normal cells.
- **Animation frames (`a=f`, `a=a`)**: part of the spec, but **not supported in Ghostty**. Mitchell Hashimoto, Jan 2025: "Ghostty doesn't support animation frames" ([discussion #5218](https://github.com/ghostty-org/ghostty/discussions/5218)). This does not matter much. Terminal-side animation plays a canned loop, while the pond is simulated live.
- **Shared memory in Ghostty**: implemented (`readSharedMemory` in `src/terminal/kitty/graphics_image.zig`, per [zenbu-labs/pixel#16](https://github.com/zenbu-labs/pixel/issues/16)).
- **Unicode placeholders (`U=1`)**: supported in Ghostty ([Mitchell Hashimoto](https://x.com/mitchellh/status/1818696111999299976)). This is the part that makes images work inside tmux. See the tmux section.

Performance evidence from others:

- One project streams full-resolution frames into Ghostty over the PTY: a 2928x1836 pane is 21.5 MB per frame in BGRA, "~1.3 GB/s of escape-stream traffic" at 60 fps. It reaches 60 fps, but costs about 1.2 to 1.6 CPU cores for the app plus 1.2 for the terminal, and the PTY is the bottleneck ([zenbu-labs/pixel#16](https://github.com/zenbu-labs/pixel/issues/16)). The lesson: do not send full resolution, and do not send base64 through the PTY if shared memory is available.
- The same issue reports that upscaling a lower-resolution image "visibly softens" it. For water that softness may help. For koi edges it may not. We have to test this.
- Ghostty's image path still copies data more than it needs to. An open PR removes a CPU pixel conversion (swizzle) worth 3.8% of renderer CPU in a profile ([ghostty#14379](https://github.com/ghostty-org/ghostty/pull/14379), not merged as of today). Sending RGBA (`f=32`) instead of RGB may avoid that conversion on OpenGL. Measure it.
- Ghostty renders on its own thread with OpenGL on Linux, synced to the display refresh ([Ghostty features](https://ghostty.org/docs/features), [discussion #7374](https://github.com/ghostty-org/ghostty/discussions/7374)).

### Why not sprites as separate placements?

You could upload the water as one image and each koi as its own image, then move them with placement updates (`p=`, `X=`, `Y=`). That keeps bandwidth tiny. But koi bend and turn every frame, so their sprites change every frame anyway, and ripples distort what is under them. Compositing in software and sending one frame is simpler and removes any layering or ordering bugs. Revisit only if the full-frame upload turns out to be the bottleneck.

## Synchronized output (mode 2026)

Wrap each frame in `CSI ? 2026 h` ... `CSI ? 2026 l`. The terminal holds its screen update until the end marker, so a frame never appears half-drawn. Ghostty supports it. Its terminfo advertises it: `infocmp -x xterm-ghostty` shows `Sync=\E[?2026%?%p1%{1}%-%tl%eh%;` on this machine. Also write each frame with a single `write()` call from one buffer, so the kernel does not split it more than it has to.

In tmux:

- tmux has used the outer terminal's `Sync` capability for **its own** redraws since 3.2 ([tmux CHANGES](https://github.com/tmux/tmux/blob/master/CHANGES)).
- tmux only honors mode 2026 **from the application** from **3.7** on: "Add support for applications to use synchronized output mode (DECSET 2026) to prevent screen tearing during rapid updates" ([tmux#4744](https://github.com/tmux/tmux/pull/4744), merged Dec 2025). tmux 3.7 also fixed several flicker and redraw bugs around sync.
- You have **tmux 3.4**. In 3.4, the pond's 2026 markers are ignored. tmux redraws when it chooses, so a frame can reach the screen half old and half new. It will probably look fine for slow water, and badly on fast ripples. Upgrading to tmux 3.7 or later fixes this.

## Diffing

Keep the previous frame's cells. For each changed cell, emit a cursor move only if it is not directly after the last written cell. Emit fg/bg SGR only if they differ from the current pen. Use one combined SGR (`\e[38;2;r;g;b;48;2;r;g;bm`), then `▀`.

Water will change most cells every frame, so diffing helps less than in a normal TUI. Things that help:

- Quantize colors a little (for example 5 to 6 bits per channel) before comparing. Invisible changes then don't count as changes, and calm areas stay still.
- Do not use per-frame temporal dithering. It turns every cell into a change.
- Keep a "static" region (pond edge, stones, lily pads) and only redraw it on resize.

Kitty images have no partial update in Ghostty. The spec's frame-editing commands are part of the animation feature that Ghostty lacks, so the whole image is re-sent every frame. With shared memory that is a memcpy, not a stream.

## Bandwidth per frame

Assumptions: a window of 200x50 cells as the typical case, and 300x80 for a big fullscreen window. A half-block cell with full SGR is about 36 bytes (`ESC[38;2;` + rgb + `;48;2;` + rgb + `m` + 3-byte `▀`).

| Method | Frame at 200x50 cells | Per second at 60 fps | 300x80 cells, per second |
|---|---|---|---|
| Half-block, full redraw | ~360 KB | ~22 MB/s | ~52 MB/s |
| Half-block, diffed, ~40% cells change | ~140 KB | ~9 MB/s | ~21 MB/s |
| Kitty direct, RGB, image 400x200 px (2x2 per cell) | 240 KB raw, 320 KB base64 | ~19 MB/s | ~46 MB/s (600x320) |
| Kitty direct, RGB, 800x400 px (4x8 per cell) | 960 KB raw, 1.28 MB base64 | ~77 MB/s | ~185 MB/s |
| Kitty direct, near full res ~2000x1000 px | 6 MB raw, 8 MB base64 | ~480 MB/s | too much |
| Kitty shared memory, any size | ~100 B escape through the PTY, plus a memcpy of the raw buffer in the terminal | ~6 KB/s through the PTY | same |

Tens of MB/s through a PTY into Ghostty directly is workable. The same amount through tmux is heavy, because tmux parses every byte and keeps its own grid. Shared memory takes the pixel data off the PTY completely, which is why it is the primary choice.

Shared memory only works when the program and Ghostty are on the same machine. Over SSH, fall back to direct transmission at a low resolution, or to half-blocks.

## How tmux affects each option

Your current setup: tmux 3.4, `allow-passthrough off`, `TERM=tmux-256color`.

| | Directly in Ghostty | In tmux 3.4 | In tmux 3.7+ |
|---|---|---|---|
| Half-block truecolor | Works. Uses 2026. | Works if tmux knows the outer terminal has RGB. Ghostty's terminfo has `Tc`, so it should. No app-level 2026, so some tearing is possible. Extra CPU in tmux. | Works, with 2026 passed through. |
| Sextants/octants | Works | Works | Works |
| Kitty, placed at the cursor | Works | Broken. With passthrough, the image lands wherever Ghostty's real cursor is, which tmux controls. It also does not move with panes or windows. | Same problem |
| Kitty with Unicode placeholders (`U=1`) | Works | Works with `set -g allow-passthrough on`. Each image command must be wrapped in `ESC P tmux; ... ESC \` with inner ESCs doubled. The placeholder cells (U+10EEEE plus diacritics, id in the fg color) flow through tmux as normal text, so the image follows the pane. Passthrough is dropped for panes that are not visible unless it is set to `all`. | Same |
| Kitty shared memory in tmux | Works | The pixels never pass through tmux; only the short escape does. So bandwidth is fine. Placement is still the problem, so placeholders are still needed. | Same |

Other tmux points:

- tmux 3.4 has a 1 MiB limit on a single escape sequence (`INPUT_BUF_LIMIT 1048576` in `input.c`; later made configurable as `input-buffer-size`). Kitty chunks are at most 4 KB, so this is not a problem, but it rules out stuffing a whole frame into one DCS.
- To give tmux truecolor and sync explicitly: `set -as terminal-features ',xterm-ghostty:RGB:sync'`.
- Existing Rust support: [ratatui-image](https://github.com/benjajaja/ratatui-image) knows Ghostty implements Kitty with Unicode placeholders. It is aimed at static images, not 60 fps, but it is a useful reference for the tmux wrapping and placeholder encoding.

## What the prototype should measure

1. Half-block at 200x50 and 300x80: frame time, CPU for the game and for Ghostty, directly and in tmux 3.4.
2. Kitty `t=s` at 400x200, 800x400 and full res: the same numbers, plus `f=24` compared to `f=32`.
3. Visual: does Ghostty's scaling of a low-res image look soft or pixelated? Which looks calmer?
4. Tearing: fast ripples in tmux 3.4 compared to directly in Ghostty.
5. Resize: both paths must redraw cleanly when the window changes size. Kitty needs the cell pixel size (`CSI 16 t` or `TIOCGWINSZ`) to pick the image resolution.

## Sources

- Kitty graphics protocol spec: https://sw.kovidgoyal.net/kitty/graphics-protocol/
- Ghostty, no animation frames: https://github.com/ghostty-org/ghostty/discussions/5218
- Ghostty shared memory and full-frame streaming numbers: https://github.com/zenbu-labs/pixel/issues/16
- Ghostty image copy PR: https://github.com/ghostty-org/ghostty/pull/14379
- Ghostty Unicode placeholders: https://x.com/mitchellh/status/1818696111999299976
- Ghostty features: https://ghostty.org/docs/features
- Ghostty redraw and vsync discussion: https://github.com/ghostty-org/ghostty/discussions/7374
- Ghostty glyph and sprite rendering: https://deepwiki.com/ghostty-org/ghostty/5.5.3-glyph-rendering-and-atlases
- Ghostty octants: https://github.com/ghostty-org/ghostty/discussions/4108
- Legacy computing glyph survey: https://arewelegacycomputingyet.com/
- Octant renderer demo: https://github.com/reynoldsme/octant
- tmux app-level 2026 support: https://github.com/tmux/tmux/pull/4744
- tmux changelog: https://github.com/tmux/tmux/blob/master/CHANGES
- tmux 3.4 input buffer limit: https://github.com/tmux/tmux/blob/3.4/input.c
- tmux man page (`input-buffer-size`): https://man7.org/linux/man-pages/man1/tmux.1.html
- ratatui-image: https://github.com/benjajaja/ratatui-image
