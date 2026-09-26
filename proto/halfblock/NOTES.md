# halfblock prototype

Answers one question: can a half-block koi pond hold a steady 60 fps in the terminal? Yes, with plenty of headroom.

Each cell is `▀` with a 24-bit foreground (top pixel) and background (bottom pixel), so a 160x45 terminal is a 160x90 pixel pond. Each frame diffs against what is on screen, sends only the changed cells, skips repeated color escapes, and goes out as one write wrapped in synchronized output (`CSI ?2026h` / `?2026l`).

## Run

```
cargo build --release
./target/release/halfblock                              # interactive
./target/release/halfblock --bench 600 --size 160x45    # headless, no TTY needed
```

Keys: click or `f` drops food, `d` toggles the stats overlay (fps, frame work time p50/p99, bytes of the last frame), `q`, Esc or Ctrl-C quits.

## Numbers

Release build on this machine. The bench drops food every 120 frames (every 2 s at 60 fps), which is busier than a calm session. "Frame build" covers the sim step, shading, diffing, encoding and writing to a sink.

| size (cells) | avg build | p50 | p99 | full repaint | avg bytes/frame | max bytes/frame | at 60 fps |
|---|---|---|---|---|---|---|---|
| 160x45 | 0.24 ms | 0.24 ms | 0.31 ms | 139 KB | 47 KB | 66 KB | 2.8 MB/s |
| 240x70 | 0.34 ms | 0.34 ms | 0.41 ms | 310 KB | 45 KB | 64 KB | 2.7 MB/s |

Interactive run in a pty at 240x70, with no food, measured by the overlay: 60.0 fps, work p50 0.33 ms, p99 0.41 ms, about 5 KB per frame.

## What mattered

- With an exact diff, fading ripples change almost every cell by 1 or 2 color levels each frame, so the diff sends more than a full repaint: 172 KB/frame at 160x45. Re-sending a cell only once a channel has drifted more than 2 levels from what is on screen cuts that to 47 KB, and the error is too small to see. Tolerance 1 gave 83 KB, tolerance 4 gave 22 KB.
- CPU is not the limit. The limit is how many bytes the terminal has to parse. A full repaint of 240x70 is 310 KB, or 18.6 MB/s if every frame changed everything.

## Caveats

- These figures only cover building each frame. Nothing here measures how fast Ghostty parses and draws the stream. That needs a check by eye in Ghostty, with the `d` overlay on.
- tmux: 24-bit color needs `set -as terminal-features ',xterm-ghostty:RGB'` (or `Tc`). tmux 3.4+ understands mode 2026. Older versions pass it through or drop it, so tearing is possible there. tmux also re-encodes the output, which adds latency and CPU.
- A pixel is roughly square only if the font's cells are about 1:2. The fish look stretched on other fonts.
- Resize rebuilds the water and keeps the fish. Food is lost.
- The first fps reading after start can show about 61 because of how the first window is counted.
- Mouse capture uses crossterm's `EnableMouseCapture`, which also reports every mouse move. The program ignores those events, but they still arrive.
