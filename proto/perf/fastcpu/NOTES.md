# fastcpu: cheap game side at --px 8

Copy of `proto/kitty` with the renderer and frame transport reworked so the game costs
far less CPU at `--px 8`. The look matches `--px 8`: same crisp koi, same ripples.

## Run it

```sh
cargo build --release          # .cargo/config.toml sets target-cpu=native
./target/release/koi-fastcpu --px 8                  # defaults: --medium ring --water 2 --threads 1
./target/release/koi-fastcpu --px 8 --water 4        # coarser water sim, cheaper
./target/release/koi-fastcpu --bench 600 --px 8 --size 155x85 --medium ring
```

Keys as before: click drops food, `f` drops food at random, `d` toggles the overlay
(it now also shows water divisor and thread count), `q` or Ctrl-C quits.

## What changed and why it is cheaper

Profile of the stock build at `--px 8` (real window is 1240x1352, 1.7 Mpx): 88% in
`render`, spread over the fish loop (libm `sinf` per pixel), the refraction pass and two
full-frame copies. Each fresh shm object also cost about 3 ms/frame of kernel time
(page faults and zeroing).

1. **Ring medium (`t=f`, default).** Three persistent shm objects are mapped once and
   the frame is rendered straight into one of them. It is sent as a plain file path
   under `/dev/shm`, which Ghostty reads without deleting, so no page is allocated or
   faulted after setup. This cut game `sys` time from about 1.0 s to 0.06 s per
   300 frames. `S=` is left out on purpose: Ghostty's limited file read fails when the
   file is exactly `S` bytes long.
2. **RGBA (`f=32`) instead of RGB.** Ghostty converts every RGB frame to RGBA (wuffs
   swizzle plus an allocation) before upload. Sending RGBA skips that on the Ghostty
   side and lets the renderer move whole `u32` pixels.
3. **Water at half resolution (`--water K`, power of two, default 2).** The wave grid is
   `w/K x h/K`. Wave speed is scaled (`c^2 = 0.5/K^2`) so ripples move and space out in
   image pixels like the full-res sim. Refraction offsets and glint are bilinearly
   upsampled per pixel in the same pass. A `--bench` dump at K=2 is visually the same
   as the stock build (ripples slightly softer).
4. **Calm runs are copied.** Where both water nodes of a cell give zero offset and at
   most one level of glint, the pixels are a straight copy of the layer. Runs of calm
   cells become one `copy_from_slice`. Allowing one glint level (invisible) cut live
   instructions by about 30%.
5. **Fish loop vectorizes.** A branch-free polynomial sine replaces libm, divisions
   became reciprocal multiplies, the per-pixel early exit is gone (zero weights leave
   the pixel unchanged) and the row loop uses an exclusive range. Layer phase went from
   7.1 ms to 2.9 ms single-threaded. Each fish only walks the rows and per-row spans of
   its rotated bounding box plus the shadow offset.
6. **Floor restore only where fish were.** The layer keeps last frame's fish boxes and
   restores only those from the floor, instead of copying the full 6.7 MB floor.
7. **`--threads N`** splits rows across `std::thread::scope` threads for both phases.
   It lowers frame latency but adds total CPU, so the default is 1 thread.
8. **SIGHUP/SIGTERM and pty write errors exit through cleanup**, so the ring files do
   not outlive a closed window in `/dev/shm`.

## Numbers (noisy: other agents were running, so treat as rough)

Bench, 155x85 cells at `--px 8` (1240x1360), 1 thread, food every 45 frames:

| build | total ms/frame |
|---|---|
| stock `koi-kitty` (RGB, shm) | 18.5 |
| fastcpu `--medium shm` | 8.6 |
| fastcpu `--medium ring` (default) | 5.2 |

Live Ghostty window (160x45, two food drops, 8 s pidstat), game CPU in percent of one
core: stock 100, default about 30 to 32, `--water 4` about 23 to 26, `--threads 4`
about 32 to 34. Ghostty's own CPU swung between 17% and 45% run to run under the
shared load, so I could not compare it reliably; that is for the Measure phase.

## Command lines for the Measure phase

```sh
/home/jeremy/Documents/workspace/cli-game/proto/perf/fastcpu/target/release/koi-fastcpu --px 8
/home/jeremy/Documents/workspace/cli-game/proto/perf/fastcpu/target/release/koi-fastcpu --px 8 --water 4
/home/jeremy/Documents/workspace/cli-game/proto/perf/fastcpu/target/release/koi-fastcpu --px 8 --medium shm
```

The third line isolates the ring medium's effect on Ghostty (same renderer, fresh shm
object per frame).

## What did not work

- **Fixed-point refraction offsets.** Slower than float: the signed divisions by 256
  and the glint division cost more than the saturating float casts (+40% instructions).
- **Per-frame water-node grid** (offsets and calm flags once per frame, rows only
  interpolated inside non-calm runs). Pixel-identical output, but about 25% more
  instructions live: live frames are mostly non-calm, so the extra pass does not pay.
- **Chunked calm check** (16 cells at a time instead of exact runs). No gain.
- **Flushing tiny wave heights to zero.** A few thousand subnormals appear per frame,
  but flushing them changed nothing measurable, so it is not in.
- **More threads.** 4 threads use more total CPU than 1 (thread spawns plus cache
  traffic); the game holds 60 fps on one thread at about 5 ms per frame.

## Notes for whoever runs it

- `measure.sh` starts the game from a shell that has `TMUX` set, so the game wraps its
  output in tmux passthrough (the overlay shows `| tmux`). Ghostty still draws it.
  This is the same for every variant.
- In my checks, `xdotool ... click 1` against the window did not always drop food,
  while `mousedown 1 sleep 0.1 mouseup 1` did. The game code for input is unchanged
  from `proto/kitty`.
