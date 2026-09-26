# koi-layers

This variant splits the pond into separate Kitty images, stacked by z-order, instead of sending one full frame.

| Layer | Image | Resolution | Sent when |
|---|---|---|---|
| Water (floor, fish shadows, ripples) | id 1, `z=-1000`, `c=cols,r=rows` | `--water-px` px per cell (default 2); Ghostty scales it up | every frame, or every other frame with `--water-fps 30` |
| Each koi | its own image, `z=-100+k` | `--fish-px` px per cell (default 8, same as `--px 8`); `0` means native screen pixels | only when its pose (heading, tail phase) is not already cached |
| Food pellets | ids 2..9, one per fade level, `z=-10` | native screen pixels | once at startup; after that, only placements change |

In most frames the game sends a few hundred bytes of `a=p` placement moves. It uploads new pixels only for the water and for poses that are not cached yet.

## What changed

- **Water at low resolution, same ripple look.** The height field runs on the coarse grid. Wave speed (`0.5 * ratio^2`), refraction strength (`6 * ratio^2`) and glint (`40 * ratio`) are rescaled by `ratio = water_px / 8`. Ripples therefore travel, bend the floor and shine as they did at `--px 8`. Refraction samples bilinearly, because at `--water-px 2` the offsets are under one pixel. Fish shadows go into the water, so they stay soft and ripple with it.
- **Koi as sprites.** At startup each fish is shaded once into a straight, local-space body texture. A pose is a resample of that texture: rotate by the heading, and shift each column by the tail bend. Poses are cached per fish, keyed by (heading bucket, tail-phase bucket): `--angles` default 120, `--phases` default 12. Each fish keeps up to 48 cached poses and evicts the least recently used with `a=d,d=I`. A cache hit costs one placement move.
- **Sub-cell positioning.** Each placement sets `X=`/`Y=` pixel offsets. The sprite canvas is padded to whole cells, so `c=`/`r=` scaling does not stretch it. Ghostty cannot place an image at a negative position. When a koi hangs off the left or top edge, the game poses an uncached, pre-cropped sprite for that frame (image ids 20+k). Overflow at the right or bottom edge is clipped by the viewport.
- All uploads are RGBA (`f=32`) over shared memory, so Ghostty skips its RGB to RGBA swizzle.
- Dropped from the `kitty` crate: the `--medium`, `--probe` and tmux paths, and the flate2 dependency. This variant is shared memory only.

## Why it should be cheaper

- Ghostty uploads about 400 KB of water per frame, not 5 MB. Fish uploads average about 45 to 80 MB/s at 60 fps, depending on how much the fish are turning or dashing.
- Game CPU no longer scales with the full `--px 8` frame. Water shading covers about 100k pixels. A pose is a resample of about 35k pixels, and only about half of the frames need one.
- Checked in Ghostty 1.3.1 source (`src/renderer/image.zig`, `graphics_storage.zig`):
  - A kitty-state change rebuilds the placement list and sorts it by z. With 7 to 15 placements that is cheap.
  - Textures stay on the GPU until their image is deleted, so switching a placement to a cached pose re-uploads nothing.
  - Re-transmitting an id keeps its placements.
  - `d=I` frees an image only once nothing uses it.
  - Linear filtering scales the water smoothly.

## Rough numbers (this machine, other agents running, not final)

`measure.sh`, 155x78 cells of 12x26 px:

| Flags | Ghostty | Game | Total |
|---|---|---|---|
| (default) | 14.3% | 15.4% | ~30% |
| `--water-fps 30` | 11.9% | 8.6% | ~20% |
| `--water-px 3 --water-fps 30` | 13.2% | 14.2% | ~27% |
| `--water-fps 30 --fish-px 6` | 11.6% | 8.2% | ~20% |

For comparison, the baseline with one full frame: `--px 8` 166%, `--px 4` 49%, `--px 2` 25%. The koi here are at least `--px 8` sharp. Ghostty seems to have a floor of roughly 11 to 15% whenever something changes at 60 fps. This variant cannot go below that.

## Run it

```sh
cargo build --release
./target/release/koi-layers                         # water-px 2, fish-px 8, water 60 fps
./target/release/koi-layers --water-fps 30          # water re-sent every other frame; koi still move at 60
./target/release/koi-layers --water-px 3 --fish-px 0 --angles 180 --phases 16   # finer water, native-pixel koi
./target/release/koi-layers --bench 1200 [--size 155x78] [same flags]           # game-side cost; drops food every 45 frames, so pessimistic
```

Controls: click drops food, `f` drops food at a random spot, `d` shows the overlay (fps, build ms, PTY bytes per frame, shm MB/s, pose renders per second), `q` or Ctrl-C quits.

## Measure phase: try these

```
/home/jeremy/Documents/workspace/cli-game/proto/perf/layers/target/release/koi-layers
/home/jeremy/Documents/workspace/cli-game/proto/perf/layers/target/release/koi-layers --water-fps 30
/home/jeremy/Documents/workspace/cli-game/proto/perf/layers/target/release/koi-layers --water-px 3 --water-fps 30
```

## What did not work, or costs something

- **Caching poses helps less than hoped.** Headings drift all the time, so hit rates are only 40 to 60%. About 150 poses per second get rendered and uploaded while the fish chase food, fewer when they cruise. Coarser buckets (96x8) cut that only to about 120/s and make the tail visibly step, so I kept 120x12. The tail wave is quantized to 12 frames per cycle (about 15 updates per second at cruise speed). Position and rotation still update every frame at 60 fps.
- **Native-pixel koi (`--fish-px 0`) are about twice the cost.** On this HiDPI window a koi is about 350 px long.
- **The first sprite version shaded every pixel analytically on each pose,** at about 1.5 ms per pose. The body-texture resample replaced it.
- **Visual changes from the reference:**
  - Ripples no longer refract the koi, because the koi sit above the water.
  - Spot patterns now bend with the body.
  - The water is softer than `--px 8`, because Ghostty scales it with linear filtering.
- Ghostty keeps up to 5 x 48 cached poses, about 50 to 70 MB of image data plus GPU textures.
- If the window is killed abruptly, the few shared-memory objects Ghostty had not read yet stay in `/dev/shm/koi-layers-*`.
