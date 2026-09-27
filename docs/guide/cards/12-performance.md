# Performance

> Is koi fast and light enough, and how would you know?

Most of what koi costs is the terminal's work. `docs/PERFORMANCE.md` found that Ghostty uses about 23% of a CPU core for any 60 fps animation, however little changes. So what matters most is how often koi sends a frame and how many bytes it carries, then koi's own time per frame.

koi sends 60 frames a second only while the window has focus or something is happening, 8 when the window is in the background and the pond is calm, and nothing when nothing changed. Large images go through shared memory, and the drawing runs on the GPU when there is one.

Press `d` to see a live stats line. The numbers in `docs/PERFORMANCE.md` come from prototypes built before the game, not the game itself, and no benchmark or CI job checks speed.

::: medium
### What matters, and where to see it

| Resource | Why it matters here | Where to see it |
|---|---|---|
| Terminal CPU | The terminal parses and redraws every frame koi sends | `top`, per process |
| koi's time per frame | It has to fit in 16.7 ms at 60 fps | `d`: build p50 and p99 |
| Bytes to the terminal | More bytes, more parsing; slow over SSH | `d`: B/frame |
| Shared memory | Every ring slot holds a whole image, in RAM | `d`: shm MB/s |
| Binary size | koi ships as one file | `ls -l` |
| Startup | koi waits for the terminal to answer its questions | not measured |

### The stats line

Press `d` (the `input.stats` key). The line is built here:

@excerpt src/main.rs:724-732

Reading it left to right:

- **theme, protocol, terminal, backend**: what koi is drawing, how it reaches the terminal, and whether the GPU or CPU renders.
- **fps, sent**: frames per second koi prepared, and how many of those it actually wrote. A frame where nothing changed isn't written (`src/main.rs:794`), so `sent` can be lower.
- **target (reason)**: the rate koi is aiming for and why: `focused`, `food`, `input`, `calm`, or `slow terminal` when the terminal couldn't keep up.
- **build p50, p99**: the median and slowest milliseconds per frame, over the last 120 frames.
- **B/frame**: bytes written to the terminal per sent frame.
- **shm MB/s**: image bytes going through the shared-memory ring each second.
- **poses/s**: koi images drawn per second.
- **water**: the size of the water image in pixels.

### How the rate adapts

Each frame, the loop checks for reasons to run fast: focus, food, recent input, ripples, a darting koi, petting, a fading HUD (`src/main.rs:425-436`). With none, it drops to `fps.unfocused_calm`. Some drawing methods also have a cap. Inline Kitty images and sixel top out at 20 or 30 fps, since each frame is many bytes for the terminal to parse (`src/layers.rs:55-62`).

### What `docs/PERFORMANCE.md` measured

It ran each prototype in a 160x45 Ghostty window and recorded CPU for Ghostty and for the game separately. It took 10 s averages, three runs each, and reported the median (`docs/PERFORMANCE.md:7`). Its conclusion is the design koi uses, with an estimate of 27 to 30% of a core in total at 60 fps. It says plainly that the finished game was not measured the same way (`docs/PERFORMANCE.md:92`).

::: check You add a feature. With the window focused, the stats line shows build p99 at 25 ms and `target 60 (focused)`. Is koi slow, or the terminal?
Not the terminal alone. `build` is timed from before the simulation steps (`src/main.rs:686`) to after the frame is written and flushed (`src/main.rs:797-798`), so it includes the terminal's write. If the write alone took longer than the frame interval, the ceiling would halve and the reason would read `slow terminal` (`src/main.rs:799-800`). It still says `focused`, so every write fit in the 16.7 ms interval. At least 8 ms of the 25 went to koi's own work: stepping, rendering or encoding.
:::

### Measuring without touching your screen

Use a release build, since a debug build isn't optimized. `scripts/vshot.sh` runs koi in Ghostty on a private virtual display with audio off, and saves a screenshot and a short clip. That's the way to see a change. It isn't a way to time one. Ghostty there draws with software OpenGL and koi's GPU work runs on lavapipe, a Vulkan driver on the CPU, and the script's own header says its CPU numbers aren't representative (`scripts/vshot.sh:9-11`).
:::

::: high
### What the stats line leaves out

- **The water isn't in shm MB/s.** The number reads the main ring (`src/main.rs:828`), which carries the koi, the food and the HUD. On the usual Kitty path the water goes through a second ring (`src/layers.rs:492-501`), which isn't counted.
- **Two kinds of bytes.** For inline Kitty images, shm MB/s counts pixels before compression (`crates/koi-term/src/lib.rs:685`), and B/frame counts the compressed base64 actually written.
- **p99 of 120 samples** is about the slowest frame of the last two seconds at 60 fps, not a long-run tail.

### Release profiles

`cargo build --release` uses Cargo's defaults. What ships uses `dist`, which adds whole-program optimization and strips symbols. The web build uses the same settings and also aborts on panic:

@excerpt Cargo.toml:64-78

This shows the local release binary next to the three songs compiled into it. Look at how much of the size is music:

@run du -h target/release/koi assets/music/builtin

The songs are about 6.6 of the 17 MB. The unstripped local build is larger than what ships: the README gives 14 MB for the `dist` binary.

### Memory

Nothing measures memory. The largest use is the shared-memory rings. `check` limits the settings that grow them: at most 50 koi, `water_px` up to 16, `fish_px` up to 64 (`src/config.rs:361-366`).

::: inferred
The ring that carries the water, or in tmux the whole frame, has 6 slots (`src/layers.rs:15`). On the usual path they're small, since the water is sent at 2 pixels per cell. In tmux and in pixel themes, each slot holds the whole window in RGBA: at 1600x945 pixels that's about 6 MB a slot, so about 36 MB of `/dev/shm`.
:::

### Gaps

- There are no benchmarks and no performance check in CI. A slowdown fails nothing.
- The CPU tables are from prototypes. The finished game's cost is an estimate.
- Startup isn't timed. The terminal probe waits up to 2 s for answers (`src/main.rs:160`), though most terminals answer at once and end the wait early.
- In the browser, the canvas is limited to 1.2 million pixels because the koi are drawn on one CPU core (`site/main.js:12-16`).
:::
