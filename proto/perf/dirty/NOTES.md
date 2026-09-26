# koi-dirty: send only what changed

Copy of `proto/kitty` that stops re-sending the whole pond every frame.

## Does Ghostty support partial image updates?

No. In Ghostty 1.3.1 (`src/terminal/kitty/graphics_exec.zig`), the actions
`a=f`, `a=a` and `a=c` all return `ERROR: unimplemented action`. There is no
way to patch a sub-rectangle of an existing image.

What Ghostty does support, and this variant relies on:

- Re-transmitting an image id with `a=t` replaces the pixels and keeps its
  placements (`ImageStorage.addImage` swaps the data and leaves placements alone).
- The renderer re-uploads only images whose `transmit_time` changed
  (`renderer/image.zig`, `prepImage`). Unchanged images cost no copy and no upload.

So dirty rectangles are done with tiles: the pond is split into a grid of
separate Kitty images, each placed once over its own block of cells. Only
tiles whose pixels changed get re-sent.

## What changed

- **Tiles.** `--tile CxR` (cells, default 16x8). The first frame sends each tile
  with `a=T` at its cell position. After that a changed tile is sent with `a=t`
  only: no cursor moves, no re-placing. Frames where nothing changed write
  nothing to the PTY.
- **Quantized water.** `--quant N` (default 4) rounds the glint to steps of N
  levels. That gives a hard threshold: if every height in a tile is below
  `min(N/160, 1/12)`, the tile shades to exactly the unrippled pixels. Those
  tiles are marked calm. They are shaded once more when they settle, then
  never touched again.
- **Change detection.** A tile gets shaded only if koi or food are within 12 px
  of it this frame or last frame, or the water stepped and the tile is not calm
  and settled. After shading, the tile is compared with the bytes last sent
  and skipped if identical.
- **Water at 30 Hz.** `--water-hz 30` (default) runs two water steps every other
  frame, so ripples keep their speed and decay. Koi still move and redraw at
  60 Hz. Tiles that only have water in them change at 30 Hz.
- **Cheaper koi drawing.** Each koi row only visits the columns inside the
  koi's and its shadow's oriented boxes, and skips pixels outside the body
  before the `sin` calls. Pixels come out identical to before (checked over
  6000 frames with an assert that no skipped pixel would have been drawn).
- Calm tiles skip the refraction pass.
- Dropped the media probe, tmux wrapping and the file/direct/zlib media. It is
  shm only.
- shm objects are unlinked if Ghostty has not read them 120 frames later, and
  on exit. SIGHUP is ignored so that closing the window turns into a write
  error and the cleanup still runs.
- `--rgba` sends `f=32` so Ghostty skips its RGB to RGBA swizzle. It made no
  clear difference in my runs.

## Run

```
cargo build --release
./target/release/koi-dirty [--px 8] [--tile 16x8] [--quant 4] [--water-hz 30] [--rgba]
./target/release/koi-dirty --bench 600 --size 155x83 [same flags]
```

Controls are unchanged: click drops food, `f` drops food at random, `d` toggles
the overlay (fps, build time, tiles and KB sent per frame), `q` quits.

## Numbers so far (not final, other agents were running)

The measure.sh window turns out to be 155x83 cells, so the pond is 1240x1328 px
at `--px 8`. On `--bench 600 --size 155x83`, which drops food every 45 frames
(a busy pond):

| | build ms/frame | sent per frame |
|---|---|---|
| koi-kitty --px 8 | 18.8 | 4824 KB, 1 image |
| dirty 8x4 tiles | 8.6 | 1123 KB, 95 of 420 tiles |
| dirty 16x8 tiles | 9.0 | 1549 KB, 33 of 110 tiles |
| dirty 32x16 tiles | 10.2 | 2398 KB, 14 of 30 tiles |

In a live window, Ghostty's cost followed the number of tiles more than the
bytes. With 8x4 tiles Ghostty used around 50%, with 16x8 about 37%, and with
32x16 about 25 to 30%. The game used 55 to 70%. The machine was busy, so these
numbers are rough. My guess at why: every kitty change makes Ghostty rebuild
all placements, and it draws each placement as its own quad and texture.

## What limits this

The koi. Five koi at `--px 8` cover roughly 600 KB of pixels, and they move
every frame, so no amount of dirty tracking gets under that at 60 fps. Drawing
them costs about 3.5 ms of CPU per frame. Water is not the problem: the sim is
under 1 ms, and quantizing lets most of the pond go calm between food drops.
Wakes keep the tiles around each koi active.

Single digits total is not reachable this way. Ghostty's fixed cost to redraw
a 60 Hz window is already around 15% (see the `--px 2` baseline), and the koi
tiles add to that. Combine this with a variant that makes the koi cheaper
(sprites or the GPU) to get further.

## Didn't work

- Re-shading only the koi rectangles inside a tile on frames where the water
  didn't step. The koi rects plus margin, this frame and last, overlap enough
  that it was slower (9.8 ms against 8.6 ms).
- Replacing constant divides with multiplies in the koi shader: about 0.25 ms.
  Not worth the noise in the code, so I left it out.

## For the Measure phase

```
/home/jeremy/Documents/workspace/cli-game/proto/perf/dirty/target/release/koi-dirty --px 8 --tile 16x8
/home/jeremy/Documents/workspace/cli-game/proto/perf/dirty/target/release/koi-dirty --px 8 --tile 32x16
/home/jeremy/Documents/workspace/cli-game/proto/perf/dirty/target/release/koi-dirty --px 8 --tile 8x4
```

All three use `--quant 4 --water-hz 30` by default.
