# A calm HUD for the pond

## Recommendation

- **Hidden by default.** The pond stays a picture. The HUD shows when you
  press `Tab`, press any HUD key, or (optionally) move the mouse to the bottom
  edge. It fades out about 4 s after the last HUD interaction.
- **One small row of pebbles at the bottom.** Four rounded, icon-first
  buttons: music, food, scene, time of day. No words except the track name,
  which only shows while the HUD is open or for a few seconds after a track
  changes.
- **Its own Kitty image layer**, painted once in Rust per theme, above the
  koi and below terminal text. Text (the track name, key hints) is real
  terminal text drawn on top, so it uses Ghostty's font and costs no font code.
- **Keyboard first, mouse second.** Every button has a key. Clicks on a pebble
  do the same thing. Mouse hover is optional polish.
- **Static costs nothing.** An open, unchanging HUD sends nothing per frame.
  Fades are a handful of placement swaps between pre-sent alpha levels, the
  same trick the pellets already use.
- **Food types are the first real mechanic.** Five foods with a different
  look, float and sink time, ripple, chime and koi reaction. Then, if wanted:
  time of day that follows the clock (with gentle rain), and a pond journal of
  rare visitors.

The rest of this file is the survey behind those choices, the terminal
constraints, and the concrete proposals.

## What cozy games do

