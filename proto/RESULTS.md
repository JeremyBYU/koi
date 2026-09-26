# 60 fps spike results

Question: can we render a calm animated koi pond at a steady 60 fps in the terminal?

Short answer: on the program side, yes, by a wide margin, for both approaches. What is still unknown is whether Ghostty keeps up with what we send. That needs a look by eye, and the commands for it are below.

Two spikes, same content (height-field water, 5 koi, food and ripples, `d` overlay, `q` quits):

- `proto/halfblock`: each cell is `▀` with a 24-bit fg/bg, so 2 pixels per cell. The diff sends only cells whose color has moved more than 2 levels.
- `proto/kitty`: renders an RGB image (4 image px per cell by default) and replaces image id 1 every frame with the Kitty graphics protocol. By default the image goes through shared memory, so only about 125 bytes per frame go down the pty.

## Verified

I re-ran everything myself on 2026-09-24 (Threadripper 7960X, release builds, 600 frames, stdin from `/dev/null`, no TTY).

- Both `cargo build --release` runs are clean: 0 warnings, 0 errors after touching `main.rs`. I fixed nothing because nothing was broken.
- Both exit 0 on `q` in a pty and restore the terminal (leave alt screen, mouse off, cursor shown). The kitty spike also deletes its image.
- The kitty benchmark leaves no `/dev/shm` objects behind.
- My numbers match the builder reports within noise.

"Build" is one frame's sim step, render, encode and write to a sink, in ms. The budget at 60 fps is 16.7 ms.

| spike | size (cells) | image px | build avg | build p99 | bytes/frame down the pty | at 60 fps |
|---|---|---|---|---|---|---|
| halfblock | 160x45 | 160x90 | 0.25 | 0.31 | 47,090 avg, 66,418 max (first frame 139,441) | 2.8 MB/s |
| halfblock | 240x70 | 240x140 | 0.35 | 0.41 | 45,308 avg, 63,918 max (first frame 310,242) | 2.7 MB/s |
| kitty shm | 160x45 | 640x360 | 1.75 | 2.23 | 123 | ~0 (675 KB/frame via shm) |
| kitty file | 160x45 | 640x360 | 1.69 | 1.82 | 163 | ~0 (via temp file) |
| kitty direct | 160x45 | 640x360 | 1.89 | 2.05 | 923,707 | 55.4 MB/s |
| kitty zlib | 160x45 | 640x360 | 4.33 | 5.26 | 326,649 | 19.6 MB/s |
| kitty shm | 240x70 | 960x560 | 4.04 | 4.57 | 124 | ~0 (1.6 MB/frame via shm) |
| kitty file | 240x70 | 960x560 | 3.91 | 4.31 | 164 | ~0 |
| kitty direct | 240x70 | 960x560 | 4.42 | 5.49 | 2,155,207 | 129.3 MB/s |
| kitty zlib | 240x70 | 960x560 | 8.50 | 10.03 | 550,343 | 33.0 MB/s |
| kitty shm, `--px 10` | 160x45 | 1600x900 | 11.13 | 12.06 | 125 | ~0 (4.2 MB/frame via shm) |

Notes on the numbers:

- Halfblock uses about 2% of the frame budget. Its cost is the ~45 KB/frame of escape codes Ghostty has to parse.
- Kitty spends almost all its time rasterizing the pond (1.55 ms at 160x45, 3.6 ms at 240x70, 9.6 ms at `--px 10`). The water sim itself is 0.05 to 0.5 ms. It is single-threaded, so there is room to go faster.
- Kitty `direct` and `zlib` are fallbacks for remote sessions only. 55 to 130 MB/s of base64 down a pty is not a calm-app workload.
- Neither benchmark measures anything on the terminal side. With shm, the kitty overlay will say 60 fps even if Ghostty drops frames, because almost nothing goes through the pty to push back.

## What cannot be measured headlessly

