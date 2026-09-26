# Prior art: smooth, pretty and cheap

How other people make animation like ours smooth, good-looking and light on CPU. Terminal programs come first, because we run inside Ghostty and draw with the Kitty graphics protocol. General game techniques follow as supporting material. The last section is a ranked list of changes for this game.

I read source code where I could. Claims I could not check are marked **unverified**, and my own reasoning is marked **inference**. Ghostty facts refer to 1.3.1 (what is installed here) unless a line says "tip".

## Where we are now

The code has moved past parts of `docs/ARCHITECTURE.md`, so this is the baseline I compared against:

- The sim runs at a fixed 60 Hz. Frames already blend the last two sim states (`Pose::lerp` in `koi.rs`, `blend` in `main.rs`).
- Koi are posed fresh every frame on the GPU. There is no pose cache any more, so heading and tail phase are no longer quantized. The `render.angles` and `render.phases` rows in ARCHITECTURE.md are stale.
- Each koi is its own image at `fish_px = 8` sprite pixels per cell width. A cell is about 10 to 11 screen pixels, so one sprite pixel is about 1.3 screen pixels, and Ghostty scales the image up.
- The placement is rounded to a whole screen pixel (cell plus `X`/`Y`). The sprite is sampled at integer sprite-pixel offsets from the fish centre (`origin: vec2<i32>` in `koi.wgsl`). The fish shape therefore sits rigidly on its own 1.3 px lattice, and the whole image moves in 1 screen-pixel steps with no fractional shift inside it.
- Heading already runs through a second-order system: turn rate lags toward twice the heading error, clamped to a cap (`koi.rs` around line 493).
- Water is encoded to 8-bit sRGB with plain rounding and no dither (`water.wgsl` line 203).

I did not reproduce the reported 1.3 px steps against the current build. With screen-pixel placement the outline should step by 1 screen pixel. If 1.3 px steps are still visible, the cause is either an older build or something in how Ghostty draws a scaled image at an offset. Recommendation 1 below fixes both cases, and a recording at 0.1 px per frame would tell them apart.

## 1. Terminal games and terminal graphics

### Classic ASCII animators

None of these are smooth in our sense. They are useful as a record of what a PTY tolerates.

