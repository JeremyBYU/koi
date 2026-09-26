# Measure phase results

All numbers come from `perf/measure.sh`. Runs were sequential, the window was 160x45 under XWayland, and CPU is in percent of one core averaged over 10 s. Each pond command ran 3 times and the table shows the median, with the range in brackets. Floor extras ran once.

How it was run:
- `TMUX` was unset for every run (`env -u TMUX -u TMUX_PANE`), so no variant wrapped its output in tmux passthrough. That matches the real setup: its own Ghostty window, no tmux.
- A stale `koi-test` Ghostty window (halfblock, about 42% CPU) was left running from an earlier phase. I killed it and threw away the rounds measured while it ran. Those rounds had Ghostty numbers swinging 16 to 46% for the same command. Once it was gone, repeats agreed within a few points, except `gpu --fmt 32` and `fastcpu` round 1.
- The recorder captures about 47 to 53 fps from this window, so "captured" is about 140 to 160 per 3 s. "Distinct" close to captured means no dropped frames at that rate. A 30 fps sender shows about half.
- Commands below are relative to `proto/`. The absolute paths are `/home/jeremy/Documents/workspace/cli-game/proto/...`.

## Table

| label | command | Ghostty % | game % | total % | captured/distinct (run 1) | look |
|---|---|---|---|---|---|---|
| base-px2 | `kitty/target/release/koi-kitty --px 2` | 23.9 [23.5-24.5] | 7.4 | 31 | 154/151 | Koi soft with stair-stepped patch edges, ripples faint |
| base-px4 | `koi-kitty --px 4` | 28.6 [28.1-29.6] | 24.5 | 53 | 133/132 | Between the two |
| base-px8 | `koi-kitty --px 8` | 49.6 [48.6-55.2] | 99.4 | 149 | 138/124 | Reference look: crisp koi, clear ripples. Drops frames (game pinned at 100%) |
| layers-default | `perf/layers/target/release/koi-layers` | 27.1 [26.2-28.3] | 13.2 | 40 | 144/144 | Koi as crisp as px8 or crisper. Water and ripples slightly softer but clearly visible. No seams or misplaced sprites seen |
| layers-w30 | `koi-layers --water-fps 30` | 26.8 [26.2-28.2] | 8.9 | 36 | 141/121 | Same as default. Motion still smooth, since the koi move at 60 |
| layers-wpx3-w30 | `koi-layers --water-px 3 --water-fps 30` | 27.3 [27.1-27.6] | 14.0 | 41 | 147/145 | Same, water a bit sharper |
| dirty-16x8 | `perf/dirty/target/release/koi-dirty --px 8 --tile 16x8` | 54.5 [54.1-58.4] | 55.7 | 110 | 153/147 | Koi crisp. **Artifact:** stale blotchy "fossil" ripple patches stay in calm water |
| dirty-32x16 | `koi-dirty --px 8 --tile 32x16` | 43.9 [43.3-46.5] | 63.2 | 107 | 154/143 | Same artifact, clearly visible around old ripples |
| dirty-8x4 | `koi-dirty --px 8 --tile 8x4` | 69.1 [68.7-70.3] | 51.8 | 121 | 157/149 | Same artifact, less obvious |
| fastcpu-px8 | `perf/fastcpu/target/release/koi-fastcpu --px 8` | 48.5 [47.9-58.6] | 29.8 | 78 | 150/148 | Matches base-px8 |
| fastcpu-px8-w4 | `koi-fastcpu --px 8 --water 4` | 48.4 [43.7-57.8] | 24.5 | 73 | 148/146 | Matches base-px8, ripples a touch softer |
| fastcpu-px8-shm | `koi-fastcpu --px 8 --medium shm` | 44.9 [43.9-46.7] | 54.5 | 99 | 144/142 | Matches base-px8 |
| gpu-px8 | `perf/gpu/target/release/koi-gpu --px 8` | 44.4 [41.7-45.6] | 3.5 | 48 | 148/136 | Matches base-px8: crisp koi, clear ripples |
| gpu-px8-f32 | `koi-gpu --px 8 --fmt 32` | 52.6 [42.2-57.3] | 3.9 | 57 | 146/146 | Same as gpu-px8 |
| gpu-px6 | `koi-gpu --px 6` | 34.6 [32.2-41.5] | 2.5 | 37 | 141/141 | Very close to px8 at this window size |
| floor-text | `perf/floor/target/release/koi-floor --mode text` | 22.8 [22.0-23.3] | 0.2 | 23 | 160/155 | Test mode |
| floor-static8 | `koi-floor --mode static --px 8` | 56.4 [45.7-58.4] | 9.5 | 66 | 154/1 | Test mode (same image re-sent) |
| floor-pond4-30 | `koi-floor --mode pond --px 4 --fps 30` | 20.0 [19.1-20.3] | 13.1 | 33 | 144/83 | px4 look at 30 fps |
| floor-text5 | `koi-floor --mode text --fps 5` | 3.5 | 0.0 | 3.5 | 165/15 | Test mode |
| floor-text15 | `koi-floor --mode text --fps 15` | 11.2 | 0.1 | 11 | 163/42 | Test mode |
| floor-text30 | `koi-floor --mode text --fps 30` | 21.0 | 0.2 | 21 | 160/82 | Test mode |
| floor-tiny | `koi-floor --mode tiny` | 22.9 | 0.1 | 23 | 160/81 | Test mode |
| floor-place2 | `koi-floor --mode place --px 2` | 23.1 | 0.1 | 23 | 158/135 | Test mode |
| floor-once2 | `koi-floor --mode once --px 2` | 0.8 | 0.0 | 0.8 | 164/1 | Test mode |
| floor-static8-15 | `koi-floor --mode static --px 8 --fps 15` | 17.7 | 2.8 | 20.5 | 161/1 | Test mode |
| floor-pond8-15 | `koi-floor --mode pond --px 8 --fps 15` | 19.1 | 25.5 | 45 | 148/43 | px8 look at 15 fps, visibly steppy |
| floor-pond8-30 | `koi-floor --mode pond --px 8 --fps 30` | 32.3 | 50.3 | 83 | 147/83 | px8 look at 30 fps |

