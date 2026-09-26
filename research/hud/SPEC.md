# HUD spec

A small, calm HUD for the pond: music, food, scene and time of day. It is
hidden by default and made of a few rounded pebbles at the bottom of the
window. This spec builds on research/hud/survey.md, which has the reasoning
and most of the sources. The mockup is research/hud/mockup.html.

The pond is the star. The HUD only holds choices, never status. There are no
meters, counters, scores or anything that can run out.

This version applies the review of the first draft. The review points I did
not take, or took only in part, are listed at the end with reasons.

## At a glance

- **Hidden** until you press a HUD key or `Tab`.
- **Peek.** A HUD key shows only the pebble it changed, with a one-word label,
  for 3 s. A new track does the same for the music pill.
- **Expanded.** `Tab` shows the whole row: a music pill and four small pebbles
  for food, scene, time and help. No key hints on the row; `?` lists the keys.
  It fades 4 s after the last HUD input, once the pointer has left it.
- **Tray.** Clicking the food, scene or time pebble opens one rounded tray of
  choices above it.
- **Hover reveal** is off by default. With `hud.hover = true`, resting the
  pointer in the bottom 4 rows for 0.5 s expands the row.
- **Rendering.** Each pebble is its own Kitty image at `z=-2`, painted in Rust
  at native cell pixels and re-sent only when it changes. Text is ordinary
  terminal text printed over the image with no background color.
- **Theme.** Colors and shapes come from the active theme. Painterly themes get
  soft painted stones. Pixel themes get chunky pixel pebbles.

## Layout

Sizes are in terminal cells. A cell is about twice as tall as it is wide, so
a 6x3 pebble is close to square. Examples use a 160x45 window; rows count
from 1 at the top.

```
                                (pond)
row 37              ╭──────────────────────────────╮
row 38              │  ●     ✿     ◦     ✦     ◐   │   tray, only while picking
row 39              │  1     2     3     4     5   │
row 40              │           petals             │
row 41              ╰──────────────────────────────╯
row 42  ╭──────────────────────────────╮ ╭────╮ ╭────╮ ╭────╮ ╭──╮
row 43  │ ♪  Rain on Stone           ▸ │ │ ●  │ │ ▦  │ │ ☀  │ │? │
row 44  ╰─────●●◐○○────────────────────╯ ╰────╯ ╰────╯ ╰────╯ ╰──╯
row 45  (empty margin)
        └───────────── 30 ─────────────┘ 2  6  2  6  2  6  2 4
```

The box lines stand for the painted edge. They are not extra rows: a pebble
is 3 rows, and its edge runs through the top and bottom rows. The icon sits
across rows 42 and 43, the title in row 43, and the volume dots in row 44 on
the stone.

| Element | Size (cols x rows) | Where |
|---|---|---|
| Music pill | 30 x 3 | Left end of the row |
| Food, scene, time pebbles | 6 x 3 each | After the pill, 2-col gaps |
| Help pebble | 4 x 3 | Right end |
| Whole row | 60 x 3 | Bottom center, rows `R-3` to `R-1`, one empty row below |
| Tray | 16 to 32 x 5 | Directly above its pebble, centered on it, clamped to the window |
| Peek label chip | text + 2, x 1 | One row above the peeking pebble |
| Help card | 34 x 7 | Centered on the row, bottom edge at the row's top |

Inside the pill (content is in the middle and bottom rows; the top row is
mostly the painted edge and the top half of the icon):

| Cols | Row 2 | Row 3 |
|---|---|---|
| 2 to 4 | Note icon (painted, spans rows 1 to 2) | |
| 6 to 24 | Track title, terminal text, 19 chars, cut with `…` | 5 volume dots (painted) at cols 7 to 11 |
| 26 to 28 | Next icon `▸` (painted) | |

Inside a 6x3 pebble the icon sits in rows 1 to 2 (about 40 px square). Row 3
is plain stone.

**Placement.** `hud.align = "center"` (default) or `"left"`. Left puts the row
at column 3, so it never covers the middle of the pond. The top-left is never
used: the sun and the foliage live there (docs/STYLE.md).

**Small windows.**

