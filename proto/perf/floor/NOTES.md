# floor: what Ghostty costs no matter what we do

This crate isn't a pond variant. It's a set of test modes that isolate Ghostty's
share of the CPU: how much it costs to redraw at all, how much the image size adds,
and what the update rate does. Every mode except `pond` sends one kind of traffic
and nothing else. The game side is close to 0% in all of them.

## Run it

```sh
cargo build --release
B=./target/release/koi-floor
$B --mode text              # one text cell changes 60 times a second, no graphics at all
$B --mode tiny              # 16x16 image with new pixels each frame, 60 Hz, 2x1 cells
$B --mode static --px 2     # full-window pond image, rendered once, re-sent (a=T, shm) every frame
$B --mode place --px 2      # full-window image sent once, then only re-placed (a=p) each frame, X offset 0/1
$B --mode once --px 2       # full-window image sent once, then nothing
$B --mode blank             # sends nothing at all
$B --mode pond --px 4 --fps 30   # the real pond (same pond.rs) at a capped rate; sim still steps at 60 Hz
```

`--fps N` sets the send rate for every mode that sends frames (default 60).
`--px N` is the image pixels per cell width, the same as in proto/kitty. Keys work
as before: `d` toggles the overlay (mode, measured fps, image size) and `q` quits.
Click and `f` drop food, but only `pond` mode draws it. `once` and `blank` block
in `poll()` until input arrives.

## What I saw (rough, other agents were running Ghostty windows at the same time)

XWayland Ghostty 1.3.1 at 160x45, focused by a click the same way measure.sh does it.
The numbers are Ghostty CPU as a percent of one core, averaged over 8 s. Repeated runs
differed by up to 2x, so read them as ranges.

| mode | Ghostty % |
|---|---|
| blank, once (static picture on screen) | 0 to 0.5 |
| text 1 fps / 5 / 15 / 30 / 60 | 0.5 / 3 / 8 / 14 / 16 |
| tiny (16x16 image, 60 Hz) | 10 to 20 |
| static --px 2 (620x~340 re-uploaded, 60 Hz) | 10 to 24 |
| place --px 2 (no upload, only a=p) | 9 to 29 |
| static --px 8 (~2.5 MB re-uploaded, 60 Hz) | 28 to 37 |
| pond --px 4 at 30 / 45 / 60 fps | 19 / 14 / 16 (game 15 / 23 / 29) |
| custom-shader (pass-through), `once`, focused | 12 |

What this means:

- **Redrawing costs the same whatever changed.** One changed text cell at 60 Hz costs
  as much as re-uploading a 620x340 image. Ghostty repaints the whole surface on every
  change, so about 15% of a core at 60 Hz is the floor for any animation. Uploads only
  start to matter at large sizes: --px 8 roughly doubles it.
- **Cost grows with the update rate until about 30 fps, then flattens.** That's about 0.5%
  per update per second, so 5 fps costs about 3% and 15 fps about 8%. Above 30 fps
  Ghostty seems to merge updates and the curve bends. That also means pond at 30 fps
  saves Ghostty little. It mainly halves the game's own render cost.
- **A static screen is free.** Nothing sent means nothing drawn. Holding the rate at 60
  and making each frame cheaper won't get the total below about 15%. Single digits
  overall need fewer redraws: stop sending while nothing moves, or drop to about
  10 to 15 fps when the pond is calm.
- Window size barely mattered (80x22 cost about the same as 160x45), so the cost is
  per frame, not per pixel.

## Ghostty config options that affect render cost

From `ghostty +show-config --default --docs` (1.3.1, Linux):

- `custom-shader` together with `custom-shader-animation = true` (the default) runs a
  render loop while the window is focused, even when nothing changes. That cost 12%
  on an idle `once` window. Never ship a shader, or set `custom-shader-animation = false`.
- `window-vsync`: macOS only. It does nothing here.
- `background-opacity = 0.9`: no measurable difference in `text` (within noise).
  `background-blur` (not tested) works on Linux only under KWin, where the
  compositor does the blurring, not Ghostty.
- `cursor-style-blink`: a blinking cursor redraws about twice a second. The pond hides
  the cursor (`?25l`), so it doesn't apply.
- `image-storage-limit` (320 MB default): only a cap. It doesn't affect speed while we
  stay under it. Per-frame shm uploads replace image id 1, so nothing piles up.
- `alpha-blending`, `window-colorspace`, `font-thicken`: per-glyph shading settings,
  not tested. Given that one changed cell costs as much as a full image, they
  shouldn't move the floor.
- `gtk-single-instance = detect`: under native Wayland, `ghostty -e` from a shell can
  hand the window to an already-running Ghostty process, and then pidstat measures the
  wrong process. measure.sh uses `GDK_BACKEND=x11`, which avoids this. For a manual
  check, pass `--gtk-single-instance=false`.
- The user's `~/.config/ghostty/config` only sets keybinds and QoL options. Nothing in
  it touches rendering.

## Didn't work

- **Native Wayland (`GDK_BACKEND=wayland`) could not be measured from this session.**
  The window got created, but the child never started and Ghostty showed 0% CPU. It
  probably never got mapped or focused on COSMIC. grim can't capture COSMIC
  (no wlr-screencopy). Every number here is XWayland. The Wayland path may have a
  different floor, and someone at the desk should check it by hand.
- The measurements are noisy because other agents had Ghostty windows open at the same
  time. The Measure phase should re-run them one after another.

## For the Measure phase (release binary, one per line)

```
/home/jeremy/Documents/workspace/cli-game/proto/perf/floor/target/release/koi-floor --mode text
/home/jeremy/Documents/workspace/cli-game/proto/perf/floor/target/release/koi-floor --mode static --px 8
/home/jeremy/Documents/workspace/cli-game/proto/perf/floor/target/release/koi-floor --mode pond --px 4 --fps 30
```

1. `text` is the pure 60 Hz redraw floor, with no graphics.
2. `static --px 8` shows what uploading at the target look costs on top of that floor,
   with the game at about 0.
3. `pond --px 4 --fps 30` is the capped-rate comparison.

If there's room for more: `--mode text --fps 15` (the rate curve), `--mode once --px 2`
(a static picture costs about 0), `--mode tiny`, and `--mode pond --px 4 --fps 45`.