| Game | What its UI does | What to take |
|---|---|---|
| **Townscaper** | Almost no UI. You click the water and a house grows. A small palette strip and a few corner icons, nothing else. Its author calls it "more of a toy" than a game. ([Wikipedia](https://en.wikipedia.org/wiki/Townscaper), [Game Developer](https://www.gamedeveloper.com/game-platforms/how-townscaper-works-a-story-four-games-in-the-making)) | The best HUD is the pond itself. Clicking the water to feed already works this way. The HUD should only hold choices, never status. |
| **A Short Hike** | Very little HUD. The golden feather count is the one persistent-feeling element, and it matters only while climbing or gliding. Low-res pixel look with crisp icons. ([Game UI Database](https://www.gameuidatabase.com/gameData.php?id=243), [Interface In Game](https://interfaceingame.com/games/a-short-hike/)) | Show a thing when it is relevant, then get out of the way. Pixel-crisp icons at integer scale read well even when tiny. |
| **Animal Crossing: New Horizons** | Round edges, pastel colors, soft textures, minimal on-screen UI. Most tools live in the NookPhone, a rounded phone of big friendly app icons you summon with one button. ([Medium essay](https://medium.com/@t.ecardinal/welcome-to-the-island-the-cozy-game-craft-of-environment-genre-and-atmosphere-in-animal-cd83bcf32801), [Game UI Database](https://www.gameuidatabase.com/gameData.php?id=606), [NookPhone web mockup](https://github.com/IdreesInc/NookPhone)) | Summon, use, dismiss. A small tray of big round icons beats a permanent bar. |
| **Stardew Valley** | The opposite end: a clock and date box always top right, a toolbar always at the bottom, energy bar bottom right. It works because the game is about time and tools. ([Stardew wiki: Energy](https://stardewvalleywiki.com/Energy), [HUD Customizer mod](https://www.nexusmods.com/stardewvalley/mods/23704)) | A persistent HUD is right when you use it every few seconds. The pond is used every few minutes, so it should not be persistent. The popularity of HUD mods shows people want to move and hide HUDs. |
| **Unpacking** | Wordless. No score, no timer, no fail state. The UI was meant to be as small as possible and grew only as features needed it; much of it is skeuomorphic (a to-do list, photo albums). The creative director asks of every feature: "Does my game really need this feature?" ([Game Developer](https://www.gamedeveloper.com/design/unpacking-the-design-pillars-of-a-chill-puzzle-game), [GamesRadar](https://www.gamesradar.com/the-making-of-unpacking-developer-witch-beam-on-bringing-the-moving-indie-hit-to-life/), [Wikipedia](https://en.wikipedia.org/wiki/Unpacking_(video_game))) | No meters, no scores, no fail states. Make controls look like objects from the pond's world: a food pouch, a pebble, a leaf. |
| **Spiritfarer** | Hand-drawn UI that matches the painted world and moves at the game's slow pace. ([Game UI Database](https://www.gameuidatabase.com/gameData.php?id=1238)) | The HUD should be painted in the same style as the current theme, not a generic overlay on top of it. |
| **Alba: A Wildlife Adventure** | A phone camera and a wildlife log of 62 species. No reward or punishment for a good or bad shot. ([ustwo on Medium](https://medium.com/@ustwogames/the-environment-art-of-alba-a-wildlife-adventure-6bddd8b56955), [Engadget](https://www.engadget.com/alba-a-wildlife-adventure-steam-apple-arcade-ustwo-review-152045112.html), [Game UI Database](https://www.gameuidatabase.com/gameData.php?id=1576)) | A journal that fills in by noticing things is gentle and never demands anything. Basis for the visitors idea below. |
| **Kind Words** | One cozy bedroom as the only view, lo-fi music, stickers you collect sit in the room. ([Wikipedia](https://en.wikipedia.org/wiki/Kind_Words_(video_game)), [Steam](https://store.steampowered.com/app/1070710/Kind_Words_lo_fi_chill_beats_to_write_to/)) | Music is part of the mood, not a feature screen. Collected things live in the scene, not in a menu. |
| **Lofi rooms and players** | Lofi Cozy Room switches between 14 rooms with one click, layers ambient sounds, and lets you switch UI themes to match the desktop. Discord's Lofi activity has a day/night toggle. A Go terminal player, `lofi`, spends "its character budget on motion rather than chrome". ([Steam](https://store.steampowered.com/app/4621450), [Discord Lofi FAQ](https://support-apps.discord.com/hc/en-us/articles/26502210208663-Lofi-FAQ), [lofi on pkg.go.dev](https://pkg.go.dev/github.com/aiscenblue/lofi)) | Scene switch and day/night are one-click things. Now playing is one line. |

Two general sources agree with all of this. Calm Technology's principles:
"Technology should require the smallest possible amount of attention", "make
use of the periphery", "can communicate, but doesn't need to speak", and "The
right amount of technology is the minimum needed to solve the problem"
([calmtech.com](https://calmtech.com/)). Cozy-UI write-ups recommend
notifications that fade in and out, soft sound cues, and letting players hide
or customize the HUD
([SDLC Corp](https://sdlccorp.com/post/the-art-of-designing-intuitive-user-interfaces-in-cozy-games/),
[UX Connections](https://www.uxconnections.com/recipe-for-a-cozy-game/)).

## What makes a HUD calm

1. **Absent until asked.** Nothing on screen by default. The pond is the
   status display: food in the water shows you fed, music is heard.
2. **Choices, not status.** No meters, counters, hunger bars, streaks or
   badges. Nothing that can go wrong or run out.
3. **Icon first, few words.** One icon per pebble. The only text is the track
   name and single-letter key hints.
4. **Soft shapes.** Rounded pebbles and pills, soft outline a little darker
   than the fill (the same rule as the koi in STYLE.md), no pure black, no
   hard drop shadows. A small cast shadow down and to the right keeps the
   light direction honest.
5. **Slow, short transitions.** Fade in about 0.25 s, fade out about 0.6 s.
   No bouncing, no sliding across the screen, no pulsing.
6. **Out of the light.** The sun and foliage sit top left, so the HUD sits at
   the bottom. The current status line on row 1 sits right where the painted
   light is; moving it into the HUD fixes that.
7. **Matches the scene.** Painted pebbles in painterly themes, chunky pixel
   pebbles in pixel themes, colors from the theme's `ui_*`, `stone_*` and
   `koi_white` slots.
8. **Quiet sounds.** A soft click or wood tap on open and choose, much quieter
   than the food chime. Optional.

## Terminal constraints

### The HUD as its own image layer

The Kitty protocol has what the HUD needs
([kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)):

- **z order.** "Negative z-index values mean that the images will be drawn
  under the text." Today the water is at `z=-1000`, koi at `-100+k`, pellets
  at `-10`. A HUD at around `z=-2` sits above koi and pellets and still below
  terminal text.
- **Alpha.** RGBA images blend with what is under them, so rounded pebbles
  with soft edges and a translucent backing work.
- **Moving without resending.** "If you send two placements with the same
  image id and placement id the second one will replace the first. This can be
  used to resize or move placements around the screen, without flicker."
- **Hiding without freeing.** Lowercase `a=d,d=i` deletes the placement but
  keeps the image data, so showing the HUD again is one placement command.

So: transmit the HUD images once per theme (and again on resize), then only
place and delete them. The ids between the pellets (2 to 9) and the koi (100+)
are free.

**Text on top.** Cells with the default background let images below them show
through (that is how the whole pond shows today). So the HUD image can be the
panel and icons, and the track name and key hints can be ordinary terminal text
printed into the cells over it, with a foreground color and no background
color. That gives a crisp font at every size with no font rasterizer. The cost
is that text snaps to cells, so the panel should be sized in whole cells. The
current overlay sets a background color; HUD text must not, or it paints a
solid box.

**Painting.** Paint the pebbles and icons once in Rust, the same way koi body
textures are painted once in `koi.rs`, at the real cell pixel size so they are
crisp (native pixels, like the pellets, not scaled up like the water). For a
pixel-art theme, paint at a small size and scale by a whole number with
nearest-neighbor, like A Short Hike.

**Fades.** Kitty has no per-placement opacity. The pellets already solve this:
`layers.rs` transmits 8 fade levels once and switches ids. The HUD can do the
same with 4 alpha levels, so a fade is 4 placement swaps over 0.25 s. No pixel
data moves.

### Mouse

- `term.rs` turns on `?1000h` (press and release) and `?1006h` (SGR). That is
  enough to click pebbles. Clicks already arrive as `CSI < 0 ; x ; y M`; a
  click inside the HUD rectangle should go to the HUD and not drop food.
- **Scroll** arrives as buttons 64 and 65 with the same modes. Scroll over the
  music pebble for volume is cheap to add and very natural.
- **Hover** needs `?1003h` (all motion). It would let the bottom edge reveal
  the HUD and let pebbles lift slightly under the pointer. One catch: `main.rs`
  counts every non-focus input as activity, so plain motion would hold the
  frame rate at 60 while the pointer moves over the window. Motion should only
  count as activity when it changes what the HUD shows.
- **Pixel mode** `?1016h` is supported by Ghostty since 1.0, with the same
  report format as 1006 but in pixels
  ([vtdn: SGR-Pixel](https://vtdn.dev/docs/decset/mode1016-sgr-pixel/)).
  Pebbles are big enough that cell precision is fine. 1016 matters only for a
  hand shadow following the pointer on the water. tmux does not support it
  (research/input.md), so it stays optional.

### Keyboard (primary)

Existing keys: `f` feed, `n` next track, `m` mute, `+` `-` volume, `d` stats,
`q` quit. Proposed additions, all rebindable in `[input]` like the rest:

| Key | Action |
|---|---|
| `Tab` | Show or hide the HUD |
| `1` to `5` | Choose food (shows the food pebble briefly) |
| `f`, click | Feed the chosen food |
| `p` / `n` | Previous / next track |
| `t` | Next scene or theme |
| `[` / `]` | Earlier / later time of day (or back to "follow the clock") |
| `Esc` | Close the HUD |

Any HUD key also shows the HUD for a few seconds, so you see what changed
without opening it. `audio.rs` has `NextTrack` but no previous; that needs a
new `Event` and a short history in the playlist.

### Frame rate

- An open, static HUD costs nothing: `encode` already sends only what changed.
- A fade is 4 placement commands. A key press already raises the rate to 60
  for `input_secs` (3 s), which covers the fade.
- Avoid anything that animates while the HUD is idle: no spinning record, no
  scrolling marquee for long titles (truncate with an ellipsis instead), no
  equalizer bars. Those would keep the rate at 60 all day.
- Switching theme means repainting the static water layers, like a resize. It
  is a one-off cost of a rebuild. It should keep the seed (the resize path
  bumps it) so the pond layout stays the same and only the look changes.

## HUD proposal

### States

1. **Hidden** (default). Nothing drawn.
2. **Toast.** After a track change or a HUD key: one small pill bottom left
   with an icon and a line of text ("Rain on Stone", "volume 60%",
   "petals"). Fades after about 5 s. Replaces the row-1 status line.
3. **Open.** The pebble row, bottom center. Stays while the pointer is over it
   or for 4 s after the last HUD key.
4. **Picker.** Choosing a pebble opens a small rounded tray above it with the
   choices (foods, scenes, times). One tray at a time.

### Layout

```
                          (pond)

              ╭─────────────────────────╮
              │  🍂 pellets  🌸 petals    │   <- food tray (only while picking)
              │  🌾 seeds    🍉 treat     │
              ╰─────────────────────────╯
   ╭───────────────────────╮  ╭────╮ ╭────╮ ╭────╮
   │ ♪  Rain on Stone   ▸▸ │  │ 🍂 │ │ 🏞 │ │ ☀ │
   ╰───────────────────────╯  ╰────╯ ╰────╯ ╰────╯
     n  m  +  -                 1-5    t     [ ]
```

(The emoji only stand in for painted icons.) The music pill is wider because
it holds the title; the rest are single pebbles.

### Pebbles

| Pebble | Shows | Does |
|---|---|---|
| Music | A note icon, the title (truncated), prev and next marks, a small volume dot row | Click title: next. Scroll: volume. Click note: mute (the note gets a small leaf over it rather than a red cross). |
| Food | The chosen food's sprite | Opens the food tray. The pebble shows what `f` and a click will drop. |
| Scene | A tiny thumbnail of the current theme | Opens a tray of theme thumbnails, painted from each theme's palette. |
| Time | Sun, low sun, moon or "clock" | Cycles morning, afternoon, evening, night, and "follow the clock". |

### Look per theme family

- **Painterly:** pebbles in `stone_light` with a cool `stone_dark` underside
  (like the rim stones), a soft outline, a light grain, and a tiny cast shadow
  down and right. Icons in the theme's koi and lily colors.
- **Pixel:** the same shapes as a 1 px dark outline at whole-number scale, 2
  to 3 flat tones, no grain.

research/style/ and themes/ are not written yet; the HUD only needs a few
color slots and a flag for painterly or pixel, so it should read those from the
theme when it exists.

## Food types

Real koi eat floating and sinking pellets, and occasional treats like
watermelon, orange, lettuce and silkworm pupae. Bread is poor for them and
fouls the water
([Next Day Koi](https://nextdaykoi.com/koi-fish-facts/floating-sinking-koi-food/),
[Pond Informer](https://pondinformer.com/what-can-koi-eat/),
[K.O.I.](https://koiorganisationinternational.org/blog-entry/sinking-vs-floating-koi-food)).
Koi feed at the surface in warm months and lower down when it is cold. So the
set below uses flakes instead of bread crumbs; say if you want bread anyway, it
is only a game.

Today every drop is one pellet that floats for 20 s (`FOOD_FLOATS`) and sinks
over 5 s (`FOOD_SINKS`), with one splash size and one chime. Each food below
varies those numbers plus a few more: how many pieces per drop, how they
scatter, how far koi notice them, how koi approach, and the chime pitch.

| Food | Look | In the water | Ripple and sound | Koi reaction |
|---|---|---|---|---|
| **Pellets** (default) | Round ochre beads, as now | 1 to 3 pieces. Float about 20 s, then sink. | Small plop, the current chime | Nearest koi turn and come at a steady glide; each gulp makes a tiny ring. |
| **Flakes** | Pale cream and orange flecks, irregular | A pinch of 8 to 12 tiny pieces scattered wide. Float long, drift slowly with a faint current. | Almost no splash, a soft higher chime | Several koi share, nibbling slowly at the surface. No rush. |
| **Petals** | Pink or white cherry-style petals from `lily_flower` | 3 to 5 that spin slowly and drift. They never sink; they fade after about a minute. | Barely a touch on the water, a very soft bell | Koi rise to look and mouth at them, then lose interest. Most petals are left alone. Decoration more than food. |
| **Seeds** | Small dark sinking seeds | Sink slowly from the start, drifting down in a gentle spiral, shrinking and fading as they go deeper. | A small dull plop, a low wooden tone | Koi follow them down: their shadows tighten and they dim a little, as if diving, then come back up. |
| **Treat** (watermelon or orange) | One larger slice, bright but in palette colors | One piece, floats a long time. Only one in the pond at a time. | A bigger, rounder splash, a warm two-note chime | All koi drift over and circle it slowly, taking turns bumping it. It shrinks bite by bite over about a minute. |

Rules to keep it calm:

- No hunger, no overfeeding penalty, no score. Uneaten food just fades.
- Keep the existing cap on food in the water per type, so a held key or rapid
  clicks cannot flood the pond.
- The treat is limited to one at a time, which makes it feel special without a
  cooldown timer.
- Each food is a few sprites painted once, with fade levels like the pellets
  (5 foods by 8 levels fits in ids 2 to 41).

## Other small mechanics

Pick at most two or three. Each should run on its own and never ask for
anything.

1. **Time of day and weather.** The time pebble can follow the real clock:
   morning, afternoon, evening and night map naturally onto palettes like
   jade-morning, summer-garden, evening-garden and dusk-indigo, switched at
   the hour boundaries. Gentle rain is a toggle (or random on some days):
   more small splashes across the water (the water already takes splashes),
   a rain loop in the ambient layer, and koi a little livelier. This gives the
   all-day-open pond a slow rhythm without any input.
2. **Visitors and a pond journal.** Rarely, something visits: a dragonfly
   rests on a lily pad, a frog sits on a stone, a butterfly crosses, a leaf
   falls and spins. The HUD gets a fifth pebble, a small journal page of
   silhouettes that fill in once you have seen each visitor, like Alba's
   wildlife log with no rewards. Visitors stay still most of the time, so they
   cost almost nothing at the calm rate.
3. **Koi you know.** Click a koi (or press `k` to cycle) to see its name in a
   tiny tag that fades. Names are generated once and saved. Optionally koi
   grow very slowly with the days they were fed, and new ones arrive on long
   intervals. Never death, never sickness.

Not recommended: currencies, shops, daily login rewards, streaks, pop-up
achievements. Each gives the pond a way to nag.

## Open questions for you

- **Hover reveal.** Worth turning on `?1003h`, or is `Tab` plus clicking
  enough? Hover is nicer but needs the motion-is-not-activity rule.
- **Where it sits.** Bottom center, or a corner so it never covers the middle
  of the pond?
- **Persistence.** Should the chosen food, theme, time mode and volume be
  remembered between runs? That needs a small state file separate from the
  config.
- **Bread.** Flakes, or bread crumbs anyway?
- **Which extra mechanic first:** time of day and rain, visitors, or named koi?

## Sources

- [Townscaper, Wikipedia](https://en.wikipedia.org/wiki/Townscaper)
- [How Townscaper Works, Game Developer](https://www.gamedeveloper.com/game-platforms/how-townscaper-works-a-story-four-games-in-the-making)
- [A Short Hike, Game UI Database](https://www.gameuidatabase.com/gameData.php?id=243)
- [A Short Hike, Interface In Game](https://interfaceingame.com/games/a-short-hike/)
- [Animal Crossing cozy craft essay, Medium](https://medium.com/@t.ecardinal/welcome-to-the-island-the-cozy-game-craft-of-environment-genre-and-atmosphere-in-animal-cd83bcf32801)
- [Animal Crossing: New Horizons, Game UI Database](https://www.gameuidatabase.com/gameData.php?id=606)
- [NookPhone web mockup](https://github.com/IdreesInc/NookPhone)
- [Stardew Valley wiki: Energy](https://stardewvalleywiki.com/Energy)
- [Stardew Valley HUD Customizer mod](https://www.nexusmods.com/stardewvalley/mods/23704)
- [Unpacking design pillars, Game Developer](https://www.gamedeveloper.com/design/unpacking-the-design-pillars-of-a-chill-puzzle-game)
- [The making of Unpacking, GamesRadar](https://www.gamesradar.com/the-making-of-unpacking-developer-witch-beam-on-bringing-the-moving-indie-hit-to-life/)
- [Unpacking, Wikipedia](https://en.wikipedia.org/wiki/Unpacking_(video_game))
- [Spiritfarer, Game UI Database](https://www.gameuidatabase.com/gameData.php?id=1238)
- [Alba environment art, ustwo games](https://medium.com/@ustwogames/the-environment-art-of-alba-a-wildlife-adventure-6bddd8b56955)
- [Alba review, Engadget](https://www.engadget.com/alba-a-wildlife-adventure-steam-apple-arcade-ustwo-review-152045112.html)
- [Alba, Game UI Database](https://www.gameuidatabase.com/gameData.php?id=1576)
- [Kind Words, Wikipedia](https://en.wikipedia.org/wiki/Kind_Words_(video_game))
- [Kind Words, Steam](https://store.steampowered.com/app/1070710/Kind_Words_lo_fi_chill_beats_to_write_to/)
- [Lofi Cozy Room, Steam](https://store.steampowered.com/app/4621450)
- [Discord Lofi FAQ](https://support-apps.discord.com/hc/en-us/articles/26502210208663-Lofi-FAQ)
- [lofi terminal player](https://pkg.go.dev/github.com/aiscenblue/lofi)
- [Calm Technology principles](https://calmtech.com/)
- [Designing UI in cozy games, SDLC Corp](https://sdlccorp.com/post/the-art-of-designing-intuitive-user-interfaces-in-cozy-games/)
- [Recipe for a Cozy Game, UX Connections](https://www.uxconnections.com/recipe-for-a-cozy-game/)
- [Kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- [vtdn: SGR-Pixel mouse (1016)](https://vtdn.dev/docs/decset/mode1016-sgr-pixel/)
- [vtdn: Any-event mouse (1003)](https://vtdn.dev/docs/decset/mode1003-any-event/)
- [Floating vs sinking koi food, Next Day Koi](https://nextdaykoi.com/koi-fish-facts/floating-sinking-koi-food/)
- [Sinking vs floating koi food, K.O.I.](https://koiorganisationinternational.org/blog-entry/sinking-vs-floating-koi-food)
- [What can koi eat, Pond Informer](https://pondinformer.com/what-can-koi-eat/)