## What the floor tells us

- **Ghostty's cost is set by how often the screen changes, not by what changed.** One text cell, a 16x16 image or a re-placed image all cost about 23% at 60 Hz. That is the floor for any 60 fps animation in this window.
- **The rate curve flattens above 30 fps.** Text costs 3.5% at 5 fps, 11% at 15, 21% at 30 and 23% at 60. Below 30 it is roughly 0.7% per update per second.
- **Uploading a full --px 8 frame roughly doubles Ghostty's cost** (23% becomes about 50 to 56% at 60 Hz). At --px 6 the upload adds about 12 points instead of about 25.
- **A picture that doesn't change is free** (0.8%).

So no variant can get below about 23% total at 60 fps. The only way to single digits is to change the screen less often: about 5 to 10 updates per second, or none.

## What worked

- **GPU rendering (gpu) wins on game CPU.** At --px 8 the game went from 99% to 3.5% with the same look, and the frame rate holds. The cost left over is Ghostty ingesting the large image.
- **Layers wins on Ghostty CPU.** A low-res water image plus a small image per koi keeps Ghostty at 27%, only 4 points above the redraw floor, with koi as sharp as px8. The game adds 9 to 14%. `layers-w30` is the best 60 fps total measured: 36%.
- **gpu --px 6** gets 37% total with a look very close to px8.
- **The fastcpu ring (persistent `t=f` files) cut game CPU from 55 to 30%**, by removing page faults.

## What did not work

- **Dirty tiles.** Ghostty's cost goes up with the tile count, so it saved nothing on the Ghostty side (44 to 69% against 50%), the game still used 52 to 63%, and the calm-tile threshold leaves visible stale ripple blotches. Drop it.
- **The ring medium did not lower Ghostty's cost** (48% ring against 45% fresh shm). It only helps the game.
- **`--fmt 32` (RGBA)** did not lower Ghostty's cost either (53% against 44%, and noisy). Skipping the RGB-to-RGBA conversion is not where Ghostty spends its time.
- **Halving the water rate** in layers only saved game CPU (13 to 9%). Ghostty still redraws at 60 because the koi move.
- **30 fps overall** saves the game about half its CPU but saves Ghostty little (pond8 at 30: 32% against about 50%).

## Recommendation

Build the real game on **layers + GPU + an adaptive rate**:

1. **Layers:** one low-res water/shadow/ripple image (2 px per cell, water at 30 Hz) that Ghostty scales, plus one image per koi at px8 or better, moved by placement. This keeps each Ghostty frame close to the redraw floor.
2. **GPU:** render the water and the koi pose images with the wgpu compute path from `gpu`, so the game costs about 3% instead of 9 to 14%. Keep layers' pose cache so most frames upload nothing new.
3. **Adaptive rate:** this is the only lever that reaches single digits.
   - Send at 60 fps while the window is focused or something is happening: food in the water, recent clicks, active ripples.
   - When the window is unfocused (Kitty focus reporting, `CSI ?1004h`) and the pond is calm, drop to 5 to 10 fps.
   - Send nothing when nothing on screen changed.

Expected total CPU, from the floor curve plus the measured parts:
- Focused at 60 fps: about 27 to 30% (Ghostty about 27, game about 3). No 60 fps design can get meaningfully under this, because of Ghostty's 23% floor.
- Unfocused and calm at 10 fps: about 7 to 9%. At 5 fps: about 4 to 5%. These are estimates from the text-mode floor, not measured with the combined build.

The trade-off for you to decide: the calm all-day case gets single digits only if slow koi motion is acceptable while the window is unfocused. A smooth 60 fps pond costs about 30% of a core in Ghostty regardless of how the game renders.

## Other findings

- **Shared memory leaks on kill.** measure.sh kills Ghostty, and frames that were written but not yet read stay in `/dev/shm`. After these runs there were 225 `koi*` objects (174 MB): 140 from dirty, 34 from layers, 30 from kitty and 21 from floor. fastcpu and gpu left none, because they clean up on SIGHUP/SIGTERM. The real game needs that cleanup too. The Safety Net hook blocked me from deleting them, so to clear them run: `find /dev/shm -maxdepth 1 -name 'koi*' -delete`
- **Clicks.** measure.sh uses `click 1`, which the fastcpu builder found sometimes misses. Pellets were not checked per run. The ripples in every screenshot suggest drops landed in most runs.
- **Native Wayland was not measured.** Every number here is XWayland. A native Wayland window may have a different floor.

Screenshots and recordings are in `proto/perf/results/<label>.png|.mkv`. Run 1 has no suffix, later runs end in `-r2` and `-r3`.