| Window width | Change |
|---|---|
| 64 cols or more | As above |
| 44 to 63 | Title cut to 8 chars, pill shrinks to 20 cols |
| under 44 | Pill shows only the note icon (6 cols); title only appears in peek |
| under 14 rows | No expanded state. Peek still works. |

## States

```
          HUD key, track change                      Tab, 0.5 s dwell at the
 hidden ───────────────────────────▶ peek ─────────▶ bottom (hud.hover), click
   ▲  ◀──────── 3 s, fade 0.6 s ─────┘               on the peek pebble
   │                                                          │
   │                                                          ▼
   └────────── 4 s idle and pointer away, Esc, Tab ──── expanded ◀──▶ tray
                                                              │
                                                  ? ──▶ help card (until ?, Esc or any key)
```

| State | Shows | Enters on | Leaves on |
|---|---|---|---|
| Hidden | Nothing | Start, timeout, `Esc`, `Tab` while expanded | See below |
| Peek | The one pebble that changed, plus a label chip above it ("petals", "Rain on Stone", "volume 60%", "Evening Garden", "evening") | A HUD key (not `Tab`), a track starting, a theme switched by the clock (only with `time.follow_clock`) | 3 s after the last change, then a 0.6 s fade. Any new HUD key restarts the 3 s. |
| Expanded | The whole row | `Tab`, 0.5 s of pointer dwell in the bottom 4 rows (only with `hud.hover`), a click on a peeking pebble | 4 s after the last HUD input and with the pointer outside the HUD. `Tab` and `Esc` close at once. |
| Tray | Expanded, plus one tray | Click the food, scene or time pebble. Also `1` to `5` while expanded shows the food tray briefly with the choice marked. | Choosing (closes 0.4 s later, so you see the choice land), clicking the same pebble, `Esc`, or the HUD hiding |
| Help | A 34x7 card with the keys in two columns, no prose | `?` or clicking the help pebble | `?`, `Esc`, or any other key (which then does its normal job) |

If peek and expanded both apply, expanded wins. The peek label chip also shows
over the expanded row when a key changes something, so you see the result by
the pebble you would click.

**Timing.**

| What | Value | Config |
|---|---|---|
| Fade in | 0.25 s, 4 alpha steps | `hud.fade = true` (false: instant) |
| Fade out | 0.6 s, 4 alpha steps | same |
| Peek hold | 3 s | `hud.peek_secs` |
| Expanded hold | 4 s after the last HUD input, and only once the pointer has left | `hud.hold_secs` |
| Hover dwell | 0.5 s in the bottom 4 rows | none |
| Tray close after a choice | 0.4 s | none |

Terminal text cannot fade. It appears on the third alpha step of a fade in and
disappears at the start of a fade out. At 0.6 s that reads as the text
"settling" first, which is fine.

**Fades need frames.** A peek the user did not trigger (a new track, a clock
switch) does not raise the frame rate, so at the 8 fps calm rate a 0.25 s fade
would collapse to 2 steps. While any HUD element is fading, the frame-rate
logic gets a `hud` activity reason, next to `input`, `food` and `ripples`. It
lasts at most 0.6 s per fade.

The row replaces today's row-1 status line for now playing and volume. The
row-1 line stays for the `d` stats line and config warnings, which are
diagnostics, not HUD.

## Controls and shortcuts

Every control has a key. The mouse is optional. Keys are rebindable under
`[input]`, like the existing ones.

| Control | Key | Mouse | Status |
|---|---|---|---|
| Show or hide the HUD | `Tab` | 0.5 s dwell at the bottom (with `hud.hover`) | new |
| Close tray, help or HUD | `Esc` | Click the water closes a tray | new |
| Help card | `?` | Click the help pebble | new |
| Feed | `f` | Click the water | exists |
| Choose food | `1` to `5` | Food pebble, then tray; scroll over the food pebble cycles | new |
| Next track | `n` | Click `▸` | exists |
| Mute | `m` | Click the note | exists |
| Volume | `+` `-` | Scroll over the music pill | exists |
| Next / previous scene | `t` / `T` | Scene pebble, then tray | exists (theme loader) |
| Later / earlier time of day | `l` / `L` | Time pebble, then tray | exists (theme loader) |
| Reload theme and config | `r` | none | exists (theme loader), not in the HUD |
| Quit, stats | `q`, `d` | none | exist, not in the HUD |