1. **Ghostty's real paint rate.** For halfblock: can it parse ~45 KB/frame at 60 Hz without lag? For kitty: can it read a new 0.7 to 4 MB shm image and upload it as a texture 60 times a second?
2. **Visual smoothness.** Tearing, flicker when an image is replaced in place, judder in fish motion, and whether the 2x-per-cell halfblock resolution reads as "pretty" or "blocky".
3. **Terminal CPU.** How much CPU Ghostty itself burns to show each spike. For a leave-it-open app, this matters as much as fps.
4. **tmux behavior.** Your tmux here is 3.4 (understands mode 2026). Its current settings will break both spikes: `allow-passthrough` is `off` (kitty shows nothing), and `terminal-features` has no `RGB` entry for Ghostty (halfblock colors may get quantized).

## Try it in Ghostty

Build once:

```sh
cd ~/Documents/workspace/cli-game/proto
(cd halfblock && cargo build --release)
(cd kitty && cargo build --release)
```

**1. Halfblock, bare Ghostty.** Maximize the window, run, press `d` for the overlay, and click a few times to drop food.

```sh
./halfblock/target/release/halfblock
```

Watch for: steady ripples with no stepping, fish that glide instead of jumping, and no tearing across the screen while ripples spread.

**2. Kitty, bare Ghostty.**

```sh
./kitty/target/release/koi-kitty --probe          # should list shm and file as OK
./kitty/target/release/koi-kitty                  # shm, --px 4
./kitty/target/release/koi-kitty --px 8           # sharper fish, 4x the pixels
./kitty/target/release/koi-kitty --medium file    # compare against shm
./kitty/target/release/koi-kitty --medium direct  # the remote-session fallback, worst case
```

Watch for: flashes or blank frames when the image is replaced, and motion that looks slower or jerkier than halfblock even though the overlay says 60. The overlay cannot see dropped frames here, so trust your eyes over the number.

**3. Terminal CPU.** Run this in a second Ghostty window while each spike runs, and compare the `ghostty` %CPU:

```sh
top -p "$(pgrep -d, ghostty)"
```

**4. tmux.** First turn on what each spike needs:

```sh
tmux set -as terminal-features ',xterm-ghostty:RGB'
tmux set -g allow-passthrough on
tmux set -g mouse on        # only needed for clicks; 'f' drops food either way
```

Then detach and reattach so the RGB feature applies, and run both spikes once in a single full-window pane and once in a split pane:

```sh
./halfblock/target/release/halfblock
./kitty/target/release/koi-kitty
```

What to expect: halfblock should look the same as bare Ghostty, with some extra lag. Kitty will draw at the top-left of the whole window, not the pane, so it only lines up in a full-window or zoomed pane. It may also leave the image behind when you switch windows.

## Recommendation

**Build on halfblock**, unless the Ghostty test shows it is not smooth or you find it too blocky.

Why:

- It already hits 60 fps with over 95% of the frame budget left, and every cost is under our control.
- It works in tmux panes, splits and window switches like any other TUI. You said you may run inside tmux, and the kitty path is broken there in ways this spike cannot fix (wrong placement, leftover images). Fixing that properly means Kitty Unicode-placeholder placements, which is real extra work.
- It does not depend on Ghostty's image pipeline keeping up. That is the biggest unknown in the kitty path, and it only shows up by eye.

What kitty offers is resolution. It has 16x the pixels at `--px 4` and 100x at `--px 10`, so smoother fish edges and finer ripples. If the Ghostty test shows kitty clean at 60 Hz and you mostly run the pond outside tmux, it is the better-looking option, and the switch stays cheap. Both spikes already render the pond into a pixel buffer and differ only in how that buffer reaches the terminal. Keep the game code writing to a pixel buffer, and the kitty backend can come back later as an opt-in mode.

What I need from you: run steps 1 to 4 and tell me (a) whether either one visibly stutters or flickers, (b) roughly what Ghostty's CPU is for each, and (c) whether halfblock's resolution is good enough to look at all day.
