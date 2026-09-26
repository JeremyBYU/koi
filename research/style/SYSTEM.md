# Theme system

A theme is one TOML file that sets how the pond looks: its colors (`[palette]`), its light (`[light]`), how shapes are drawn (`[style]`) and what is in the pond (`[scene]`). The pond layout, the koi and the simulation stay the same across themes, so switching reads as "same pond, different light". This file is the design. All six build steps are in the code (`crates/koi-theme`, `water.rs`, `water.wgsl`, `koi.rs`, `koi.wgsl`, `src/main.rs`, `src/layers.rs`); docs/STYLE.md describes what runs today.

What is here:

- `themes/*.toml`: 12 themes, plus the hidden `pixel.toml` base. `summer-garden.toml` is the root and lists every key with its default.
- `research/style/preview.html`: renders every theme side by side as a mock of the shaders. Click a pond to open it large and animated. Inside that view, `t`/`T` steps the scene, `l`/`L` the time of day, `p` cycles the pixel size and `space` pauses.
- `research/style/build_preview.py`: resolves the themes the way the game will, checks koi readability over every water band, warns on unknown keys, and writes the resolved themes into the preview. It exits non-zero on any problem. Run `python3 research/style/build_preview.py` after editing a theme. `build_preview.py ramp water=#4F9BC8 foliage=#5FA048 stone=#C8A878 sun=#FFF3D0` prints `[palette]` lines from seed colors.

The research behind the choices is in `ghibli.md` (painting method, mood palettes), `pixel.md` (pixel rendering in Ghostty), `koi.md` (readability, what pond players like) and `prior-art.md` (how other tools structure themes).

## Files and resolution

| Where | What |
|---|---|
| `themes/summer-garden.toml` | The root. Every key with its default and a comment. It reproduces today's look, except that caustics are calmer (0.6 at scale 1.2). |
| `themes/pixel.toml` | Hidden base for pixel themes. Hard edges, no grain or bloom, dash glints, `selout` outlines, `palette_lock = "scene"`, `anim_hz = 4`. |
| `themes/<id>.toml` | Built-in themes, compiled in with `include_str!`. In debug builds they are read from disk so edits show up live. |
| `~/.config/koi-pond/themes/<id>.toml` | User themes. A user file with the same id as a built-in replaces it, as in Ghostty and Helix. |
| `config.toml` `[theme]` | `name` (the starting theme). Nothing else yet. |

The id is the file name without `.toml`, and it is the name lower-cased with spaces as dashes, so `--theme` and the toast agree.

Resolution, root first, each step merging key by key over the one before:

1. `summer-garden.toml`. A theme with no `extends` extends it.
2. The `extends` chain. `lantern-dusk` extends `pixel`, which extends `summer-garden`.
3. Derived palette slots are filled last, from the final values of the slots they come from.

