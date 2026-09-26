# Summary: recommended stack and plan

This page condenses the seven research files in `research/`. The sources for each claim are in those files.

## Recommended stack

| Area | Choice | Source file |
|---|---|---|
| Language | Rust (cargo 1.93 is installed, Go is not). No GC, one reused `Vec<u8>` per frame, one `write_all`. | language.md |
| Terminal I/O | crossterm for raw mode, keys and mouse, with our own renderer. No ratatui for the pond. | language.md |
| Frame model | Composite everything into one RGB pixel buffer. Only the output step knows about the terminal. | rendering.md |
| Output A (works everywhere) | Half-block `▀` cells in 24-bit color, diffed against the last frame and wrapped in mode 2026. | rendering.md |
| Output B (sharp, Ghostty only) | Kitty graphics over shared memory (`t=s`) at about 480x270, re-sent every frame with `a=T,q=2` and scaled by Ghostty. About 100 bytes per frame go through the PTY. | rendering.md |
| Water | Damped two-buffer wave equation at display resolution. Fixed 60 Hz step, k about 0.15, velocity damping 0.995 to 0.998, sponge band at the shore. Shading comes from derivatives: refraction, analytic caustics, tilt light, thresholded glints and depth tint. Quantize the output so still water sends zero bytes. Measured cost is a 20 us step plus 164 us of shading at 480x240. | water.md |
| Koi | A Reynolds steering brain plus an 8-joint follow-the-leader spine. Traveling-wave tail at Strouhal 0.3, burst-and-coast gait, 40°/s turn cap, 0.6 s steering low-pass. Food state machine: cruise, approach, eat, linger. | koi.md |
| Turtles | Same brain with half the speed and turn rate, a rigid shell, and a trip to the surface to breathe every 1 to 3 minutes. | koi.md |
| Palette | Jade Morning by default and Dusk Indigo for night. Both use the same 21 slots, stored as data. Blend in linear light. | palette.md, palettes.html |
| Input | A left click drops food at the clicked cell center plus some scatter (crossterm `EnableMouseCapture`). Keyboard reticle moves with `hjkl` or arrows, `space` drops food, `f` tosses food near the koi. | input.md |
| Audio | Generative ambient audio made with fundsp on a cpal callback thread. The game sends it events over a lock-free channel. Local files via rodio + symphonia come next. | music.md |
| YouTube | List playlists with the Data API (`youtube.readonly`), open the chosen one in the browser and control it over MPRIS. Do not ship yt-dlp + mpv. | music.md |

## Contradictions found, and how I resolved them

1. **mpv for YouTube.** language.md suggests driving mpv + yt-dlp over IPC. music.md shows this breaks the YouTube ToS and Developer Policies III.E.1, III.I.7 and III.I.9. music.md wins. mpv is only acceptable as an "external player command" the user sets up for their own use.
2. **Which output is primary.** language.md and water.md put half-blocks first and add Kitty later. rendering.md makes Kitty shared memory primary. The build order is the same either way: half-blocks first because they work in tmux, then Kitty behind the same pixel buffer. Which one is the default is an open decision below.
3. **Truecolor in tmux.** rendering.md says tmux should pick up RGB from Ghostty's terminfo `Tc`. palette.md says you must set it or the pond turns grey. I checked this tmux session and palette.md is right, but its fix would not work here.
   - The tmux client reports `client_termname` as `xterm-256color`, not `xterm-ghostty`.
   - `client_termfeatures` has no `RGB` or `sync`, so every color inside tmux is currently mapped to the 256-color palette.
   - Both suggested lines (`xterm-ghostty:RGB` and `xterm-ghostty:RGB:sync`) would not match that terminal name.
   - The fix is `set -as terminal-features ',xterm-256color:RGB'`, or fix Ghostty's `TERM` so tmux sees `xterm-ghostty`.
