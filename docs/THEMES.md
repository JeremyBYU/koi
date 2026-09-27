# Themes

A koi theme is a TOML file that sets how the pond looks: its colors (`[palette]`), its light (`[light]`), how shapes are drawn (`[style]`) and what is in the pond (`[scene]`). The pond layout, the koi and the simulation are the same in every theme, so a switch keeps every fish in place and changes only the look. This file is the reference for writing one. docs/STYLE.md describes the built-in themes and the art direction.

## Files

| Where | What |
|---|---|
| `themes/summer-garden.toml` | The root. It lists every key with its default and a comment, and every resolution starts from it. |
| `themes/pixel.toml` | A hidden base for pixel themes (see "Pixel themes"). |
| `themes/<id>.toml` | The built-in themes. Release builds compile them in. Debug builds read them from the repository, so edits show without a rebuild. |
| `$XDG_CONFIG_HOME/koi-pond/themes/<id>.toml` | User themes, or `~/.config/koi-pond/themes/<id>.toml` when `XDG_CONFIG_HOME` is unset. A user file with a built-in's id replaces that built-in whole. |

The id is the file name without `.toml`. `--theme` and `theme.name` accept the id or the display name: "Evening Garden" is lower-cased and its spaces become dashes, so it finds `evening-garden.toml`. `koi --list-themes` lists every theme.

A user theme can be three lines:

```toml
# ~/.config/koi-pond/themes/my-garden.toml
name = "My Garden"
[palette]
koi_red = "#EE6343"
```

The game starts with `--theme NAME` if given. Otherwise it starts with the theme you last switched to, until `theme.name` in config.toml names a different one, and then with `theme.name` (default `summer-garden`).

## Resolution

1. Start from the built-in `summer-garden.toml`. It is the base even when a user file replaces `summer-garden`: the user file then merges over it.
2. Follow the `extends` chain from the root down to the theme. A theme with no `extends` extends `summer-garden`. `lantern-dusk` extends `pixel`, which extends `summer-garden`.
3. Merge each file over the one before. Tables merge key by key. Lists replace whole, so `foliage = ["top-left"]` replaces the parent's two corners.
4. Fill the derived palette slots from the final values of the slots they come from.

Two kinds of key are never inherited:

- The top-level keys belong to the file they are written in. A theme that extends `summer-garden` does not join the `garden` family.
- A derived palette slot set in a parent was picked for the parent's colors, so a child derives it again unless it sets it itself.

Every other key is inherited, including the 14 required palette slots. A theme lists only what it changes. An integer is fine where a float is expected (`caustics = 1`).

## Top-level keys

| Key | Default | Meaning |
|---|---|---|
| `name` | the id | Shown in the toast and the HUD. |
| `description` | empty | One line for `--list-themes`. |
| `extends` | `"summer-garden"` | Parent theme id. |
| `family` | the id | Themes in one family are the same place at different times. A theme with no family is a family of one. |
| `time` | `"noon"` | `dawn`, `morning`, `noon`, `afternoon`, `evening`, `dusk` or `night`. Orders the family. |
| `credit` | empty | Palette author and URL. Set it when the palette is borrowed, for example from Lospec. |
| `hidden` | `false` | A base for other themes, left out of the `t` and `l` cycles. |

`t` and `T` step to the next or previous family, in built-in order and then user themes by file name. They keep the current time if the new family has it, else take its noon, else its earliest time. `l` and `L` step to a later or earlier time in the current family and wrap round.

## `[palette]`

Colors are `"#RRGGBB"`. Every theme resolves 22 slots: 14 required and 8 derived.

### Required

