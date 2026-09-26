# koi-gpu

The kitty prototype with the water sim, floor, koi, shadows, refraction, glints and food pellets all moved into two wgpu compute shaders (`src/pond.wgsl`). It runs headless on Vulkan (RTX 4090, driver 580). The CPU only does fish steering, food, input and the terminal escape. Each frame the GPU writes the finished image into a storage buffer, we copy that into a mappable buffer, read it back, and hand it to Ghostty over shm like before.

## What changed

- `pond.rs` keeps steering only. `splash()` now queues `[x, y, radius, amount]` instead of touching a height field.
- `gpu.rs` holds two height buffers that swap each step. The `wave` pass runs one sim step and adds the step's queued splashes in the same pass. The `shade` pass computes 4 pixels per invocation: the procedural floor, the koi and their shadows at the refracted source pixel, the glint, and the pellets. It packs them straight into f=24 RGB words, or f=32 RGBA with `--fmt 32`, so the CPU does no conversion.
- `ShmRing`: 8 shm files that stay mapped and faulted in. Each frame is copied into the next file, and that file is then hard-linked to the frame's unique name. Ghostty unlinks the frame name after it reads it, and the ring's own name keeps the pages alive. The old path used a fresh `shm_open` + `write` every frame, so it zeroed and faulted 2.7 MB of new pages each frame.
- Image width is rounded up to a multiple of 4 so the 4-pixel RGB packing always lines up. Ghostty scales to `c=`/`r=` anyway.
- SIGHUP/SIGTERM break the loop so the ring files get removed when the Ghostty window closes.
- Removed: temp-file, direct and zlib media, the tmux wrapping and the probe. This variant assumes Ghostty with shm.

## Cost (`--bench 600`, 160x45 cells, --px 8 -> 1280x720)

| step | ms/frame |
|---|---|
| CPU steering + wave submit | 0.016 |
| GPU shade + copy + map wait (wall) | 0.20-0.27 |
| copy mapped frame into the shm ring | 0.10 (f=24), 0.14 (f=32) |
| total | 0.32-0.40, p99 ~0.5-1.6 |

For comparison, the CPU prototype at --px 8 spends about 16 ms per frame, so it cannot hold 60 fps.

Readback cost is small: 2.7 MB at f=24 takes about 0.2 ms from submit to a mapped buffer. Copying out of mapped memory runs as fast as copying from a normal Vec, so the mapped buffer is not uncached. The one-time 0.7 ms per frame that remained came from faulting in fresh destination pages, which the ring removes. `/usr/bin/time` over 3000 bench frames shows 0.24 ms of CPU per frame, so the fence wait blocks rather than spins. In a Ghostty window, pidstat showed koi-gpu at about 3% of one core at 60 fps with `d` reading 60.0 fps and a 0.72 ms build p50. That window was not the measure.sh size and other agents were running, so treat these as rough numbers.

At --px 16 (2560x1440) a frame costs 1.3 ms, so the game side scales fine. What remains is Ghostty's cost to ingest and scale about 166 MB/s at --px 8. This variant cannot reduce that except through the format choice.

## Run

```
cargo build --release
./target/release/koi-gpu [--px 8] [--fmt 24|32]      # defaults: --px 8, --fmt 24
./target/release/koi-gpu --bench 600 [--px N] [--fmt 32] [--size 160x45] [--dump out.ppm]
```

Controls are unchanged: click drops food, `f` drops random food, `d` toggles the overlay (fps, build ms, adapter, format, image size), `q` quits.

The `--dump` output at --px 8 matches koi-kitty's `--bench 600 --dump` frame to the eye: same seed, same fish positions, same ripples.

## For the Measure phase

```
/home/jeremy/Documents/workspace/cli-game/proto/perf/gpu/target/release/koi-gpu --px 8
/home/jeremy/Documents/workspace/cli-game/proto/perf/gpu/target/release/koi-gpu --px 8 --fmt 32
/home/jeremy/Documents/workspace/cli-game/proto/perf/gpu/target/release/koi-gpu --px 6
```

The second line tests whether RGBA saves Ghostty a conversion even though it sends 33% more bytes. The third shows how much of Ghostty's share goes away at a slightly softer image.

## What did not work

- Reusing shm pages through `O_TMPFILE` + `linkat(/proc/self/fd/N)`. The first link works. After Ghostty unlinks that name, the kernel refuses to relink the inode (ENOENT, because `I_LINKABLE` is cleared on the first link). Keeping a named ring file and hard-linking it fixes this.
- `xdotool key --window` (XSendEvent) never reaches Ghostty. Use `windowactivate --sync <id> key d` instead.
