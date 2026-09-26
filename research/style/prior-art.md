# Theme system: how to make looks swappable

A survey of how games, editors and terminals structure swappable themes, and a
concrete schema for koi-pond.

## Key takeaways

1. **Fixed slot names, many files.** Every mature system (base16, Catppuccin,
   Ghostty, Helix, Godot) keeps one fixed set of named slots and ships many
   files that fill them. We already have this: the 21 palette slots. Keep them.
2. **One TOML file per theme, three tables:** `[palette]` (colors),
   `[style]` (how things are drawn: pixel scale, posterize steps, outlines,
   grain, dither), `[scene]` (what is in the pond and how much of it).
   A fourth small `[light]` table holds sun direction and warm/cool mixing.
3. **Inheritance by `extends = "name"`, merged key by key.** Helix does exactly
   this with `inherits`. A variant theme is then 5 to 15 lines: it names its
   parent and lists only what changes. Time-of-day variants are just themes
   that extend a daytime theme.
4. **Built-ins compiled in with `include_str!`, user themes in
   `~/.config/koi-pond/themes/`, same name wins for the user.** This is the
   Ghostty and Helix layout. Every built-in is also a readable file in
   `themes/`, so "copy a file, change a few values" works from day one.
5. **Reuse the config merge.** `config.rs` already merges a user TOML table
   over a default table and warns on unknown keys. Theme resolution is the same
   operation run along the `extends` chain. No new machinery.
6. **Hex in the file, OKLCH in the tool.** Keep theme files plain `#RRGGBB`.
   Generate new palettes with a small script that builds hue-shifted ramps in
   OKLCH from a few seed colors and writes a theme file. Runtime stays dumb.
7. **Hue-shift the ramps.** Shadows rotate toward cool (blue/violet), lights
   toward warm (yellow). Slynyrd uses about 20 degrees per step over 9 steps.
   That is the painted, Ghibli-ish quality, and it matches STYLE.md's
   "warm light, cool shadow" rule.
8. **Pixelation is one number.** A Short Hike renders low-res and scales up
   with no smoothing, with a player-facing "pixel size" option. Our
   `water_px` already is that knob; move it into `[style]` as `pixel_scale`.
9. **Switch live with `t` / `T`, reload with `r`.** A theme switch rebuilds the
   pond (the static layers are painted once on the CPU) but keeps the seed and
   the koi. Hot reload of the current theme file can be a one-second mtime
   poll; the `notify` crate is optional.

## The schema

### Files and lookup

```
themes/                          # in the repo, compiled in with include_str!
  base.toml                      # every key, with defaults; never shown in the cycle
  summer-garden.toml
  evening-garden.toml            # extends = "summer-garden"
  jade-morning.toml
  dusk-indigo.toml
~/.config/koi-pond/themes/       # user themes; same file name replaces a built-in
  my-rainy-day.toml
```

The theme name is the file stem. Resolution for theme `X`:

1. Start from `base.toml` (compiled in, holds every key).
2. Walk the `extends` chain from the root down, merging each file over the
   result. A file with no `extends` extends `base`.
3. Merge the user's `config.toml` `[theme.overrides]` table last (the
   successor of today's `[palette.colors]`).
4. Look each name up in the user directory first, then the built-ins.
5. Stop with a warning on a cycle or a chain deeper than 8, and fall back to
   `summer-garden`.

Unknown keys warn and are ignored, same as `config.rs` does today. A bad
color warns and keeps the inherited value. A broken user theme never stops
the game.

### config.toml

```toml
[theme]
name = "summer-garden"
# The order `t` and `T` step through. Empty means every theme, sorted by name.
cycle = ["summer-garden", "evening-garden", "jade-morning", "dusk-indigo"]
# Re-read the current theme file when it changes on disk.
watch = true

[theme.overrides]
# Any theme key, for example:
# palette.koi_red = "#EE6343"
# style.grain = 0.0
```

`[palette] name` and `[palette.colors]` can stay as deprecated aliases for a
release, or just be removed; there are no users yet.

### A complete theme file

