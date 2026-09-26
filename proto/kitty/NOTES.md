# Kitty graphics spike

Question: can we draw a calm, animated koi pond at a steady 60 fps in the terminal?

This spike renders the pond into an RGB buffer and sends each frame to the
terminal as one Kitty graphics image. Every frame reuses image id 1 and
placement id 1. The upload and placement sit inside one synchronized update
(mode 2026), so the new frame replaces the old one in place.

## Run it

```sh
cargo build --release
./target/release/koi-kitty                      # interactive, in Ghostty
./target/release/koi-kitty --probe              # which transmission media the terminal accepts
./target/release/koi-kitty --medium direct      # force shm | file | direct | zlib
./target/release/koi-kitty --px 8               # image pixels per cell width (default 4)
./target/release/koi-kitty --bench 600 --size 160x45 [--medium shm] [--dump frame.ppm]
```

Keys: click drops food, `f` drops food at a random spot, `d` toggles the stats
overlay (fps, build time p50/p99, bytes per frame), `q` or Ctrl-C quits.

The image is `cols * px` wide. Its height follows the real cell aspect from
`TIOCGWINSZ` (a 1:2 cell if the terminal reports no pixel size). Ghostty scales
it to cover the window. The overlay is ordinary text: the image sits at `z=-1`,
under the text.

## Transmission media

At startup the program asks the terminal (`a=q`) which media work, then picks the
first one that answers OK, in this order:

| medium | what goes down the pty | notes |
|---|---|---|
| `shm` (`t=s`) | ~125 bytes | New POSIX shm object per frame. The terminal unlinks it after reading; we also unlink names 120 frames old, so a terminal that never reads them cannot fill `/dev/shm`. |
| `file` (`t=t`) | ~165 bytes | Temp file whose name contains `tty-graphics-protocol`, as Ghostty requires. Costs about the same as shm. |
| `direct` (`t=d`) | base64 RGB, 4096-byte chunks | 55 MB/s at 160x45 and 130 MB/s at 240x70, at 60 fps. |
| `zlib` (`t=d,o=z`) | base64 of zlib-fast RGB | About 1/3 of direct's bytes, but compressing costs 2.5 to 4.5 ms per frame. |

`shm` is the default. `direct` is only a fallback, for remote sessions where the
terminal cannot see our memory or files.

## Benchmark (headless, writes to /dev/null)

Ryzen Threadripper 7960X, release build, 5 koi, a food drop every 45 frames.
"Build" covers sim step + render + encode + write, all in ms.

160x45 cells -> 640x360 px (`--px 4`), RGB 675 KB:

| medium | sim | render | encode+write avg / p99 | total avg / p99 | bytes/frame |
|---|---|---|---|---|---|
| shm | 0.05 | 1.53 | 0.14 / 0.18 | 1.72 / 1.94 | 123 |
| file | 0.05 | 1.55 | 0.09 / 0.11 | 1.69 / 1.86 | 163 |
| direct | 0.05 | 1.55 | 0.27 / 0.31 | 1.87 / 2.04 | 923,707 |
| zlib | 0.05 | 1.57 | 2.55 / 3.25 | 4.17 / 4.90 | 326,649 |

240x70 cells -> 960x560 px (`--px 4`), RGB 1575 KB:

| medium | sim | render | encode+write avg / p99 | total avg / p99 | bytes/frame |
|---|---|---|---|---|---|
| shm | 0.13 | 3.59 | 0.33 / 0.43 | 4.05 / 4.70 | 124 |
| file | 0.11 | 3.60 | 0.18 / 0.22 | 3.89 / 4.28 | 164 |
| direct | 0.12 | 3.64 | 0.64 / 0.72 | 4.40 / 4.80 | 2,155,207 |
| zlib | 0.12 | 3.64 | 4.49 / 5.83 | 8.25 / 9.59 | 550,343 |

At native resolution (160x45 at `--px 10`, 1600x900 px, 4.2 MB per frame),
shm builds a frame in 11.1 ms avg and 12.3 ms p99. That fits in 16.7 ms, but
without much margin. Everything runs on one thread; render is the whole cost,
and it would split across row bands easily.

A pty smoke test (160x45, no real terminal on the other end) held 60.0 fps,
with a build p50 of about 1.9 ms, both with and without `$TMUX`.

## Caveats

- **Not yet seen in Ghostty.** Everything above was measured headless or through
  a plain pty. What we still need to know is what Ghostty does with it: whether
  it can read a new shm image and upload it as a texture 60 times a second, and
  whether same-id replacement is really flicker-free. The overlay fps counts our
  own loop. With shm the pty carries only ~125 bytes a frame, so it never pushes
  back, and a Ghostty that drops frames would still show 60 fps. Watch the
  screen, not only the number.
- **tmux**: with `$TMUX` set, every graphics sequence (plus the cursor-home and
  sync markers) is wrapped in `DCS tmux; ... ST`. This requires
  `set -g allow-passthrough on`. On this machine it is currently `off`, and
  nothing will show until that changes. tmux does not know the image exists:
  - The image is placed at the outer terminal's top-left, not the pane's. It
    only lines up with a single full-window pane, or a zoomed pane with the
    status bar at the bottom.
  - Switching windows can leave the image on screen.
  - tmux swallows the probe replies. When that happens the program assumes shm.
  - Clicks only reach the program when tmux has `mouse on`. `f` works either way.

  Kitty's Unicode-placeholder placements (`U=1`) are the proper fix for panes.
  This spike does not implement them.
- The image is upscaled to fill the window at the default `--px 4`, and it looks
  soft. The water suits that. Fish edges might want `--px 6` to `8`.
- The water sim runs one step per pixel per frame, so ripples move about 60 px/s
  in image pixels. At a different `--px` the same ripple crosses the screen at a
  different speed.