`1` to `5`, `?`, `L`, `Tab` and `Esc` are unused today, so nothing
clashes with `q`, `f`, `d` or click.

### Music

- **Now playing.** Title in the pill, and in peek when a track starts. No
  artist. Long titles are cut with `…`, never scrolled.
- **Next track.** `n` or the `▸` icon, the existing `Event::NextTrack`. The
  title itself is not a button: a click meant to expand the row or focus the
  window must never skip the song.
- **No previous track.** It would need a play history on top of the shuffle in
  `audio.rs`, and the pill is simpler without it.
- **Volume.** `+` and `-` step by 0.1 (`audio.rs`), so the 5 dots fill in
  halves: each dot is empty, half or full, and every press shows a change.
  Filled in the accent color.
- **Mute.** Muted, a small leaf covers the note (in `lily_light`) and the dots
  go hollow. No red cross.

### Food

- **Choose.** `1` to `5`, the tray, or scroll over the food pebble. The pebble
  always shows the chosen food. Peek label is the food name.
- **Feed.** `f` or click the water drops the chosen food. Clicks inside a
  visible HUD element never drop food. A click on the water while a tray is
  open closes the tray and still feeds.

### Scene and time of day

A scene is a theme `family`, and each theme in it has a `time`, one of seven:
dawn, morning, noon, afternoon, evening, dusk, night (themes/summer-garden.toml
lists both keys). Neither is inherited through `extends`: a theme without
`family` is a family of one, named by its id. Today's families:

| Family | Times it has |
|---|---|
| garden | dawn (morning-mist), noon (summer-garden), evening (evening-garden), night (moonlit-pond) |
| pixel-garden | noon (hillside-summer), dusk (lantern-dusk) |
| cedar-shade, maple-afternoon, petal-spring, rainy-afternoon, ink-and-vermilion, pocket-moss | one each, at noon |

`koi_theme::Catalog::next_scene` and `next_time` implement the stepping rules
below.

- **Scene.** `t` / `T` step through families; the tray shows one water swatch
  per family. Peek label is the theme's `name`.
- **Time.** `l` / `L` step through the times the current family has, and the
  time tray lists only those. A choice always changes the pond. In a family
  with one time, `l` peeks "noon only" and changes nothing.
- **Switching scene** keeps the current time if the new family has it,
  otherwise uses the family's noon, otherwise its first time.
- **Keep the koi.** Every theme switch keeps the pond seed and each koi's
  position, heading and pattern, as research/style/SYSTEM.md requires. The
  switch in `main.rs` repaints in place (`Water::set_theme`, `Poser::recolor`,
  `Layers::recolor`), so the koi and the waves carry on.
- **Follow the clock** is off by default (`time.follow_clock = false`). It is
  the only thing that would change the scene without being asked. When on, the
  pond uses the latest time the family has at or before the local hour (dawn
  05:00, morning 07:00, noon 11:00, afternoon 14:00, evening 17:00, dusk
  19:00, night 21:00), and the time pebble peeks when it switches. Pressing
  `l` or using the tray turns it off for the rest of the run.

The time pebble shows a sun at a height for the time (full sun at noon, on the
horizon at evening, half set at dusk) or a moon at night.

### Mouse details

- **Hit testing** is by cell rectangles, from the layout table. The HUD knows
  where each visible element is; `main.rs` asks it first and only drops food
  if the HUD did not take the click.
- **Scroll** is SGR buttons 64 and 65, already reported with modes 1000 and
  1006. No new mouse mode.
- **Hover** (`hud.hover = true` only) needs mode 1003 (any motion). Motion must
  not set `last_input`: today `main.rs` sets it for every input before
  parsing, so the input loop has to parse first and count motion as activity
  only when the HUD says its view changed. With hover on, the pebble under the
  pointer lifts by 1 px, its outline warms toward `highlight`, and its key
  shows in its bottom row. That is the only place key hints appear.