| Slot | Used for |
|---|---|
| `deep`, `mid`, `shallow` | Water color by depth band. `shallow` also tints the floor seen through the water. |
| `highlight` | Sunlight: lit sides, caustics, ripple lines, glints, mist. |
| `shadow` | Shade: cast shadows and cool sides. Its hue at full brightness tints the unlit floor. |
| `stone_light`, `stone_dark` | Rim stones, sand, pebbles, slate, planks. |
| `lily_dark`, `lily_light` | Pads, moss, bank, foliage. |
| `lily_flower` | Flower petals, blossom petals, maple leaves. |
| `koi_white` | White koi skin. Also white petals and water lilies. |
| `koi_red` | The red of Kohaku, Sanke and Showa. |
| `koi_sumi` | The black of Sanke and Showa, and the eyes. |
| `ogon` | The gold of the Ogon, and flower centers. |

### Derived

The rules mix in linear light.

| Slot | Rule when unset | Used for |
|---|---|---|
| `outline` | `mix(koi_sumi, shadow, 0.35)` | Koi outlines, and the edges on stones and pads in `dark` and `selout`. For `outline = "rim"` it is the light edge. |
| `cloud` | `mix(koi_white, highlight, 0.4)` | Cloud reflections. |
| `asagi_blue` | `mix(koi_white, shadow, 0.5)` | The Asagi's blue-grey back. |
| `asagi_red` | `mix(koi_red, ogon, 0.3)` | The Asagi's orange flanks. |
| `food` | 0.75 times the average of `stone_light` and `ogon` | Food pellets, the ochre in the sand, plank wood. |
| `ui_text` | `koi_white` | Top line and HUD text. |
| `ui_dim` | `mix(shallow, highlight, 0.5)` | Secondary HUD text. |
| `ui_accent` | `ogon` | Errors and emphasis. |

`swatches` is an optional list of colors that `style.palette_lock` snaps to. Empty means every slot color. See "Palette lock".

### Starting from seed colors

To start a palette, pick a few seed colors (water, foliage, stone, sunlight) and make a three-step ramp from each in OKLCH. The darker step drops lightness by about 0.13 and turns toward blue-violet (hue 265), the lighter step rises and turns toward warm yellow (hue 95), by at most about 15 degrees per step, with chroma 15 to 25 percent lower at both ends. That gives warm light and cool shadow. Then tune by hand. Leave the koi colors out of this: they are what the eye tracks, and they stay close to real koi colors in every theme.

## `[light]`

| Key | Default | Meaning |
|---|---|---|
| `sun` | `[-0.6, -0.8]` | Direction toward the sun in image space, y down. The default is upper left. It is normalized on load, and a zero vector is an error. |
| `warm` | 0.2 | How far lit sides of stones, pads and koi mix toward `highlight`. |
| `cool` | 0.15 | How far shaded sides mix toward `shadow`. |
| `ambient` | 1.0 | Multiplies the whole water image at the end of shading. |
| `shadow_len` | 1.0 | Multiplies every cast shadow offset. Above 1 reads as a low sun. |
| `diffuse` | 0.0 | 0 is direct sun, 1 is overcast: cast shadows and caustics fade out. |

## `[style]`