Tables merge key by key and lists replace whole (`foliage = ["top-left"]` replaces the parent's list). The existing `merge()` in `config.rs` already does this for config.toml. Two kinds of key are never inherited:

- `name`, `description`, `family`, `time`, `credit` and `hidden` belong to the file they are written in. A new theme that extends `summer-garden` does not silently join the `garden` family.
- A derived palette slot set in a parent fits the parent's colors, so a child derives it again unless it sets it itself.

Unknown keys produce a warning with the file and key, the same way config.toml warns today. An `extends` cycle, a missing parent, a missing required slot or a bad color falls back to `summer-garden` and shows the error in the toast.

```toml
# config.toml
[theme]
name = "summer-garden"
```

The old `[palette.colors]` overrides become a three-line user theme:

```toml
# ~/.config/koi-pond/themes/my-garden.toml
name = "My Garden"
[palette]
koi_red = "#EE6343"
```

`render.water_px` stays as the resolution for painted themes. `style.pixel_px` replaces it when set (see "Pixel themes").

### Retired palettes

The old built-in palettes `jade-morning` and `dusk-indigo` are retired, with no aliases. Both were darker than the garden set, and Moonlit Pond and Cedar Shade now cover that range.

## Top-level keys

| Key | Example | Meaning |
|---|---|---|
| `name` | `"Evening Garden"` | Shown in the toast. |
| `description` | one line | Shown in the preview and in `--list-themes`. |
| `extends` | `"pixel"` | Parent theme id. Default `summer-garden`. |
| `family` | `"garden"` | Themes in one family are the same place at different times. Default: the theme's own id, so a single theme is a family of one. |
| `time` | `"evening"` | One of `dawn`, `morning`, `noon`, `afternoon`, `evening`, `dusk`, `night`. Orders the family. Default `noon`. |
| `credit` | palette author and URL | Required when the palette comes from Lospec or anywhere else. |
| `hidden` | `true` | A base for other themes, left out of the cycle. |

## `[palette]`

Fourteen slots are required. Ten more are derived when unset. Every required slot has a job no other slot can do.

### Required

| Slot | Job | Used today in |
|---|---|---|
| `deep`, `mid`, `shallow` | Water color by depth band. `shallow` also tints the floor seen through the water. | `deep/mid/shallow` in `shade()` (`water.wgsl`) and its CPU twin in `Water::render`. |
| `highlight` | Sunlight: lit sides, caustics, ripple lines, glints. | `sun_col`. |
| `shadow` | Shade color for cast shadows and cool sides. | Today `paint()` takes shade from the hue of `deep` (`shade = deep / max(deep)`), which goes wrong in forest and rain themes. Every theme sets it, so it has no derived rule. |
| `stone_light`, `stone_dark` | Rim stones, sand, pebbles, slate, planks. | `paint()`. |
| `lily_dark`, `lily_light` | Pads, moss, bank, foliage. | `paint()`. |
| `lily_flower` | Lotus and water-lily petals, blossom petals, maple leaves. | `paint()`, petal colors. |
| `koi_white`, `koi_red`, `koi_sumi`, `ogon` | Koi. | `koi.rs` body textures. |

### Derived

| Slot | Rule when unset (mixes in linear light) | Why it is its own slot |
|---|---|---|
| `outline` | `mix(koi_sumi, shadow, 0.35)` | Koi outline and pixel outlines. Replaces `ink` (`palette.deep`) in the `outline` closure in `koi.rs`. For `outline = "rim"` it is the light edge color. 10 of 12 themes set it. |
| `cloud` | `mix(koi_white, highlight, 0.4)` | Cloud reflections. The same mix `Water::new` does today. |
| `asagi_blue` | `mix(koi_white, shadow, 0.5)` | Asagi back. |
| `asagi_red` | `mix(koi_red, ogon, 0.3)` | Asagi belly. |
| `food` | `0.75 * mix(stone_light, ogon, 0.5)` | Pellets, the ochre in the sand, plank wood. |
| `ui_text`, `ui_dim`, `ui_accent` | `koi_white`, `mix(shallow, highlight, 0.5)`, `ogon` | Status line and toast. |

`swatches` is an optional list of hex colors that `palette_lock` snaps to. Empty means every slot color. Themes that use a Lospec palette list the subset they use. A lock snaps blended colors too, and the water over the floor is a blend, so a large list can pull the water to greys. Check every locked theme in the preview.

### Starting from seed colors

`build_preview.py ramp water=… foliage=… stone=… sun=…` prints `[palette]` lines, which you paste into a theme and tune by hand. Each seed becomes three OKLCH steps: the darker step drops lightness by about 0.13 and turns toward blue-violet (265 degrees), the lighter one rises and turns toward warm yellow (95 degrees), at most `hue_shift` degrees per step (default 15), with chroma 15 to 25 percent lower at both ends and reduced rather than clipped when out of gamut. That is the "warm light, cool shadow" rule from `ghibli.md` and `pixel.md`. Slynyrd's ramps turn about 20 degrees over 9 steps (https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes). The game never runs this: themes stay plain hex, as `prior-art.md` recommends.

Koi colors are never generated. They are what the eye tracks and they stay close to real koi colors in every theme.

## Where each key runs

The game has two backends: wgpu (`water.wgsl`, `koi.wgsl`) and a CPU fallback (`Water::render`, `Poser::pose`). The static layers (floor, depth, stones, pads, foliage, grain) are painted once on the CPU by `paint()` and shared by both. Per-frame shading is written twice. So every key is one of:

- **paint**: used only in `paint()`. One implementation, free per frame. Preferred.
- **frame**: used in per-frame shading. Two implementations, `water.wgsl` and `Water::render` (or `koi.wgsl` and `Poser::pose`), which must agree. Most are a number moved into `Params`.
- **koi**: used in the koi texture painter in `koi.rs` (`paint` in `impl Body`). One implementation, run on theme load.

The parity test in "Build order" keeps the two frame implementations honest.

## `[light]`

| Key | Default | Runs | Today | Change |
|---|---|---|---|---|
| `sun` | `[-0.6, -0.8]` | paint, frame | `const SUN` in `water.wgsl` and `water.rs`, plus pad, stone and flower lighting in `paint()`. | Pass to `paint()`, add to `Params`. |
| `warm` | 0.2 | paint, koi | Stones mix toward `sun_col` by 0.2. | Stones, pads, pebbles, koi cel tones. |
| `cool` | 0.15 | paint, koi | Stones mix toward `shade` by 0.15. | Same. |
| `ambient` | 1.0 | frame | none | One multiply at the end of shading. |
| `shadow_len` | 1.0 | paint, frame | Pad shadows `0.012u, 0.018u`, stones `0.02u, 0.028u`, koi `(0.1, 0.15) * len` in `koi_shadow()`. | Multiply every offset. |
| `diffuse` | 0.0 | paint, frame | none | Overcast. Fades cast shadows and caustics, pulls `lit` toward its mean. |

## `[style]`

| Key | Default | Runs | Today | Change |
|---|---|---|---|---|
| `pixel_px` | 0 | pass | `render.water_px = 2`. | New pass, see "Pixel themes". 0 means painted. |
| `tone_steps` | 3 | paint, frame | `steps(x, 3.0)` for floor light in shading, pad and stone tone in `paint()`. | Param. |
| `depth_bands` | 3 | paint | `steps(depth_raw, 3.0)` in `paint()`. | Param. |
| `band_softness` | 0.3 | paint, frame | `steps()` uses `smoothstep(0.35, 0.65, fract)`. | `smoothstep(0.5 - s/2, 0.5 + s/2)`. 0 is a hard edge. |
| `koi_tones` | 3 | koi | Cel steps in the koi painter. | Param. |
| `outline` | `"soft"` | frame (pose pass) | Baked into the koi texture in `koi.rs`: rim mixed toward `mix(mix(c, black, 0.5), ink, 0.35)`. | See "Koi outlines". `none`, `soft`, `dark`, `selout`, `rim`. |
| `grain` | 0.04 | frame | `col * (1 + here.z * 0.06)`. | Param. 0 also stops grain wobbling band edges in `paint()`. |
| `dither` | `"none"` | paint | none | `bayer2` or `bayer4` on the art grid, added to `depth_raw` before `steps()`, so it only makes a thin seam at water band edges. Never on stone or pad tones. |
| `dither_strength` | 0.25 | paint | none | Fraction of one band. Above about 0.3 it reads as speckle. |
| `palette_lock` | `"off"` | frame | none | See "Palette lock". `scene` exempts the koi. |
| `caustics` | 0.6 | frame | `1 + 0.35 * lines * sun` | Multiplies the 0.35. Today's look is 1.0; 0.6 reads as painted light, not a CG net. |
| `caustic_scale` | 1.2 | frame | `scales.z/w` | Multiply. `koi.md` cites a study where fine caustics hid moving fish, so painted themes stay at 1 or above. |
| `caustic_softness` | 0.22 | frame | `smoothstep(0.0, 0.22, cells)` | Param. |
| `glint` | 1.0 | frame | `smoothstep(0.7, 1.0, tilt) * 0.6` | Multiplies the 0.6. |
| `glint_threshold` | 0.7 | frame | the 0.7 above | Param. |
| `highlight_style` | `"soft"` | frame | Glint plus halo. | `dashes`: 1-art-pixel horizontal strokes, 2 to 4 long, placed by a hash on the strongest crests. |
| `bloom` | 0.12 | frame | `smoothstep(0.4, 1.0, tilt) * 0.12` | Param. |
| `cloud_reflections` | 0.14 | frame | `mix(col, cloud, 0.14 * ...)` | Param. |
| `leaf_shadows` | 0.7 | frame | `open = ex.x * (1 - 0.7 * ex.y * (1 - dapple))` | Param. |
| `wash`, `wash_top`, `wash_bottom` | 0, white | frame | none | Vertical gradient mixed into the water image. The koi are separate images (see below), so they stay out of it for free. |
| `anim_hz` | 8 | frame | `const AMBIENT_HZ = 8.0` in `water.rs` | Param. Pixel themes use 4. |

Every frame number fits one extra `vec4` pair in `Params`. None costs anything per frame.

Knobs that no theme sets stay literals until one does: outline width, ripple line strength, koi shadow strength, stone spacing, lily cluster and pad counts, turtles.

### Koi outlines

The fish texture rotates with the fish, so a "sun side" baked into it turns with the fish. Outlines move out of the texture painter into the pose pass (`koi.wgsl` and `Poser::pose`), which writes each koi's screen-aligned image. That pass already has the posed alpha, so it finds edge pixels (alpha set, a neighbor clear) and colors them by mode, with the sun direction in screen space:

| Mode | Edge color | Use |
|---|---|---|
| `none` | none | |
| `soft` | `mix(mix(water, body, 0.3), outline, 0.8)`, anti-aliased | Painted default. |
| `dark` | `outline` | Pale or hazy themes, pixel themes. |
| `selout` | `mix(body, outline, 0.45)` on the sun side, `outline` on the shade side | Pixel art selective outlining. |
| `rim` | `mix(outline, body, 0.2)` on the sun side, `mix(body, shadow, 0.6)` on the shade side, with a light `outline` | Night themes. |

The pose pass does not know the water color under the fish, so `soft` uses `mid` for "water". `recolor()` then only repaints body colors.

As built: the painter marks which texels are edges, and the pose pass colors them, so the sun side is worked out in screen space. `soft` fades in with the edge mark; the other modes draw a line where the koi's opacity crosses 3/4. The translucent fins get no line. A koi following sinking food dives (`Pose::depth`), and the pose pass fades it toward `deep` by `0.3 * depth`; its shadow moves in under it and sharpens.

### Palette lock

Searching up to 26 swatches per pixel per frame is wasteful and hard to keep identical on two backends. On theme load, build a 32×32×32 table that maps each sRGB color (5 bits per channel) to its nearest swatch in OKLab. The GPU gets it as a storage buffer of 32 768 `u32`s, the CPU uses the same array, and the lookup is one index per pixel. Both backends then snap identically. `scene` applies it to the water image only. `all` also runs it in the pose pass.

## `[scene]`

Everything here is painted once on the CPU by `paint()`, so none of it costs anything per frame. The layout seed stays the same across themes, so the pond shape, stones and pads do not jump on a switch.

| Key | Default | Today | Change |
|---|---|---|---|
| `floor` | `"sand"` | Sand with ochre and sage patches. | `pebbles` (denser, greyer), `moss` (green floor), `slate` (streaked grey). One base color and one patch color each. |
| `pebbles` | 1.0 | Hash threshold `mix(0.75, -0.3, depth)`. | Multiply. 0 = none. |
| `rim` | `"stones"` | Stones along the shore, evenly spaced. | Vary stone size, cluster them and leave the odd gap, so the rim does not read as beads on a string (the preview does this). Add `planks`: a boardwalk ring. |
| `moss` | 1.0 | Moss threshold on stones. | Param. |
| `bank` | `"moss"` | Moss green bank. | `grass`, `gravel`. |
| `pad_size` | 1.0 | `r = u * (0.045 + 0.03 * rand)` | Multiply. |
| `flowers` | 0.3 | `rand < 0.3` | Param. At least one flower when above 0. |
| `flower` | `"lotus"` | Lotus. | `water-lily`: flatter, star-shaped. |
| `foliage` | `["top-left", "bottom-right"]` | Two fixed anchors. | Any corners; the first is the large one. |
| `foliage_density` | 1.0 | 90 and 45 leaves. | Multiply counts and reach. |
| `foliage_kind` | `"broadleaf"` | One leaf shape. | `maple`, `cherry` (with blossom dots), `pine`, `bamboo`. |
| `petals` | 7 | `const PETALS = 7` | Param, buffer of 24. |
| `petal_kinds` | `["blossom", "white", "leaf"]` | `pt.b.w` flags leaf or petal. | Add `maple`. Repeats weight the pick. |
| `weather` | `"clear"` | none | `rain` (random `splash()` calls, so rings are real ripples), `mist` (slow noise toward `highlight` near the edges, water image only), `fireflies` (a few warm pulsing points). Mist and fireflies are frame work, so both backends. |
| `weather_amount` | 0.0 | none | Strength. |

## Pixel themes

`pixel_px` is the size of one art pixel in device pixels, an integer from 4 to 8. Below 4 it stops reading as pixel art.

Ghostty scales every Kitty image with a linear filter and has no setting to turn it off (`mag_filter = .linear` in `src/renderer/OpenGL.zig`, https://github.com/ghostty-org/ghostty/blob/main/src/renderer/OpenGL.zig). So a pixel theme cannot look crisp by sending a small image. The fix:

1. Render the water at the art grid, `floor(W_px / pixel_px)` by `floor(H_px / pixel_px)`, from `ws_xpixel`/`ws_ypixel` in `term.rs`. `pixel_px = 5` costs about the same as `water_px = 2`.
2. The koi are separate Kitty images (`layers.rs`). Pose each one at the same art grid with no anti-aliasing, and snap its placement to a multiple of `pixel_px` device pixels, so fish and water share one grid. Mixed grids ("mixels") are the most common way pixel art goes wrong.
3. Scale the water image and each koi image up once with nearest-neighbor to exact device pixels. At 1:1 Ghostty's filter changes nothing. A 1600×945 water frame is about 6 MB, which shared memory carries, and it is only sent when it changed.

As built:

- The art grid rounds up (`ceil(W_px / pixel_px)`), and the scaled-up image is cropped to the window, so the last art column and row may be partial rather than leaving a gap.
- The simulation keeps its `water_px` grid in every theme. `Water` takes splashes and koi shadows in simulation units and scales them by `per_sim`, so a switch between painted and pixel themes (or two pixel sizes) keeps the koi, and `Water::regrid` resamples the waves onto the new grid.
- Pixel koi bodies are painted with hard edges (a texel is covered or not; fins keep their translucency), and the pose pass takes the nearest texel of the nearer beat strength. Dark, selout and rim outlines are then one crisp art pixel.
- Food sprites are drawn on the art grid and placed on it.
- The water has its own shm ring of 6 slots, since a full-window image in every slot of the main ring would cost too much memory. The main ring has 64 slots.
- Dither is only added to the depth before banding, as specified. The preview also dithers the floor light in pixel themes; the game does not, because it covered the shallows in a checkered mesh.
- Dashes: each row is cut into runs of 4 art pixels with a hashed phase; a run lights its first 2 to 4 pixels in `highlight` when the surface at its start tilts toward the sun past `glint_threshold`.
- The lock table index comes from the sRGB bytes, computed with the exact sRGB curve on both backends when locked.

`pixel.toml` holds what every pixel theme shares. Pixel themes restate every palette slot, so they lose nothing by extending it instead of a painted theme. The preview's `p` key cycles the pixel size on any theme.

## Switching live

This follows `research/hud/SPEC.md`, where a "scene" is a family:

| Key | Action |
|---|---|
| `t` / `T` | Next / previous scene (family). Keeps the current time if the new family has it, else its noon, else its first. |
| `l` / `L` | Later / earlier time in the current family. A family of one shows "noon only". |
| `r` | Reload the current theme and config from disk. |

None of these keys is taken today (`f`, `q`, `d`, `n`, `m`, `+`, `-` are). They go into `[input]`.

A switch keeps the koi, their positions, headings and patterns, and the wave heights. It re-resolves the theme, calls `paint()` with the same seed, uploads the new statics, updates the color uniforms and calls `Poser::recolor(&School, &Theme)`, which repaints koi textures without resetting the fish. (The textures live in `koi-render`, and `koi-sim` does not know about colors, so it is `Poser`, not `School`.) When `pixel_px` changes, the height field is resampled to the new grid, not cleared. `paint()` paints bands of rows in parallel, about 5 ms for a 160x45 window and 70 ms for a 4K one on a 48-thread machine, so the switch runs on the main thread and is instant.

### Toast

A switch peeks the HUD's scene stone with the new theme's name, or its time stone with the new time (research/hud/SPEC.md). The top line only carries problems: an error shows for 6 s in `ui_accent` on a translucent `shadow`, and the current theme stays: `themes/my-pond.toml: palette.deep = "#12345" is not a #RRGGBB color (kept Summer Garden)`. A reload with no problem shows `reloaded Summer Garden` for 2.5 s.

### Hot reload

Once a second the game checks the modification times of every file in the current theme's extends chain, plus config.toml. On a change it re-resolves and switches, and the toast says `reloaded Evening Garden`. A parse error keeps the current theme on screen. Polling needs no dependency.

## Keeping koi readable

Every theme must pass `build_preview.py`, which checks white, red, ogon and asagi against all three water bands. Hue does not count: moving fish are tracked mostly by lightness, and red on teal of equal lightness collapses for deuteranopes. A koi color reads over a band when either holds:

- its OKLab lightness differs from the band by 0.06 or more, or
- its drawn edge (per outline mode, above) has WCAG contrast 2.5 or more against the band.

The script also reports the distance after a deuteranopia simulation (Machado et al. 2009), for information. Since the rule ignores hue, a pass holds for deuteranopes too. It flags a `koi_sumi` equal to a water band, because sumi patches then turn into holes.

The preview puts the ogon over the shallows, the worst case for gold fish. In most painted themes red and ogon are within 0.05 lightness of some band, so the outline carries them. When a theme feels weak, darken `mid` or strengthen the outline rather than brightening the koi.

Current worst band per color (lightness gap, and what carries it):

| Theme | white | red | ogon | asagi |
|---|---|---|---|---|
| morning-mist | +0.17 light | -0.02 edge | +0.03 edge | +0.01 edge |
| summer-garden | +0.15 light | -0.05 edge | -0.01 edge | -0.04 edge |
| evening-garden | +0.16 light | +0.04 edge | +0.02 edge | +0.01 edge |
| moonlit-pond | +0.37 light | +0.07 light | +0.24 light | +0.18 light |
| cedar-shade | +0.27 light | -0.03 edge | +0.12 light | +0.09 light |
| ink-and-vermilion | +0.10 light | -0.08 light | -0.04 edge | -0.10 light |
| maple-afternoon | +0.18 light | +0.01 edge | +0.01 edge | +0.01 edge |
| petal-spring | +0.12 light | +0.06 edge | -0.03 edge | -0.03 edge |
| rainy-afternoon | +0.28 light | -0.05 edge | +0.13 light | +0.10 light |
| hillside-summer | +0.32 light | -0.04 edge | +0.16 light | +0.05 edge |
| lantern-dusk | +0.61 light | +0.33 light | +0.46 light | +0.22 light |
| pocket-moss | +0.17 light | +0.00 edge | +0.05 edge | -0.02 edge |

## The themes

| Id | Name | Kind | One line |
|---|---|---|---|
| `morning-mist` | Morning Mist | painted, garden, dawn | Cool dawn haze: blue-grey water, rose-gold first light, fog at the edges. |
| `summer-garden` | Summer Garden | painted, garden, noon | The default. Bright afternoon, sandy shallows, pink lotus, cream light, blue-violet shade. |
| `evening-garden` | Evening Garden | painted, garden, evening | Low golden sun, peach light, lavender shadows, long cast shadows over teal water. |
| `moonlit-pond` | Moonlit Pond | painted, garden, night | Blue-violet moonlight, never black. White koi, moonlit rim outlines, fireflies. |
| `cedar-shade` | Cedar Shade | painted | Heavy pine canopy on four corners, blue-teal water, bright pads, white water-lilies. |
| `maple-afternoon` | Maple Afternoon | painted | Autumn sun on clear teal water, olive pads, a few rust maple leaves. |
| `petal-spring` | Petal Spring | painted | Cherry blossom time: pink light, pastel water, lotus on most pads, 24 petals. |
| `rainy-afternoon` | Rainy Afternoon | painted | Overcast, darker water, rich greens, raindrop rings, hydrangea violet. |
| `ink-and-vermilion` | Ink and Vermilion | painted, locked | Ink wash on paper with bamboo and grain, 11 greys and greens; the koi are the only strong color. |
| `hillside-summer` | Hillside Summer | pixel 4, pixel-garden, noon | Cozy farm pond: plank edge, grass bank, clear teal water, 18 colors from Resurrect 64. |
| `lantern-dusk` | Lantern Dusk | pixel 6, pixel-garden, dusk | Strict 8 colors (SLSO8): dark blue water, dark pads, glowing flowers and koi, warm rims, fireflies. |
| `pocket-moss` | Pocket Moss | pixel 5 | Five soft greens like an old handheld screen, light dither. Only the koi keep their color. |

## Build order

Each step is usable on its own:

1. Theme files: resolve `extends`, map `[palette]` onto `Palette` with the new `shadow` and `outline` slots, `[theme] name`, aliases for the old names. Add `t`/`T`/`l`/`L`/`r`, the toast, `Poser::recolor()` and hot reload. The painted themes work with today's look (and their own colors).
2. Move the `[light]` and `[style]` frame literals into `Params` in `water.wgsl` and `Water::render` at once, and the paint literals into `paint()` arguments. Add a parity test: render one fixed frame (fixed seed, tick and splashes) of `summer-garden`, `moonlit-pond` and `lantern-dusk` on both backends and compare within 2/255 per channel. It skips the GPU half when no adapter is found.
3. Koi outlines in the pose pass (`koi.wgsl`, `Poser::pose`), `koi_tones`, and `warm`/`cool` in the koi painter.
4. `[scene]` painters in `paint()`: floor kinds, planks, varied rim stones, flower and foliage kinds, corners, petals.
5. The pixel pass: art grid, koi posed on the same grid and snapped, nearest upscale for both, dither in `paint()`, the palette lock table, dash glints. The three pixel themes and Ink and Vermilion's lock need this.
6. Weather: rain, mist, fireflies.

## Review decisions

Two reviews (calm and clarity, art and system design) went into this version. Accepted:

- The readability check now fails on any band, ignores hue, reports deuteranopia distance, catches a `koi_sumi` equal to a water band, and the preview puts the ogon over the shallows.
- `soft` outlines are stronger (0.8 toward `outline`). Evening Garden, Maple Afternoon and the pale themes use `dark`. Moonlit Pond uses `rim`, with a light `outline`.
- `wash` and mist stay off the koi.
- Maple Afternoon: rust flowers and leaves (L below 0.5), 7 petals, cream sand, olive-green pads, bluer water. Ink and Vermilion: flowers are paper white, so vermilion belongs to the fish.
- Evening Garden: more teal shallows, `warm` 0.25, no wash, cooler sand. Rainy Afternoon: water about 0.06 darker. Cedar Shade: blue-teal `mid`, lighter pads. Morning Mist: cool dawn, rose-gold light, wash 0.08, `time = "dawn"`. Petal Spring: pink light, lotus on most pads, 24 petals.
- Lantern Dusk: blue shallows, a moss floor so the pond reads blue, dark pads, gravel bank above the water, sumi `#544E68`, low dither.
- Cut `rice-paper-noon` and merged `farmhouse-pond` into `hillside-summer` (its planks and grass bank). Moved `ink-and-vermilion` to painted. The pixel lineup is Hillside Summer, Lantern Dusk and Pocket Moss.
- Dither strength 0.25, water band edges only.
- CPU backend in the design, the parity test, the palette lock table, outlines in the pose pass.
- `family`, `time` and the other file keys are not inherited. One root (`summer-garden`) instead of `base.toml` plus a copy. `shadow` required. Ramps moved to an offline command. Hidden `pixel.toml`. Unused knobs and enum values cut (`clay`, `grass`/`reeds`/`none` rims, `flower = "none"`). `[theme.overrides]` deferred. Ids match names. `system.md` renamed `prior-art.md`. `jade-morning` and `dusk-indigo` retired with aliases.
- Default caustics 0.6 at scale 1.2. Rim stones vary in size and spacing.

Rejected or changed:

- **Cap glints and caustics below `koi_white` lightness** (calm 17). It needs a clamp in both backends for 1 to 3 pixel specks that last one frame, and `glint_threshold` already makes them rare. The fish is a large moving shape. Revisit if it shows in the real renderer.
- **Deuteranopia as its own pass/fail rule** (calm 1). Folded in: the rule no longer counts hue at all, so it already holds for deuteranopes. The distance is reported, not judged.
- **Switch every `soft` theme to `dark`** (calm 2). Summer Garden and Cedar Shade keep `soft`, made stronger, because it keeps the default look softer and passes the check. Evening and Maple switched.
- **Ink and Vermilion with `bayer2` at 0.3** (calm 10). Superseded by moving it to painted with no dither.
- **Hillside `shallow = #0B8A8F`, `mid = #0B5E65`** (calm 11). Resurrect 64 has no darker teal for `deep`, and the navy `#323353` I tried turned the locked pond brown. The water stays `#0EAF9B`/`#0B8A8F`/`#0B5E65`. It is calmer because the two brightest aquas and the greys are out of `swatches`, caustics are 0.3, the floor is moss (sand patches snapped to orange blotches), and the lock is `scene`, so koi keep their own colors.
- **Hillside's bank was garish** (after the first build). The lime came from `lily_dark = #239063`, `lily_light = #91DB69` and `#1EBC73` in `swatches`. Pads and bank now use the darker Resurrect 64 greens `#165A4C` and `#239063`, and the lime greens are out of `swatches`. Muted sage greens (`#547E64`, `#92A984`) were tried first, but any sage in `swatches` pulled the locked water to flat grey-green.
- **Merge Petal Spring into Summer Garden** (calm 13, art P3 option). Given the pink-lit treatment instead. It is the one spring scene and reads differently at a glance.
- **Hillside stones in two tones** (art P2). Moot: Hillside has a plank rim now.
- **`palette_lock` on the dark pixel theme, "all"**: kept on Lantern Dusk, where the koi are drawn in the same 8 colors on purpose.
- **Lit sides face the sun** (after the scene painters). Stones, pebbles, pads and flowers were lit on the side away from the sun, while the water shading had it right. They are lit on the upper left now, as docs/STYLE.md says. The preview mock still has the inversion.
- **Moss floors had ochre patches** (after the pixel pass). The first floor patch mixed toward the ochre pebble color, which the lock snapped to bright orange blotches in Hillside's shallows. On a moss floor that patch is now darker moss (`lily_dark`), which calms Hillside and Lantern Dusk. A grey swatch (`#9BABB2`) to catch the orange ripple highlights was tried and dropped: it turned the planks and some shallows grey. The ripple rings stay orange, as in the preview.

## Decided with Jeremy

Each recommendation was accepted:

- **`l` steps through a family** (yes), rather than shifting light inside one theme. It needs no new code. A light curve can come later.
- **A user theme with a built-in's id replaces it** (yes), as in Ghostty and Helix.
- **Follow the clock**: off. `research/hud/SPEC.md` specifies it as `time.follow_clock`.
- **Crossfade on switch**: instant first. A crossfade needs two sets of statics on both backends.