- **No precise aiming.** Every target is at least 2x1 cells, and the pebbles do
  not move while you aim, per the Game Accessibility Guidelines' "make
  interactive elements that require accuracy stationary"
  ([gameaccessibilityguidelines.com](https://gameaccessibilityguidelines.com/full-list/)).

## Look

The HUD is painted to match the scene, like Spiritfarer's hand-drawn UI or
A Short Hike's pixel-crisp icons (survey). Each element is a rounded stone
lying on the water.

### Colors from the palette

Mixes are in linear light, like the water renderer. No new palette slots.
`ui_text`, `ui_dim` and `ui_accent` already exist (`koi_theme::Palette`) and the
theme system derives them when a file leaves them out.

A pond is dark when the OKLCH L of `mid` is under 0.45 (moonlit-pond,
lantern-dusk).

| HUD color | Light pond (default) | Dark pond |
|---|---|---|
| Fill | `stone_light` mixed 55% toward `koi_white` | `stone_dark` mixed 35% toward `deep` |
| Lit edge (top-left) | Fill mixed 50% toward `highlight` | Fill mixed 25% toward `highlight` |
| Shaded edge (bottom-right) | Fill mixed 35% toward `stone_dark` | Fill mixed 40% toward `deep` |
| Outline | `stone_dark` mixed 40% toward `deep`; the theme's `outline` in pixel themes | `deep` darkened; `outline` in pixel themes |
| Cast shadow | `deep` at 30% alpha | same, 40% alpha |
| Ink (text, icon outlines) | `koi_sumi` mixed 15% toward `deep` | `ui_text` |
| Dim ink (help card keys) | Ink mixed 45% toward fill | `ui_dim` |
| Accent (volume dots, selection dot) | `ui_accent` | `ui_accent` |

Contrast check: OKLCH L of fill minus L of ink must be at least 0.45. If not,
push ink toward black (light pond) or white (dark pond) until it is. Every
icon has a 1 px ink outline, so the same check covers icons: a pale petal on a
cream pebble still reads by its outline. The HUD should never be the brightest
thing on a night pond, which is why dark ponds get dark pebbles.

Selection in a tray is shown by shape and position, not color alone: the
chosen item sits 2 px higher and has a small accent dot under it
([gameaccessibilityguidelines.com](https://gameaccessibilityguidelines.com/full-list/),
"no essential information conveyed by a fixed colour alone").

### Painterly themes

- Shape: a rounded rectangle with corner radius about one row, and its edge
  pushed in and out by a few percent with low-frequency noise seeded per
  element, so each pebble is a slightly different stone. The seed is fixed, so
  the shape never wobbles.
- Shading: three flat tones, lit top-left, body, shaded bottom-right, with
  short soft edges between them (the same rule as the rim stones).
- Soft outline, 1 device px, a little darker than the fill. Never black.
- Cast shadow, soft, inside the element's image bounds. The shape fills its
  cells down to the last pixel row, so the volume dots sit on the stone. That
  leaves no room below, so the shadow falls mostly to the right (3 px right,
  1.5 px down).
- The theme's paper grain on top, static.
- Icons are simple filled shapes with a 1 px ink outline: a round note, a
  pellet, a petal, a small water swatch, a sun or moon. Cartoon, not detailed.

### Pixel themes

- The HUD uses its own pixel size: 2 device px on normal cells, 3 on HiDPI,
  whatever the pond's `pixel_px`. At the pond's 5 or 6 px, a 6x3 pebble is
  only 10 to 12 art pixels across and icons turn to mush (the mockup tried
  it). Mixed pixel sizes are the most common pixel-art mistake inside a scene
  (research/style/pixel.md, "Mixels"), but the HUD floats over the scene as
  UI, and a finer UI grid reads as UI. If that bothers you, the fallback is
  fewer, bigger icons at the pond's own size.
- Each element starts its own HUD grid at its top-left cell corner. When the
  cell size is not a multiple of the HUD pixel (21 px tall cells with a 2 px
  grid), the leftover device pixels at the right and bottom are transparent.
- Shape: a rectangle with stepped corners (2 or 3 steps), a 1 art-px outline
  in the theme's `outline` slot, 2 tones plus the outline, a 1 art-px lit
  line along the top and left, no grain, no soft shadow (a hard 1 art-px
  shadow on the right instead).
- Icons are drawn smooth on their own layer, then thresholded at about 40%
  alpha and snapped to the theme's colors, so no soft edge survives.
- Paint at art resolution, then upscale with nearest neighbor in Rust and send
  at native device pixels. Ghostty samples Kitty images with a linear filter,
  so anything not sent 1:1 blurs (research/style/pixel.md, section 1).
- Text stays terminal text in Ghostty's font. It is crisp and easy to read,
  and a pixel font would mean writing a rasterizer.

## Rendering

### Layers

| z | Layer | Image ids |
|---|---|---|
| -1000 | Water | 1 |
| -100 + k | Koi | 100+ |
| -10 | Food sprites | 2 to 41 (5 foods x 8 fade levels) |
| -2 | HUD elements | 60 to 67 |
| text | Track title, labels, help text | none |

Kitty draws images with negative z under the text, and the second placement
with the same image and placement id replaces the first "without flicker"
([kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)).
Images with the same z are ordered by id, so the tray (65) draws over a pebble
it touches.

| Id | Element |
|---|---|
| 60 | Music pill |
| 61, 62, 63, 64 | Food, scene, time, help pebbles |
| 65 | Tray |
| 66 | Help card |
| 67 | Peek label chip |

### Only send what changed

A new `hud.rs` owns the state machine and the painting. Each frame it builds a
small `HudView` for every element: visible or not, alpha step, hovered,
selected item, volume step, muted, theme id, and cell rectangle. It compares
that with what it last sent, the same way `Layers::encode` compares pellets.

- Unchanged: nothing is written.
- Changed look (volume, food, hover, alpha step): repaint that one element and
  send it with `a=T,...,i=<id>,p=1,z=-2,C=1`. The music pill is about
  300x60 px, 72 KB, through the existing shm ring.
- Hidden: `a=d,d=i,i=<id>`. The lowercase delete keeps the image data, so
  showing it again with the same look is a single `a=p` placement.
- Theme switch or resize: repaint everything once.

Fades repaint with an alpha multiplier and re-send, 4 times per fade, with no
extra ids. Start there and measure; pre-sending 4 alpha copies like the
pellets' 8 fade levels is the fallback if bytes matter.

Nothing on the HUD animates while idle: no marquee, no spinning disc, no
equalizer bars. An open, idle HUD sends nothing.

### Fit the shm ring

`ShmRing::transmit` returns an error when an image is larger than a slot
(src/term.rs), and slots are sized from the water image and koi canvases
(src/layers.rs). HUD images must fit:

- The ring's slot size also takes the largest HUD element into account: the
  tray (32x5 cells) or the help card (34x7 cells), whichever is larger in
  bytes. At 10x21 px cells the help card is 340x147 px, about 200 KB; about
  800 KB on HiDPI (20x42 px cells).
- The water image at 160x45 cells is about 240 KB, so on HiDPI the help card
  would be the largest image. Sizing the ring from it is simpler than
  splitting the card.

### Text

Text is printed after the images, at its cell, with a foreground color only:
`\x1b[38;2;R;G;Bm` then the text then `\x1b[0m`. No background color, or the
cell paints a solid box over the pebble. To remove text, erase exactly its
cells with ECH (`\x1b[<n>X`), which uses the default background and lets the
pond show through again. The HUD tracks the text spans it wrote and erases
only those.

## Food types

Five foods. Each has its own sprite set (8 fade levels, like the pellets),
float and sink times, how many pieces a drop makes, how far koi notice it,
how they approach, its ripple and its chime. Chimes stay on the ambient
layer's yo pentatonic scale, so any mix of foods stays in key. Sprites do not
rotate: that would need rotated copies for every fade level.

Today: one pellet, floats 20 s (`FOOD_FLOATS`), sinks over 5 s
(`FOOD_SINKS`), noticed within `NOTICE_RANGE` 10.

| # | Food | Phase | Look | Pieces per drop | Floats, sinks | Ripple and chime | Koi notice | Koi reaction |
|---|---|---|---|---|---|---|---|---|
| 1 | **Pellets** (default) | 1 | Round ochre beads in `food`, lit top-left | 1 to 3, close together | 20 s, then 5 s | Small plop, current chime | 10 (as now) | Nearest koi turn and glide in at the current feeding speed. One gulp each. |
| 2 | **Flakes** | 1 | Pale cream and orange flecks, irregular, about half a pellet | 8 to 12, scattered wide | 30 s, then 8 s, drifting slowly | Almost no splash, chime an octave up and quieter | 12 | Several koi come at cruise speed, not feeding speed, and nibble. No rush, no crowding. |
| 3 | **Petals** | 1 | `lily_flower` and `koi_white` petals, drifting | 3 to 5 | Never sink, fade over 60 s | A touch on the water, a very soft high bell | 6 | A koi rises, mouths at a petal once, then loses interest. Decoration more than food. |
| 4 | **Seeds** | 2 | Small dark seeds, `koi_sumi` mixed toward `food` | 3 to 6 | Sink from the start over 10 s, in a slow spiral, dimming with depth | Dull plop, low wooden tone | 8 | Koi follow them down: their shadow tightens and they dim a little, as if diving, then come back up. Needs a per-koi dim tint in the pose shader. |
| 5 | **Treat** | 2 | One watermelon or orange slice, larger, in `koi_red`, `lily_light` and `ogon` | 1, only one in the pond at a time | 60 s, shrinking bite by bite, never sinks | Round splash, a warm two-note chime | Whole pond | All koi drift over and circle it slowly, taking turns nibbling. A second drop while one floats does nothing but a soft tap. Needs a new circling behavior. |

Phase 1 foods are mostly new values for things the pellet code already has:
sprite, piece count, float and sink times, notice range, approach speed. Until
phase 2 lands, the food tray shows three items and the keys are `1` to `3`.

Rules that keep it calm:

- No hunger, no overfeeding, no score. Uneaten food fades.
- Each food keeps its own cap on pieces in the water, so holding `f` cannot
  flood the pond.
- The koi's `fps.dart_speed` activity rule still holds: flakes and petals
  never make koi dart, so they do not raise the frame rate beyond the ripple
  time.
- Flakes stand in for bread, which is bad for real koi (survey, "Food types").

## Config

```toml
[hud]
enabled = true
hover = false         # true: 0.5 s dwell at the bottom reveals the row; needs mode 1003
align = "center"      # "center" | "left"
fade = true           # false: show and hide at once
peek_secs = 3.0
hold_secs = 4.0

[time]
follow_clock = false  # true: pick the family's time from the local clock

[input]
hud = "\t"
help = "?"
next_theme = "t"
prev_theme = "T"
later = "l"
earlier = "L"
food = ["1", "2", "3", "4", "5"]
```

Decided: remember the theme (so scene and time), volume and mute between runs,
in `$XDG_STATE_HOME/koi-pond/state.toml` (`src/state.rs`), not in config.toml.
The theme is stored there today; volume and mute join it with the HUD. Food
resets to pellets each run.

## Implementation plan

In order. Each step leaves the pond working.

Status: all of it is built except `time.follow_clock`, which stays off by decision. The file
names below are from before the workspace split. The HUD is `src/hud.rs` (state machine,
hit tests, drawing) and `crates/koi-render/src/hud.rs` (stones and icons), input parsing is
in `crates/koi-term`, food kinds in `crates/koi-sim`, chimes in `crates/koi-audio`, food
sprites in `crates/koi-render/src/food.rs`, and the wiring in `src/main.rs` and
`src/layers.rs`. Volume and mute are remembered in `src/state.rs`.

1. **Input parsing (`main.rs`).** Parse before setting `last_input`. Add `Tab`
   (byte 9), lone `Esc` (a `\x1b` at the end of a read), `?`, `1` to `5`, and
   SGR buttons 64 and 65 (scroll). Keep `\x1b[` handling as it is; the time
   keys are `l`/`L`, so there is no `[` clash.
2. **Food kinds (`koi.rs`), phase 1.** Add a `kind` to `Food` and a small table
   of per-kind values replacing the global `FOOD_FLOATS`, `FOOD_SINKS` and
   `NOTICE_RANGE`. `drop_food` takes the kind. Pellets, flakes, petals.
3. **Food sprites (`layers.rs`).** One sprite set per kind, 8 fade levels
   each, ids 2 to 41. Size the shm ring from the largest HUD element too.
4. **Food chimes (`audio.rs`).** A `Event::Chime(kind, pan)` that plays one
   short note from the pentatonic set over the ambient layer.
5. **`hud.rs`, state machine only.** Hidden, peek, expanded, tray, help; the
   timers; hit testing by cell rects; a `HudView` per element. Unit-testable
   without a terminal.
6. **`hud.rs`, painting.** Paint the pill, pebbles, tray, chip and help card
   from palette colors, painterly first. Send through the ring at z=-2 only on
   change. Print text with foreground color only; erase with ECH.
7. **Wire it (`main.rs`).** Route keys and clicks through the HUD first. Status
   from `audio.rs` (`NowPlaying`, `Volume`) drives peeks. Add the `hud`
   frame-rate reason while a fade runs. Retire the row-1 now-playing line.
8. **Themes (needs the theme loader from research/style/).** Scene and time
   pebbles and trays, `t`/`T`/`l`/`L`, keeping the koi across a switch. Until
   the loader lands, hide the scene and time pebbles.
9. **Pixel look.** Stepped pebbles on the HUD grid, thresholded icons.
10. **Optional.** `hud.hover` with mode 1003 and dwell; `time.follow_clock`;
    the state file.
11. **Food phase 2.** Seeds (per-koi dim tint in `koi.wgsl`) and the treat
    (circling behavior in `koi.rs`).

## Not in this HUD

Visitors and a pond journal, named koi, and rain are good next mechanics
(survey). Each would add at most one pebble. The journal fits as a fifth
pebble that opens a tray of silhouettes. None of them should add a counter.

## Decided

- Bottom center.
- Remember theme, volume and mute between runs (see "Config").
- The pixel HUD may use a finer grid than the pond's `pixel_px`.

## Sources

- research/hud/survey.md, with 30 sources on cozy game UIs, Calm Technology,
  terminal constraints and koi food.
- research/style/SYSTEM.md, research/style/pixel.md and themes/summer-garden.toml
  for the theme schema, `family` and `time`, the `t`/`T`/`l` keys and Ghostty's
  linear image filtering.
- [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/):
  z order, replacing placements, lowercase delete keeps image data.
- [Game Accessibility Guidelines, full list](https://gameaccessibilityguidelines.com/full-list/):
  remappable controls, stationary targets, no color-only information, avoid
  flicker, option to hide non-interactive elements.
- [The Art of Designing Intuitive User Interfaces in Cozy Games, SDLC Corp](https://sdlccorp.com/post/the-art-of-designing-intuitive-user-interfaces-in-cozy-games/):
  notifications that fade in and out, soft sound cues, customizable HUD.
- [Radial menus in game design, 300Mind](https://300mind.studio/blog/radial-menus-in-game-design/):
  group related actions into menus that appear only when needed. The tray is
  the flat, calmer version of this.
- [Calm Technology](https://calmtech.com/): use the periphery, the minimum
  technology needed.

## Review points not taken, or taken in part

- **Use `ui_text` and `ui_dim` for ink (item 13): in part.** `ui_accent` is
  used everywhere, and `ui_text`/`ui_dim` are used on dark pebbles. On light
  pebbles they are wrong: they are light colors made for text on the water
  (Summer Garden's `ui_text` is #F7F3E8), and on a cream pebble they fail the
  contrast check. Light pebbles keep the dark ink derived from `koi_sumi`.
- **Drop the nearest-time fallback (item 1): in part.** Dropped for choices:
  the tray and `l` offer only times the family has. A rule is still needed
  when switching scene, since the current time may not exist in the new
  family. It keeps the time if it can, else noon, else the first time. Clock
  mode uses the latest time at or before the hour, which is a rule for the
  clock, not a fallback for a choice.
- **Extend the contrast check to icons (item 6): in part.** Instead of checking
  each icon's fill, every icon gets a 1 px ink outline, and ink already passes
  the check against the fill. Pale icons then read by their outline.
- **Show the hint for the pebble under the pointer (item 5): kept only with
  hover on.** With `hud.hover` off, the terminal reports no motion, so there
  is no pointer to follow. The help card is the only key list by default.
- **Show and hide instantly while unfocused (item 4): not taken.** One rule is
  simpler: a `hud` frame-rate reason while any fade runs, focused or not.
  `hud.fade = false` still gives instant everywhere.