| Key | Default | Meaning |
|---|---|---|
| `pixel_px` | 0 | Screen pixels per art pixel. 0 is a painted theme, rendered at `render.water_px`. 4 to 8 is a pixel theme. |
| `tone_steps` | 3 | Posterize steps for floor light, pads and stones. At least 1. |
| `depth_bands` | 3 | Flat water depth bands. At least 1. |
| `band_softness` | 0.3 | Width of the blend between bands and tones, as a fraction of one band. 0 is a hard edge. |
| `koi_tones` | 3 | Cel steps on the koi. At least 1. |
| `outline` | `"soft"` | Koi outline mode: `none`, `soft`, `dark`, `selout` or `rim`. See below. |
| `grain` | 0.04 | Static paper grain as a fraction of brightness. 0 also keeps band edges straight. |
| `dither` | `"none"` | `none`, `bayer2` or `bayer4`. Applied to the depth before banding, so it only shows as a seam where water bands meet. |
| `dither_strength` | 0.25 | Dither amplitude as a fraction of one band. Above about 0.3 it reads as speckle. |
| `palette_lock` | `"off"` | `off`, `scene` (the water image snaps to the swatches, the koi keep their colors) or `all` (the koi snap too). |
| `caustics` | 0.6 | Caustic net strength. |
| `caustic_scale` | 1.2 | Caustic cell size multiplier. Larger is coarser and calmer. Above 0. |
| `caustic_softness` | 0.22 | Width of the caustic lines. Above 0. |
| `glint` | 1.0 | Glint strength. |
| `glint_threshold` | 0.7 | Surface tilt toward the sun where glints start. From 0 to below 1. |
| `highlight_style` | `"soft"` | `soft` is a glint with a halo. `dashes` cuts each row into runs of 4 art pixels and lights the first 2 to 4 of a run in `highlight` where the crest tilts past `glint_threshold`. |
| `bloom` | 0.12 | Halo strength around bright crests. |
| `cloud_reflections` | 0.14 | How strongly clouds reflect in the water. |
| `leaf_shadows` | 0.7 | Strength of the dappled shadow under foliage. |
| `wash` | 0.0 | How far the water image mixes toward a vertical gradient. 0 is off. The koi are separate images and stay out of it. |
| `wash_top`, `wash_bottom` | `"#FFFFFF"` | The gradient's colors at the top and bottom edges. |
| `anim_hz` | 8 | Clouds, dapple, caustics and petals advance in steps of 1 / `anim_hz` seconds. Above 0. The waves and koi are not affected. |

### Koi outlines

The outline is drawn when each koi is posed, so the sun side stays on the sun side as the fish turns. `soft` fades in with the edge. The other modes draw a solid line where the koi's opacity crosses 3/4. The translucent fins get no line.

| Mode | Edge color | Built-in use |
|---|---|---|
| `none` | none | |
| `soft` | `mix(mix(mid, body, 0.3), outline, 0.8)` | Summer Garden, Cedar Shade. |
| `dark` | `outline` | Pale and hazy themes. |
| `selout` | `mix(body, outline, 0.45)` on the sun side, `outline` on the far side | The pixel base. |
| `rim` | `mix(outline, body, 0.2)` on the sun side, `mix(body, shadow, 0.6)` on the far side | Moonlit Pond, Lantern Dusk. Set a light `outline`. |

`dark` and `selout` also edge the rim stones, and `selout` edges the pads on their shaded side.

### Palette lock

On load the game builds a table that maps every sRGB color, at 5 bits per channel, to its nearest swatch in OKLab. Each pixel then snaps with one lookup. The lock snaps blended colors too, and the water over the floor is a blend, so a long `swatches` list can pull the water toward grey. List only the colors the theme needs, and check the water in every band after a change.

## `[scene]`

Most of the scene is painted once per theme, so it costs nothing per frame. Petals and weather are the exception: they move, so they are drawn every frame. The layout seed is the same in every theme, so the pond shape, stones and pads stay put on a switch.

| Key | Default | Meaning |
|---|---|---|
| `floor` | `"sand"` | `sand` (ochre and sage patches), `pebbles` (denser, greyer), `moss` (green) or `slate` (streaked grey). |
| `pebbles` | 1.0 | Pebble density multiplier. 0 is none. |
| `rim` | `"stones"` | `stones` of varied size along the shore, or `planks`, a boardwalk ring. |
| `moss` | 1.0 | Moss on the rim stones. |
| `bank` | `"moss"` | The ground beyond the rim: `moss`, `grass` or `gravel`. |
| `pad_size` | 1.0 | Lily pad size multiplier. |
| `flowers` | 0.3 | Share of pads with a flower. Above 0 means at least one. |
| `flower` | `"lotus"` | `lotus` (cupped) or `water-lily` (flatter, star-shaped). |
| `foliage` | `["top-left", "bottom-right"]` | Corners with overhanging foliage: `top-left`, `top-right`, `bottom-left`, `bottom-right`. The first is the large one. |
| `foliage_density` | 1.0 | Multiplies the leaf count and reach. |
| `foliage_kind` | `"broadleaf"` | `broadleaf`, `maple`, `cherry` (with blossom dots), `pine` or `bamboo`. |
| `petals` | 7 | Drifting petals and leaves, at most 24. |
| `petal_kinds` | `["blossom", "white", "leaf"]` | `blossom` (`lily_flower`), `white` (`koi_white`), `leaf` or `maple`. Repeats weight the pick. Must not be empty when `petals` is above 0. |
| `weather` | `"clear"` | `clear`, `rain` (raindrops that make real ripples, about 11 a second at full strength), `mist` (haze in `highlight` toward the edges) or `fireflies` (warm pulsing points, about 14 at full strength). |
| `weather_amount` | 0.0 | Weather strength. |