```toml
# themes/summer-garden.toml
name = "Summer Garden"          # display name for the status line
description = "Bright summer afternoon, sandy shallows, pink lotus."
extends = "base"

[palette]                       # the 21 slots from STYLE.md, #RRGGBB
deep = "#217E9E"
mid = "#43B09A"
shallow = "#78D8CC"
highlight = "#FFF3D2"
# ... all 21 slots

[light]
sun = [-0.6, -0.8]              # direction light comes from; STYLE.md: upper left
warm = 0.35                     # how far lit sides mix toward highlight
cool = 0.30                     # how far shaded sides mix toward the hue of deep
ambient = 1.0                   # overall brightness multiplier (dusk < 1)

[style]
pixel_scale = 2                 # water pixels per cell width (today's water_px)
koi_pixel_scale = 8             # today's fish_px; 0 = native terminal pixels
posterize = 3                   # tone bands on the floor and stones; 0 = smooth
depth_bands = 3                 # flat water depth bands
band_softness = 0.15            # width of the soft edge between bands, 0..1
outline_px = 1                  # outline on koi, pads and stones; 0 = none
outline_mix = 0.45              # outline = body color darkened toward deep hue
koi_tones = 3                   # cel steps on the koi
grain = 0.03                    # static paper grain, fraction of brightness
dither = "none"                 # "none" | "bayer2" | "bayer4"
dither_strength = 0.0
glint = 1.0                     # crest glint and halo strength
caustics = 1.0                  # caustic net strength; 0 = off
ripple_ink = 1.0                # light/dark hand-drawn ripple lines
step_hz = 8                     # ambient motion steps per second (STYLE.md: 1/8 s)

[scene]
floor = "sand"                  # "sand" | "pebbles" | "moss" | "slate" | "clay"
pebbles = 0.6                   # density, 0..1
rim_stones = 1.0                # fraction of the rim that is stone
moss = 0.4
lily_clusters = 3               # count
lily_pads = 5                   # pads per cluster, average
flowers = 0.35                  # fraction of pads with a lotus
foliage = ["top-left", "bottom-right"]   # corners with overhang
foliage_density = 0.7
dapple = 0.6                    # leaf shadow strength on the water
clouds = 0.5                    # reflection strength; 0 = clear sky
petals = 7                      # drifting petals and leaves
turtles = 0
```

### A variant is a few lines

```toml
# themes/evening-garden.toml
name = "Evening Garden"
extends = "summer-garden"

[palette]
deep = "#4A5A8C"
highlight = "#FFCFA0"
stone_dark = "#6E6484"
# ... only the slots that change

[light]
sun = [-0.9, -0.3]              # lower sun, longer shadows
warm = 0.5
ambient = 0.9

[scene]
clouds = 0.3
```

```toml
# ~/.config/koi-pond/themes/pixel-summer.toml
name = "Pixel Summer"
extends = "summer-garden"

[style]
pixel_scale = 4
koi_pixel_scale = 16
posterize = 4
depth_bands = 4
band_softness = 0.0
grain = 0.0
dither = "bayer4"
dither_strength = 0.5
```

The second example is the whole point: a "more pixelated" look is a
10-line file that touches nothing but `[style]`.

### Which params go where

The test: **palette** is "what color", **light** is "where the sun is and how
strongly it tints", **style** is "how any shape is drawn", **scene** is "what
shapes exist". A style must work with any palette, and a scene with any style.

| Table | Contents | Changes needs |
|---|---|---|
| `palette` | 21 color slots | rebuild static layers (colors are baked into the floor and surface layer) |
| `light` | sun direction, warm/cool mix, ambient | rebuild static layers |
| `style` | pixel scale, posterize, bands, outline, koi tones, grain, dither, glint, caustics, ripple ink, step rate | rebuild; pixel scale also resizes images and clears the koi pose cache |
| `scene` | floor type, pebble/moss/rim density, lily clusters, flowers, foliage corners and density, dapple, clouds, petals, turtles | rebuild with the same seed |

Things that stay in `config.toml` and not in themes: frame rates, backend,
koi count, speed, calmness, audio, key bindings. A theme is a look, not a
behavior. Borderline: `koi` count and `turtles`. I put turtles in `scene`
because a "turtle pond" theme is plausible; koi count stays in config.

Everything in the table rebuilds, so in practice there is one code path:
`build()` with a new resolved theme and the old seed. If rebuild turns out
slow, the cheap case to split out later is palette-only swaps on the GPU
path, since `water.wgsl` gets colors from a uniform, but the baked CPU layers
still need repainting, so do not optimize before measuring.

### Keys

| Key | Action |
|---|---|
| `t` | next theme in `cycle` |
| `T` | previous theme |
| `r` | reload the current theme from disk |

On switch, show the theme's `name` in the status line for a couple of
seconds. Keep the pond seed and the koi (position, heading, pattern) so the
switch reads as "same pond, different light", which is also the nicest way to
compare themes side by side. A short crossfade is a nice later touch but not
needed; the terminal replaces the water image in one frame.

`ui_accent` already exists for the status line, so no new UI slot is needed.

### Hot reload

Two options, both small:

- **mtime poll.** Once a second, `stat` every file in the current theme's
  `extends` chain. If any mtime changed, re-resolve and rebuild. No
  dependency, fits the existing frame loop, and editors that save by rename
  still bump the mtime of the path.
