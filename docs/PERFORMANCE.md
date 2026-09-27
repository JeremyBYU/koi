# Performance

This file has the CPU measurements behind koi's rendering design. They come from prototypes built before the game (they are in the git history), not from the finished game.

## How it was measured

Each prototype ran in a 160x45 Ghostty window under XWayland, with `TMUX` unset. CPU is in percent of one core, averaged over 10 s, for Ghostty and for the game process separately. Each prototype ran 3 times, and the tables show the median, with the range in brackets. The floor tests ran once. A screen recorder captured about 47 to 53 fps from the window, so about 140 to 160 frames per 3 s. When nearly every captured frame is distinct, the window changed at least as often as the recorder sampled. That can't show that all 60 frames arrived. A 30 fps sender shows about half.

Native Wayland was not measured. It may have a different floor.

## The floor: most of the cost is in the terminal

The floor tests send the least possible change at a fixed rate, to see what Ghostty costs on its own.

| Test | Ghostty % | Game % | Total % |
|---|---|---|---|
| One text cell, 5 fps | 3.5 | 0.0 | 3.5 |
| One text cell, 15 fps | 11.2 | 0.1 | 11 |
| One text cell, 30 fps | 21.0 | 0.2 | 21 |
| One text cell, 60 fps | 22.8 [22.0-23.3] | 0.2 | 23 |
| A 16x16 image, 60 fps | 22.9 | 0.1 | 23 |
| An image re-placed, 2 px per cell, 60 fps | 23.1 | 0.1 | 23 |
| A full 8 px per cell image, re-sent unchanged, 60 fps | 56.4 [45.7-58.4] | 9.5 | 66 |
| The same at 15 fps | 17.7 | 2.8 | 20.5 |
| One image sent once, then nothing | 0.8 | 0.0 | 0.8 |

What this shows:

- Ghostty's cost is set by how often the screen changes, not by what changed. One text cell, a 16x16 image or a re-placed image all cost about 23% of a core at 60 Hz. That is the floor for any 60 fps animation in this window.
- The rate curve flattens above 30 fps. Below 30 it is roughly 0.7% per update per second.
- Uploading a full 8 px per cell frame roughly doubles Ghostty's cost, from 23% to about 50 to 56% at 60 Hz. At 6 px per cell the upload adds about 12 points instead of about 25.
- A picture that does not change is free (0.8%).

So in this setup, any 60 fps animation costs Ghostty about 23%. Getting to single digits means changing the screen less often.

## The prototypes

"px" is image pixels per cell. The base prototype renders the whole pond on the CPU into one image per frame and sends it through shared memory.

| Prototype | Ghostty % | Game % | Total % | Captured/distinct | Look |
|---|---|---|---|---|---|
| Base, px 2 | 23.9 [23.5-24.5] | 7.4 | 31 | 154/151 | Soft koi with stair-stepped patch edges, faint ripples. |
| Base, px 4 | 28.6 [28.1-29.6] | 24.5 | 53 | 133/132 | Between the two. |
| Base, px 8 | 49.6 [48.6-55.2] | 99.4 | 149 | 138/124 | The reference look: crisp koi, clear ripples. Drops frames with the game at 100%. |
| Layers | 27.1 [26.2-28.3] | 13.2 | 40 | 144/144 | Koi as crisp as px 8 or crisper. Water slightly softer. |
| Layers, water at 30 fps | 26.8 [26.2-28.2] | 8.9 | 36 | 141/121 | Same. Motion stays smooth, since the koi move at 60. |
| Layers, water px 3 at 30 fps | 27.3 [27.1-27.6] | 14.0 | 41 | 147/145 | Same, water a bit sharper. |
| Dirty tiles 16x8, px 8 | 54.5 [54.1-58.4] | 55.7 | 110 | 153/147 | Stale ripple patches stay in calm water. |
| Dirty tiles 32x16, px 8 | 43.9 [43.3-46.5] | 63.2 | 107 | 154/143 | Same, clearly visible. |
| Dirty tiles 8x4, px 8 | 69.1 [68.7-70.3] | 51.8 | 121 | 157/149 | Same, less obvious. |
| Faster CPU, px 8 | 48.5 [47.9-58.6] | 29.8 | 78 | 150/148 | Matches base px 8. |
| Faster CPU, px 8, water px 4 | 48.4 [43.7-57.8] | 24.5 | 73 | 148/146 | Ripples a touch softer. |
| Faster CPU, px 8, fresh shm per frame | 44.9 [43.9-46.7] | 54.5 | 99 | 144/142 | Matches base px 8. |
| GPU, px 8 | 44.4 [41.7-45.6] | 3.5 | 48 | 148/136 | Matches base px 8. |
| GPU, px 8, RGBA | 52.6 [42.2-57.3] | 3.9 | 57 | 146/146 | Same as GPU px 8. |
| GPU, px 6 | 34.6 [32.2-41.5] | 2.5 | 37 | 141/141 | Very close to px 8 at this window size. |
| Whole pond, px 4, 30 fps | 20.0 [19.1-20.3] | 13.1 | 33 | 144/83 | px 4 look at 30 fps. |
| Whole pond, px 8, 30 fps | 32.3 | 50.3 | 83 | 147/83 | px 8 look at 30 fps. |
| Whole pond, px 8, 15 fps | 19.1 | 25.5 | 45 | 148/43 | Visibly steppy. |

The variants:

- Layers sends one low-resolution water image that Ghostty scales, plus one small image per koi, moved by placement.
- Dirty tiles re-sends only the tiles of the image that changed.
- Faster CPU keeps a ring of persistent shared-memory files instead of a fresh one per frame, which removes page faults.
- GPU renders the full image with a wgpu compute shader.

What worked:

- GPU rendering took the game from 99% to 3.5% at px 8 with the same look. What is left is Ghostty taking in the large image.
- Layers kept Ghostty at 27%, 4 points above the floor, with koi as sharp as px 8. The game added 9 to 14%. Layers with the water at 30 fps was the best measured 60 fps total, at 36%.
- The persistent file ring cut the faster CPU prototype's game cost from 55 to 30%.

What did not:

- Dirty tiles saved nothing. Ghostty's cost rises with the tile count (44 to 69% against 50%), the game still used 52 to 63%, and the calm-tile threshold left stale ripple blotches.
- The file ring did not lower Ghostty's cost (48% against 45% with fresh shm). It only helps the game.
- Sending RGBA instead of RGB did not lower Ghostty's cost either (53% against 44%, and noisy).
- Halving the water rate in layers only saved game CPU (13 to 9%). Ghostty still redraws at 60 because the koi move.
- 30 fps overall halves the game's CPU but saves Ghostty little (32% at px 8 against about 50%).

## What koi does

koi combines the three things that worked:

- Layers. One water image at `render.water_px` (default 2) pixels per cell, sent at most `render.water_fps` (default 30) times a second, and one image per koi at native screen resolution.
- GPU rendering. The water and the koi poses render with wgpu. The CPU backend is a fallback.
- An adaptive frame rate. 60 fps while the window is focused or something is happening (food in the water, recent input, a darting koi), 8 fps when unfocused and calm, and nothing sent when nothing on screen changed.

From the numbers above, a focused pond at 60 fps should cost about 27 to 30% of a core in total: Ghostty about 27, the game about 3. Ghostty's 23% floor at 60 fps leaves little room below that. An unfocused calm pond should cost under 10%: the floor curve gives about 7 to 9% at 10 fps and about 4 to 5% at 5 fps.

These are estimates from the prototypes and the floor tests. The finished game was not measured the same way.