## Pixel themes

A pixel theme sets `pixel_px` from 4 to 8. Below 4 it stops reading as pixel art. The water renders on an art grid of the window's screen pixels divided by `pixel_px` (rounded up). Each koi is posed one sprite pixel per art pixel and placed on multiples of `pixel_px`, so the fish and the water share one grid. The game scales every image up to exact screen pixels itself, because Ghostty scales Kitty images with a linear filter and would blur them.

Pixel themes extend `pixel`, which sets hard band edges, no grain or bloom, `dashes` highlights, `selout` outlines, `palette_lock = "scene"`, `anim_hz = 4` and `pixel_px = 4`. It sets no colors, so a pixel theme states its own palette.

## Keeping koi readable

The koi are what the eye follows, and moving fish are tracked mostly by lightness, not hue. Red on teal of the same lightness collapses for deuteranopes. Check each koi color (`koi_white`, `koi_red`, `ogon`, `asagi_blue`) against each water band (`deep`, `mid`, `shallow`). A color reads over a band when either holds:

- its OKLab lightness differs from the band's by 0.06 or more, or
- its outline color has a WCAG contrast of 2.5 or more against the band.

In most painted themes the red and the ogon are close to some band in lightness, so the outline carries them. When a theme feels weak, darken `mid` or switch `outline` to `dark` rather than brightening the koi. Keep `koi_sumi` away from every water band, or the sumi patches read as holes. Gold fish over the shallows is the usual worst case. Fine caustics can hide moving fish, so keep `caustic_scale` at 1 or above in painted themes.

## Reload and errors

Once a second the game checks the modification times of the current theme's files, the user theme folder and config.toml. A change reloads and re-resolves the theme, and the pond repaints in place. `r` does the same on demand. A clean reload shows `reloaded <name>` on the top line for 2.5 s. Built-in themes in a release build are compiled in, so edit a copy in the user folder.

An unknown key is a warning that names the file and key, and the rest of the theme loads. These are errors:

- a file that is not valid TOML, or a value of the wrong type
- a color that is not `#RRGGBB`, or an unknown name such as `outline = "thick"`
- an `extends` cycle, or a parent that does not exist
- `light.sun = [0, 0]`, or a `sun` that is not a finite number
- `tone_steps`, `depth_bands` or `koi_tones` below 1, `pixel_px` other than 0 or 4 to 8, `anim_hz`, `caustic_scale` or `caustic_softness` not above 0, `glint_threshold` outside 0 to below 1
- `warm`, `cool`, `diffuse`, `band_softness`, `grain`, `dither_strength`, `caustic_softness`, `bloom`, `cloud_reflections`, `leaf_shadows`, `wash`, `flowers` or `weather_amount` outside 0 to 1
- `ambient`, `caustics`, `glint`, `pebbles` or `pad_size` outside 0 to 2
- `shadow_len`, `caustic_scale`, `moss` or `foliage_density` outside 0 to 4
- `anim_hz` above 60
- `inf` or `nan` in any of these
- `petals` above 24, or `petals` above 0 with no `petal_kinds`

An error while the pond runs keeps the current theme and shows the error on the top line for 6 s, for example `~/.config/koi-pond/themes/my-pond.toml: palette.deep = "#12345" is not a #RRGGBB color (kept Summer Garden)`. An error at start in the theme named by config.toml or remembered from last time loads the built-in Summer Garden instead, shows the error on the top line for 10 s and prints it again on exit. A theme given with `--theme` that doesn't load stops koi with the error instead.