4. **COLORTERM check.** palette.md suggests checking `COLORTERM=truecolor` at startup. Inside this tmux, `COLORTERM=truecolor` is set because it was inherited from Ghostty, yet tmux still downgrades colors. So the check says yes when the answer is no. When `$TMUX` is set, run `tmux display -p '#{client_termfeatures}'` and look for `RGB` instead.
5. **Kitty shared memory in Ghostty.** language.md says `t=s` support needs testing. rendering.md cites Ghostty source (`readSharedMemory`) showing it exists. Treat it as supported, but still measure the per-frame cost.
6. **Small number differences.** None of these change the plan, and the prototype settles them:
   - Kitty image size: 400x200 to 800x400 (rendering.md) vs 480x270 (water.md).
   - Color quantization: 5 to 6 bits per channel (rendering.md) vs 32 to 64 OKLab steps per hue (water.md).
   - Bytes per half-block cell: 36 vs 40.
7. **Koi size in half-blocks.** rendering.md estimates a koi at about 12x4 pixels. At that length, koi.md's widths (max 0.14 BL) make the body 2 pixels wide, so kohaku patches and sumi spots will barely show. This is the strongest visual argument for the Kitty path and the first thing to check in the prototype.

## Current machine state that matters

- tmux is 3.4 with `mouse on`, `allow-passthrough off`, and no `RGB` or `sync` in the client features.
- tmux 3.4 ignores the app's mode 2026, which is supported only from 3.7 ([tmux#4744](https://github.com/tmux/tmux/pull/4744)). Frames can tear.
- Kitty images in tmux need passthrough plus Unicode placeholders (`U=1`).
- A half-block prototype already exists at `proto/halfblock` (crossterm 0.29, one file).

## Decisions you need to make

1. **Where the pond runs.** Either its own Ghostty window outside tmux (Kitty path, 2026 works, no setup), or inside tmux (half-blocks, needs the `RGB` fix, tears until tmux 3.7).
2. **Upgrade tmux?** Moving to 3.7 or later and adding `RGB:sync` fixes tearing, but it changes your daily setup.
3. **Default renderer.** Either Kitty when available with half-blocks as the fallback, or half-blocks everywhere for one consistent look. Decide after comparing both in the prototype.
4. **Music scope.** Is generative ambient plus local files enough, or do you want the browser + MPRIS YouTube route? That route needs your own Google Cloud project and OAuth client.
5. **Palette.** Open `research/palettes.html` and confirm Jade Morning and Dusk Indigo.
6. **muster tie-in.** The lightest option gives each agent from `GET /api/herd` (port 8630) one koi, and a `waiting` agent's koi rises to the surface. Is that the tie-in you want? Do you also want clicking a koi to jump to that agent's tmux pane?
7. **Turtles.** In v1 or later.

## Build order after the 60 fps prototype

1. **Measure.** For half-blocks at 200x50 and 300x80, record bytes per frame, frame time and Ghostty CPU, both directly and in tmux 3.4. Look for tearing on fast ripples.
2. **Kitty shared-memory output** behind the same pixel buffer. Compare it side by side with half-blocks and pick the default (decision 3). Detect capabilities at startup: `$TMUX`, tmux `client_termfeatures`, and DECRQM for modes 2026 and 1016.
3. **Water.** Wave sim with sponge edge and obstacle mask, refraction, caustics, glints and quantization. Tune k and damping by eye.
4. **Koi.** Start with one fish wandering and following the bank. Then add separation, then weak alignment and cohesion, then the spine and tail wave. Add a headless test with a fixed seed that checks turn rate, speed and spacing stay within limits.
5. **Feeding.** Mouse and keyboard reticle, pellets that drift with the surface, fish that notice food when the ripple reaches them, the food state machine and hunger.
6. **Palette as data**, with Jade and Dusk, plus the tmux truecolor warning.
7. **Generative audio** on the cpal thread, with a chime when food lands. Check that it never disturbs frame timing.
8. **Pond dressing.** Lily pads, rim stones, depth layering and ambient ripples.
9. **Turtles.**
10. **Music integrations.** Local music files first, then MPRIS now-playing, then the YouTube playlist picker.
11. **muster tie-in**, one koi per agent.