- **asciiquarium** uses `halfdelay(1)`, so `getch()` blocks up to 100 ms and the loop runs near 10 fps. Fish have fractional speeds (`rand(2)+.25` columns per tick) that Term::Animation rounds to whole cells. Motion is cell-stepped, which is fine for its look and would be wrong for ours. It never idles. ([source](https://github.com/cmatsuoka/asciiquarium/blob/master/asciiquarium))
- **cbonsai** draws with ncurses panels and `nanosleep`. In static mode it blocks on `wgetch` and costs nothing. ([source](https://gitlab.com/jallbrit/cbonsai/-/blob/master/cbonsai.c))
- **pipes.sh** paces with `read -t` at 75 fps, so the input read doubles as the sleep. One `printf` per segment, no sync, no diffing. ([source](https://github.com/pipeseroni/pipes.sh/blob/master/pipes.sh))
- **cmatrix** sleeps a fixed `napms` after each frame, so the frame period drifts with the work. ncurses `refresh()` does the cell diff. ([source](https://github.com/abishekvashok/cmatrix/blob/master/cmatrix.c))

The common pattern is "sleep N after the work, let curses diff". Nobody paces against a clock and nobody throttles when unfocused.

### Doom ports and emulators using Kitty images

These are the closest prior art to us: full-rate raster images through a PTY.

- **terminal-doom** (Zig, on libvaxis) sends a 640x400 RGB image every frame. Its loop is "set mode 2026, clear, send image, reset 2026", and the README says it works best in Ghostty and kitty. It also notes that skipping the 4 KB chunking is faster in some terminals but breaks kitty. ([README](https://github.com/cryptocode/terminal-doom/blob/main/README.md), [main.zig](https://github.com/cryptocode/terminal-doom/blob/main/src/main.zig))
- **libvaxis** wraps each render in `?2026h`/`?2026l`, deletes placements with `a=d`, diffs cells, then re-places images with `a=p,i=..,X=,Y=`. ([Vaxis.zig](https://github.com/rockorager/libvaxis/blob/main/src/Vaxis.zig))
- **kitty-doom** sends the first frame with `a=T`. After that it edits the root frame in place with `a=f,r=1` and shows it with `a=a,c=1`, so the image and placement survive and only pixels change. The README claims SIMD frame-diffing to skip identical frames at 35 fps (**unverified**). ([render.c](https://github.com/jserv/kitty-doom/blob/main/src/render.c))
- **chipz-kitty** runs 8-bit emulators on libvaxis, tested in Ghostty and kitty. ([repo](https://github.com/floooh/chipz-kitty))
- **doom-ascii** builds one buffer of truecolor SGR and characters and writes it with a single `fputs`. No 2026, no diff. ([source](https://github.com/wojciech-graj/doom-ascii/blob/master/src/doomgeneric_ascii.c))

None of them move sprites sub-pixel. They redraw a full frame, so the question never comes up.

### Terminal video players and viewers

- **mpv `--vo=kitty`** with `--vo-kitty-use-shm=yes` sends `a=T,t=s,f=24,...,C=1,q=2` with no `i=`, so every frame is a new anonymous image. It builds the whole command in one buffer and writes it in a `write()` loop, with a comment that Linux keeps large writes atomic in practice. There is no mode 2026, and pacing comes from mpv core, not the VO. It reuses one fixed shm name per process (`/mpv-kitty-%p`), which is a race if the terminal has not read frame N before N+1 overwrites it (**inference**). Ghostty's own image-path optimisation was profiled with exactly this command ([PR #13987](https://github.com/ghostty-org/ghostty/pull/13987)). ([vo_kitty.c](https://github.com/mpv-player/mpv/blob/master/video/out/vo_kitty.c))
- **mpv `--vo=tct`** draws half-blocks, wraps each frame in 2026 and flushes once. Its 256-color fallback picks the nearest entry in the 6x6x6 cube or grey ramp, with no dithering. ([vo_tct.c](https://github.com/mpv-player/mpv/blob/master/video/out/vo_tct.c))
- **timg** writes on its own thread through a bounded queue. Each frame's deadline is absolute (`animation_start + end_of_frame`), so drift cannot build up, and frames more than 250 ms late are skipped. For kitty animations it alternates between two image IDs because "sending a bunch of images with different IDs overwhelms some terminals", and it always sets `i=` because some terminals treat a missing id as 0. Block modes diff cells against the previous frame. ([buffered-write-sequencer.cc](https://github.com/hzeller/timg/blob/main/src/buffered-write-sequencer.cc), [kitty-canvas.cc](https://github.com/hzeller/timg/blob/main/src/kitty-canvas.cc))
- **chafa** offers `none/ordered/diffusion/noise` dithering and says it has "no effect with 24-bit color". That is about quantizing to a small palette. It says nothing about 8-bit banding in smooth gradients, which still exists in truecolor (see section 3). Symbol classes include half, quad, sextant, octant and braille. No 2026. ([chicle-options.c](https://github.com/hpjansson/chafa/blob/master/tools/chafa/chicle-options.c))
- **viu** sends kitty images through a temp file or direct data and falls back to half-blocks. GIFs play with a sleep per frame. ([kitty.rs](https://github.com/atanunq/viuer/blob/master/src/printer/kitty.rs))
- **kitty's icat** sends over shm and hands animations to the terminal as `a=f` frames with a gap, so kitty itself times playback. ([transmit.go](https://github.com/kovidgoyal/kitty/blob/master/kittens/icat/transmit.go))

### notcurses

- Every frame is one buffer and one blocking write, wrapped speculatively in `?2026h`. Support is probed with `?2026$p`. ([render.c](https://github.com/dankamongmen/notcurses/blob/master/src/lib/render.c), [termdesc.c](https://github.com/dankamongmen/notcurses/blob/master/src/lib/termdesc.c))
- Bitmaps ("sprixels") are placed with `a=p,i=ID,p=1,X=,Y=`. The header documents `pxoffx/pxoffy` as pixel offsets inside the cell that "can be used for smooth bitmap movement", and they must be smaller than the cell. ([kitty.c](https://github.com/dankamongmen/notcurses/blob/master/src/lib/kitty.c), [notcurses.h](https://github.com/dankamongmen/notcurses/blob/master/include/notcurses/notcurses.h))
- Blitters cover half-blocks, quadrants, sextants, octants, braille and real pixels. Without truecolor it quantizes to 256 colors. I found no dithering code.
- Nick Black's [Theory and Practice of Sprixels](https://nick-black.com/dankwiki/index.php?title=Theory_and_Practice_of_Sprixels) is architecture, not measurements, and he says in [discussion #2157](https://github.com/dankamongmen/notcurses/discussions/2157) that he has not run systematic cross-terminal benchmarks. Treat performance claims about notcurses graphics as folklore.

### Rust terminal libraries

- **ratatui** diffs the previous and next cell buffers. It does not emit 2026 itself; crossterm's `BeginSynchronizedUpdate` does, and callers wrap draws. ([repo](https://github.com/ratatui/ratatui))
- **ratatui-image** sends `f=32,t=s` over shm and shows the image through Unicode placeholder cells. That fits ratatui's diffing, but placeholders are whole cells and take no pixel offsets. Fine for a static lily pad, wrong for a swimming koi. ([kitty.rs](https://github.com/benjajaja/ratatui-image/blob/main/src/protocol/kitty.rs))
- **bevy_ratatui_camera** renders a GPU camera into half-blocks or braille and paces at a fixed 1/60 s. ([README](https://github.com/cxreiff/bevy_ratatui_camera))

### Kitty protocol and Ghostty facts that matter to us

- **Offsets are integers and smaller than a cell.** The spec says "the offsets must be smaller than the size of the cell". Ghostty clamps them to `cell_width - 1` (`graphics_storage.zig`). Placement alone can only move an image in whole screen pixels. ([spec](https://sw.kovidgoyal.net/kitty/graphics-protocol/))
- **Scaling is bilinear, with no option.** Ghostty's image textures use linear min and mag filters with no mipmaps (`src/renderer/OpenGL.zig`), and kitty uploads with `GL_LINEAR` too. Our 1.3x koi upscale and 5x water upscale are both bilinear. That suits the water. On the koi it adds a second resample on top of ours.
- **Placement replacement.** Sending the same image id and placement id again replaces the placement "without flicker". On 1.3.1, `a=T` with a fixed `i=` and **no** `p=` piles up a new placement every frame until the machine lags ([discussion #13719](https://github.com/ghostty-org/ghostty/discussions/13719), fixed on tip in [PR #13723](https://github.com/ghostty-org/ghostty/pull/13723)). We always send `p=1`, so we are not affected. Keep it that way.
- **Animation frames (`a=f`, `a=a`)** reached Ghostty tip in [PR #13943](https://github.com/ghostty-org/ghostty/pull/13943) and are not in 1.3.1. The kitty-doom and notcurses trick of editing pixels in place is not portable for us yet.
- **Mode 2026.** Ghostty skips rendering while it is set and force-resets it after 1 s (`termio/Thread.zig`), so a stalled frame cannot freeze the window.
- **Idle.** Ghostty does not draw when the window is occluded and lowers its thread priority when unfocused, but it still parses every byte we send (`renderer/Thread.zig`). Our own throttle is the only thing that saves CPU there.
- **shm.** Ghostty opens the object read-only and unlinks it after reading (`graphics_image.zig`). Sending `f=32` skips its RGB-to-RGBA step (**inference** from the code; our perf run saw no measurable Ghostty win from `--fmt 32`).

### What the terminal prior art teaches

1. **One buffer, one write, inside 2026.** notcurses, vo_tct, libvaxis and terminal-doom all converge on this. We already do it.
2. **Pace against absolute deadlines and drop late frames**, as timg does. "Sleep after the work" (cmatrix, xscreensaver) drifts.
3. **Nobody moves sprites sub-pixel.** The state of the art is integer `X`/`Y` offsets (notcurses, libvaxis). Anything finer means resampling the sprite ourselves, which we are unusually well placed to do because we already pose every koi on the GPU each frame.
4. **Nobody throttles on focus.** Mode 1004 is parsed by libvaxis and otherwise ignored. Our adaptive rate is ahead of the prior art.
5. **Expect bilinear scaling.** Any terminal-scaled image will be soft. Choose image sizes knowing that.

## 2. Koi and fish games

Technical writeups are thin. Most of what exists is marketing or reviews.

- **Koi Pond (iOS, The Blimp Pilots, 2008)** grew from a water simulation Brandon Bogle had written for an earlier game. Reviews describe fish that cast shadows and change course "as real fish would", and v2.0 added refracting koi. No technical detail on fish animation is public that I could find. ([Macworld Q&A](https://www.macworld.com/article/192532/koipond.html), [Macworld review](https://www.macworld.com/article/193939/koipond-2.html))
- **Zen Koi (LandShark Games)**. Only design material is public (breeding, hundreds of patterns). Whether it uses Spine or similar skeletal animation is **unverified**. ([press kit](https://zenkoipro.com/press-kit/))
- **KOI (Dotoyou, 2015/2016)**. Reviews praise the tail motion as "hypnotic and calming". No technical source. ([PlayStation Blog](https://blog.playstation.com/2016/03/03/introducing-koi-a-new-fish-in-the-ps4-pond/))
- **Prana and Flash-era koi toys.** Nothing technical found.
- **SereneScreen Marine Aquarium.** Jim Sachs hand-built the art in C++ in 2000 and contrasted it with the Windows 95 aquarium's "bitmaps of fish being dragged across the screen". Version 3 moved to full 3D. Whether v1 used pre-rendered sprite frames is **unverified**. ([PC Gamer](https://www.pcgamer.com/gaming-industry/a-former-air-force-pilot-was-so-disappointed-with-windows-95s-aquarium-screensaver-that-he-established-a-25-year-dynasty-of-digital-fish-that-continues-to-this-day/))
- **ABZU (Matt Nava, GDC 2017)** animates thousands of fish with vertex shaders, no bones. Godot's docs reproduce the approach: a side-to-side translation, a yaw, a travelling sine wave down the body and a mask so the head barely moves while the tail swings. This is the same model as our shader's `(0.1 + 0.9 t²) sin(phase - 2πt)`, applied to vertices instead of texture lookups. ([GDC Vault](https://www.gdcvault.com/play/1024409/Creating-the-Art-of-ABZU), [Godot docs](https://docs.godotengine.org/en/stable/tutorials/performance/vertex_animation/animating_thousands_of_fish.html))
- **Shader koi.** The public examples (an Unreal mosaic pond, several ArtStation fish shaders) all bend a mesh or sprite with a sine along the body axis. ([80.lv](https://80.lv/articles/stunning-mosaic-like-procedural-animated-pond-shader-with-swimming-koi))
- **Argonaut's procedural fish** is a chain of points with distance and angle constraints. Steer the head and the body follows. We already use this. ([repo](https://github.com/argonautcode/animal-proc-anim))

My read is that everyone serious converges on the same two pieces we have: a follow-the-leader spine for turns, plus a travelling sine with a head-to-tail mask for the swim. Nobody publishes anything cleverer, and I found no evidence that a koi app does something we are missing on the body side.

## 3. General 2D smoothness

- **Fix Your Timestep** (Glenn Fiedler). Fixed sim step plus interpolation by the leftover accumulator fraction, at the cost of one step of latency. We already do this. ([Gaffer on Games](https://gafferongames.com/post/fix_your_timestep/))
- **Why snapped sprites judder at slow speed.** A sprite moving v grid units per frame and snapped to the grid jumps once every 1/v frames. At 0.3 units per frame that is a step every third frame, a visible beat. Interpolation cannot help because the snap happens after it. For us the grid is one screen pixel for placement and one sprite pixel for the shape.
- **Snap, then shift back by the error.** Pixel-art 3D renderers snap the camera to the texel grid so pixels don't crawl, then shift the upscaled output by the snap error in screen space. David Holland credits t3ssel8r and aarthificial. ([davidhol.land](https://www.davidhol.land/articles/3d-pixel-art-rendering/), [yal.cc](https://yal.cc/gamemaker-smooth-pixel-perfect-camera/)) For us the placement is the snap and a fractional sampling offset inside the sprite is the shift back.
- **Filtering for sub-texel positions.** Several shaders sample nearest inside a texel and blend over one screen pixel at texel edges, which keeps crisp pixels while letting them slide. ([survey](https://jorenjoestar.github.io/post/pixel_art_filtering/), [CSantos](https://csantosbh.wordpress.com/2014/01/25/manual-texture-filtering-for-pixelated-games-in-webgl/), [Themaister's derivation](https://themaister.net/blog/2018/08/25/pseudo-bandlimited-pixel-art-filtering-in-3d-a-mathematical-derivation/)) Our koi texture is smooth rather than pixel art, so plain bilinear with a fractional offset is enough.
- **Springs.** Daniel Holden's exact damper `lerp(x, g, 1 - 2^(-dt/halflife))` is frame-rate independent, and his critically damped spring is parameterized by half-life, with a predictable lag of `halflife / ln 2`. A "double spring" gives an S-curve start and stop. Ryan Juckett derives the same thing with precomputed coefficients, which fits a fixed step. ([Spring-It-On](https://theorangeduck.com/page/spring-roll-call), [Juckett](https://www.ryanjuckett.com/damped-springs/))
- **Frame pacing.** Alen Ladavac (GDC 2018) shows that measured CPU frame times jitter even when presentation is steady, and feeding that jitter into the sim causes a "heartbeat" stutter. The fix is to time against presentation and change rate with hysteresis. ([slides](https://media.gdcvault.com/gdc2018/presentations/Ladavac_Alen_ElusiveFrameTiming.pdf), [article](https://medium.com/@alen.ladavac/the-elusive-frame-timing-168f899aec92)) A terminal gives no present timestamps, so the best we can do is interpolate to the frame's scheduled deadline rather than the moment the loop happened to wake.
- **Temporal aliasing.** A periodic motion of f Hz sampled at R fps aliases above R/2. Our tail beat is capped at 2 Hz, safely under the 4 Hz limit at 8 fps. The travelling wave's spatial speed is what matters for strobing, and at 8 fps it will look steppy but not backwards (**inference**).

## 4. Color

- **Blend in linear light.** sRGB-space mixing darkens and shifts hue. ([Ottosson, "How software gets color wrong"](https://bottosson.github.io/posts/colorwrong/)) Our water already works in linear light.
- **OKLab and OKLCH** give perceptually even lightness steps. Use them to build palette ramps and to interpolate between palette colors, not for physical light mixing. ([Ottosson, "A perceptual color space"](https://bottosson.github.io/posts/oklab/))
- **Hue-shifted ramps.** Pixel artists rotate hue a few degrees per lightness step, warmer toward highlights. Ramps that change only value look dead. ([Slynyrd](https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes))
- **Banding and dithering.** Mikkel Gjøl's Playdead talk is the best primary source. Dither just before quantizing, in the space you quantize in (after the sRGB encode). Use triangular-PDF noise of about ±1 LSB (`r1 + r2 - 1`) rather than uniform noise, dither each channel separately, and blend to uniform noise near 0 and 1. Blue noise is less visible than white. ([Banding in Games](https://loopit.dk/banding_in_games.pdf), [blue noise textures](https://momentsingraphics.de/BlueNoise.html))
- **Stable dither under motion.** Lucas Pope pinned Obra Dinn's dither to a sphere around the camera so it would not crawl, and chose that because the game is slow and watchful. ([devlog](https://dukope.com/devlogs/obra-dinn/tig-32/)) For a calm pond the rule is the same. Tie any dither to pond coordinates and keep it static, and never animate it per frame.

Two caveats specific to us. First, our water image is 2 px per cell and Ghostty scales it up about 5x bilinearly. Its GPU interpolates between our 8-bit values, so part of the banding is smoothed for free, and dither noise at source resolution becomes soft 5-pixel mottling. Second, `layers.rs` skips sending the water when the bytes did not change. A static dither pattern keeps that working. A time-varying one would force a resend every water frame.

## 5. Calm always-on apps

- **xscreensaver.** Each hack's draw callback returns how many microseconds to sleep, and jwz's rule is that a draw must return in a small fraction of a second. The delay is not reduced by the draw time, so real fps is `1/(draw + delay)`. ([screenhack.c](https://github.com/Zygo/xscreensaver/blob/master/hacks/screenhack.c))
- **macOS screen savers** default to 30 fps through `animationTimeInterval`. ([Apple docs](https://developer.apple.com/documentation/screensaver/screensaverview/1512471-animateoneframe))
- **Wallpaper Engine** has separate policies for when another app is focused, maximized or fullscreen, and pauses by default in games. ([docs](https://help.wallpaperengine.io/en/performance/game.html))
- **Android live wallpapers** must use CPU "only while visible". ([docs](https://developer.android.com/reference/kotlin/android/service/wallpaper/WallpaperService.Engine))
- **Browsers** pause `requestAnimationFrame` in background tabs, and Chrome throttles timers hard after 5 minutes hidden. ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame), [Chrome](https://developer.chrome.com/blog/timer-throttling-in-chrome-88))

The common idea is three states, not two: visible and active, visible but in the background, and not visible (paused). A terminal program cannot see occlusion, only focus. Our measurements say the only lever that reaches single-digit CPU is sending fewer updates, and our perf phase showed that an unchanged image costs Ghostty nothing.

## 6. Movement feel

- **Head leads, body follows.** Argonaut's chain and Rain World's point-and-distance creatures both steer a few points and let constraints do the rest. ([Argonaut](https://github.com/argonautcode/animal-proc-anim), [Rain World GDC 2016](https://www.gdcvault.com/play/1023475/Animation-Bootcamp-Rainworld-Animation)) We have this.
- **Smooth wander, not jitter.** Reynolds' wander target on a circle ahead of the fish, nudged by smooth noise. ([Reynolds 1999](https://www.red3d.com/cwr/papers/1999/gdc99steer.pdf), [Nature of Code](https://natureofcode.com/autonomous-agents/)) We have this.
- **Burst and coast.** Real fish alternate a short active phase with a glide. We have this, and research/koi.md covers the sources.
- **Secondary motion.** David Rosen's GDC 2014 talk argues for a few poses blended by springs over many keyframes. ([GDC Vault](https://www.gdcvault.com/play/1020583/Animation-Bootcamp-An-Indie-Approach)) Applied to a fish, that means fins on springs relative to their body segment, so they lag during a turn and settle after. The talk's specifics are from memory of the video and **unverified** in text.
- **Reaction delay and spread.** Fish that react at slightly different times read as individuals. We already delay noticing by ripple travel time.

What makes motion look robotic is mostly things we have already fixed: per-frame random steering, instant heading changes, constant speed and a tail beat on a global clock. The remaining risk is visual, not behavioral: correct motion shown through a coarse lattice looks jerky however good the sim is.

## What we should do

Ranked by benefit per effort. The first two address the known stepping issue directly.

1. **Sub-pixel shift inside the sprite.** Keep the placement at whole screen pixels (cell plus `X`/`Y`). Carry the leftover fraction, converted to sprite pixels, into the pose shader by changing `origin` from `vec2<i32>` to `vec2<f32>` and adding the fraction, with the same change in the CPU path. Bilinear sampling of the body texture then slides the fish continuously between placement steps. Benefit: removes the step-every-few-frames judder at cruise speed, which is the most visible smoothness problem left. Effort: small, about 15 lines across `layers.rs`, `koi.rs` and `koi.wgsl`. No prior art does this in a terminal; it is the pixel-art "snap then shift back" idea done inside our own renderer.
2. **Render koi at native screen pixels.** Set `fish_px = 0` (one sprite pixel per screen pixel) so Ghostty draws the koi at 1:1 and adds no bilinear resample of its own. Together with 1, a fractional shift becomes a true sub-pixel move with no second resample. Benefit: sharper koi and cleaner sub-pixel motion. Effort: a config default plus a perf check. Koi images get about 1.6 to 1.9 times bigger. Our perf phase found koi images cheap next to the water, but measure the `d` stats and Ghostty CPU before adopting.
3. **Verify the step with a recording before and after.** Add a debug mode that moves one koi in a straight line at 0.1 screen px per frame, record Ghostty, and count distinct positions. This tells whether the reported 1.3 px steps come from the sprite lattice, the placement, or Ghostty's scaled drawing. Effort: an hour.
4. **Interpolate to the scheduled frame time, not the wake time.** The loop already schedules against absolute deadlines (`last_present = due` in `main.rs`), as timg does. But `blend` and the sim catch-up use `now`, the moment the loop woke. Use `due` for both, so late wakeups do not show as uneven spacing (Ladavac). Benefit: steadier spacing at 60 fps and noticeably steadier at 8 fps unfocused. Effort: a few lines.
5. **Skip unfocused koi updates that would not show.** In the 8 fps calm state, send a koi only if its placement moved or its pose changed by more than a threshold (say half a screen pixel of tail travel). Unchanged images are free in Ghostty. Benefit: lower unfocused CPU, towards the 4 to 5% floor estimated in proto/perf/RESULTS.md. Effort: medium, since it needs a cheap "would the pixels change" test on the pose.
6. **Static dither in the water shader, only if banding is visible.** Before the `* 255.0 + 0.5` in `water.wgsl`, add triangular noise of ±1 LSB from a small blue-noise tile indexed by water pixel coordinates, fixed in time. Benefit: breaks up banding in dark water without breaking the unchanged-image skip. Effort: small. Check on the dusk-indigo palette first. Ghostty's 5x bilinear upscale may already hide most banding, and the dither will show as soft mottling at that scale.
7. **Critically damped spring on heading.** Replace the current clamped first-order lag on turn rate with Holden's half-life spring on heading, keeping the turn-rate cap as a clamp. Benefit: turns that ease in and out evenly, with a lag you can predict when tuning. Effort: small code change, then tuning. The current system is already second-order, so expect a subtle gain.
8. **Fin follow-through on springs.** If the pectoral fins are painted into the body texture today, give them their own small angle on a damped spring relative to joint 1: flared during glides, tucked during bursts, lagging during turns. Benefit: the secondary motion that separates "alive" from "correct". Effort: medium, because fins have to leave the static body texture.
9. **Build palette ramps in OKLCH with a small hue shift.** Generate each palette's water and koi ramps with even OKLab lightness steps and a few degrees of hue rotation per step. Keep physical light mixing in linear sRGB as now. Benefit: richer, less muddy shading. Effort: small, and it touches only `palette.rs` and docs/STYLE.md.
10. **Keep sending `p=1` with every `a=T`.** On Ghostty 1.3.1, dropping `p=` makes placements pile up until the machine lags. Nothing to change, but a comment or test should guard it. `a=f` in-place frame edits (kitty-doom, notcurses) are tip-only, so leave them until a Ghostty release ships them.

Also update docs/ARCHITECTURE.md. It still describes the pose cache and the `render.angles`/`render.phases` keys, which the code no longer has.