- **`notify` crate.** Watch the user theme dir and `themes/` in dev builds,
  drain events each frame, debounce about 200 ms because editors emit several
  events per save.

Recommend the poll. It is a dozen lines and the loop already runs. In dev,
also read `themes/` from disk instead of the compiled-in copy when
`CARGO_MANIFEST_DIR/themes` exists, so editing a built-in theme reloads live
without a rebuild. With `watch = true` and a second terminal holding the
theme file open, tuning a palette becomes: save, look, save.

### Generating palettes: a script, not a runtime feature

`scripts/theme-gen` (a uv Python script or a small Rust bin) takes a few
seeds and writes a full theme file:

```toml
# input: seeds.toml
water = "oklch(0.62 0.10 205)"   # becomes deep/mid/shallow
foliage = "oklch(0.62 0.12 140)" # becomes lily_dark/lily_light, moss
stone = "oklch(0.78 0.05 85)"    # becomes stone_light/stone_dark, food
sun = "oklch(0.96 0.05 90)"      # becomes highlight
hue_shift = 20                   # degrees per ramp step, warm toward light
koi = "classic"                  # keep koi colors from a named set
```

Method, from the sources below:

- Build each ramp in OKLCH, where equal lightness steps look equal across
  hues, so blue water and green pads at the same L read equally bright.
- Step L evenly (never to 0 or 1), let C peak in the middle of the ramp and
  drop at both ends, and rotate hue with L: darker steps turn toward blue or
  violet, lighter steps toward yellow. Slynyrd's ramps use 9 steps and about
  20 degrees of hue per step; for our 3-slot ramps use 10 to 20 degrees
  between slots.
- When a color falls out of sRGB, reduce chroma until it fits. Do not clip
  RGB channels, which shifts hue.
- Keep koi colors mostly fixed across themes (vermilion, cream, sumi, gold)
  and only tint them slightly toward the light. Koi are the subject; the pond
  is the setting.

The script prints hex, so theme files stay readable and diffable, and the
game never needs an OKLCH parser. A contrast check (koi against each water
band, in OKLCH L difference) belongs in the same script.

Optionally, the script can also import a Lospec palette
(`https://lospec.com/palette-list/{slug}.json` returns
`{name, author, colors: [hex...]}`) and assign its colors to our slots by
lightness and hue. That makes "try this pixel-art palette on the pond" a
one-liner. The mapping is heuristic; expect to hand-fix a few slots.

## Prior art, in detail

### Terminal and editor color schemes

- **base16 / Tinted Theming.** A scheme is YAML with `system`, `name`,
  `author`, `variant` (dark or light) and a `palette` of 16 slots
  `base00`..`base0F`. `base00`..`base07` are a lightness ramp (dark to light
  for dark themes), `base08`..`base0F` are semantic accents. The lesson: a
  ramp plus a few accents, with fixed slot names, is enough to theme hundreds
  of apps. The weakness: slot names like `base0D` are opaque. Our semantic
  names (`deep`, `shallow`, `lily_dark`) are better for a small game.
  https://github.com/tinted-theming/home/blob/main/styling.md
- **Ghostty.** A theme is an ordinary config file that happens to set only
  colors (`palette = N=#hex`, `background`, `foreground`, cursor and
  selection). User themes live in `$XDG_CONFIG_HOME/ghostty/themes`, looked
  up by file name; hundreds are built in. Supports
  `theme = dark:X,light:Y`. The lesson: a theme file with the same syntax as
  the config means one parser and one merge.
  https://ghostty.org/docs/features/theme
- **Helix.** TOML themes in `~/.config/helix/themes`, built-ins shipped with
  the editor, `inherits = "other"` to extend a theme, and a `[palette]` table
  of named colors that the rest of the file references. The closest match to
  what we want. https://docs.helix-editor.com/themes.html
- **Alacritty.** `[general] import = [...]` loads other TOML files in order,
  the importing file last. Same idea as `extends`, but by path rather than by
  name. By-name is friendlier for a theme picker.
  https://alacritty.org/config-alacritty.html ,
  https://github.com/alacritty/alacritty-theme
- **Zed.** A JSON theme family holds several themes (light and dark) with an
  `appearance` field. Our equivalent is a family of files linked by
  `extends`. https://zed.dev/docs/extensions/themes
- **Catppuccin.** Four "flavors" share the same 26 color names with different
  values. Good evidence that fixed names plus variants scale, and a model for
  naming our themes as a family (Summer, Evening, Night of the same garden).
  https://catppuccin.com/palette/

### Game engines

- **Godot Theme resources.** Themes hold typed items (colors, constants,
  fonts, icons, styleboxes). Lookup cascades: local override, parent control
  theme, project theme, engine default, type default. Type variations let one
  control type have several presets. The lesson for us: separate *types* of
  item (our palette/style/scene split) and a clear cascade (base, extends
  chain, user override).
  https://docs.godotengine.org/en/stable/tutorials/ui/gui_skinning.html
- **Unity URP Volumes.** A Volume Profile holds overrides (bloom, color
  grading, and so on); only overridden parameters apply, and several volumes
  blend by weight and priority. The lesson: "only the keys you set" is the
  right default for variants, and blending two resolved themes by a weight is
  a clean model for time of day if we ever want continuous transitions.
  https://docs.unity3d.com/6000.0/Documentation/Manual/urp/Volumes.html
- **Usagi Shima (Godot).** Real-time day/night: four base tints (day,
  sunrise, sunset, night), lerped over set durations, plus light sources whose
  energy is lerped on the same clock, all driven by one time singleton. For us
  that suggests: time-of-day is a small set of themes, and a later "follow the
  clock" mode lerps resolved palettes and `light` values between the two
  nearest ones. Scene and style should not lerp; they switch.
  https://usagishima.net/2021/04/13/devlog-2-real-time-day-night-cycle/
- **LUT color grading.** Many 2D games do day/night by lerping between color
  lookup tables in the shader. Heavier than we need since our colors are
  already slot-driven, but the idea (grade the final image, blend two grades)
  is a fallback if palette lerp looks wrong.
  https://halisavakis.com/my-take-on-shaders-color-grading-with-look-up-textures-lut/ ,
  https://shahriyarshahrabi.medium.com/day-night-cycle-using-lut-in-fragment-shader-of-materials-80edaf26f655
- **A Short Hike.** Renders to a low-resolution target and scales it up with
  no smoothing, and exposes Options > Graphics > Pixel Size. One integer
  controls the whole "pixelated" axis, and the art was modelled to look right
  under it. That is our `pixel_scale`.
  https://steamcommunity.com/app/1055540/discussions/0/1639792569850442961/

### Pixel art palettes and ramps

- **Slynyrd, Pixelblog 1.** Ramps of 9 swatches with about 20 degrees of
  positive hue shift per step as brightness rises; saturation peaks mid-ramp
  and never hits 0 or 100; brightness never starts at 0. 8 ramps spaced 45
  degrees apart make a full palette where everything harmonizes. He notes the
  final values still come down to eyeballing.
  https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes
- **RampenSau.** Generates a ramp from `hStart`, `hCycles`, `sRange`,
  `lRange` and easing functions. A compact parameter set for our generator
  script. https://github.com/meodai/rampensau
- **OKLCH ramp generators.** Perceptually uniform lightness, and gamut
  mapping by reducing chroma rather than clipping channels. Exports to
  Aseprite-friendly formats.
  https://dev.to/ivan_kulkin_1522025957eee/building-an-oklch-palette-generator-that-survives-gamut-limits-and-exports-to-aseprite-4g51 ,
  https://atmos.style/playground
- **Lospec and Aseprite formats.** Lospec serves every palette as PNG, PAL,
  ASE, TXT, GPL or HEX, plus JSON at `/palette-list/{slug}.json`. Aseprite
  reads .gpl, .pal, .act, .aco, .hex, .png and .ase. These formats are flat
  lists of colors with no roles, which is why a palette import needs a slot
  mapping step, and why our own format should name slots.
  https://lospec.com/palettes/api ,
  https://lospec.com/palette-list/importing-palettes ,
  https://community.aseprite.org/t/installing-new-palettes/14310
- **W3C Design Tokens (DTCG).** Tokens can alias other tokens with
  `{group.name}`. Tempting for "outline = darker deep", but it adds a
  reference resolver for little gain. Our `outline_mix` style param gets the
  same effect with a number. Skip aliases.
  https://www.designtokens.org/tr/drafts/format/

### Hot reload

- Rust games usually hot-reload data, not code: watch a directory with
  `notify`, drain events each frame, debounce around 200 ms.
  https://silverweed.github.io/Rust_game_programming_hotloading/ ,
  https://docs.rs/game-toolkit-assets/latest/game_toolkit_assets/

## Open questions for Jeremy

- Should `base.toml` be a real, visible theme (the default look) or only a
  hidden set of defaults? I lean hidden, with `summer-garden` extending it,
  so the default theme can change without touching the base.
- Do user themes replace a built-in of the same name, or should built-in names
  be reserved like Helix does? I lean "user wins": it is the easiest way to
  tweak a built-in, and it is a personal app.
- Is a clock-following time-of-day mode wanted, or is `t` to switch enough?
  The schema supports both; the lerp is extra code.
