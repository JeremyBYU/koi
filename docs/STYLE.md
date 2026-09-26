# Style

The pond should look like a hand-painted animation background: a garden pond on a bright summer afternoon. It should be warm, sunlit and calm, and read as painted, not photographed. Themes change the colors, the light and the drawing style, never the pond itself.

| Summer Garden (default) | Evening Garden |
|---|---|
| ![Summer Garden](img/summer-garden.jpg) | ![Evening Garden](img/evening-garden.jpg) |

| Morning Mist | Moonlit Pond |
|---|---|
| ![Morning Mist](img/morning-mist.jpg) | ![Moonlit Pond](img/moonlit-pond.jpg) |

All four are the same pond (`pond.seed = 7`) in the four times of the garden family, from a 160x45 Ghostty window with the default `water_px = 2`. Morning Mist has its mist and Moonlit Pond its fireflies.

| Hillside Summer | Lantern Dusk | Pocket Moss |
|---|---|---|
| ![Hillside Summer](img/hillside-summer.png) | ![Lantern Dusk](img/lantern-dusk.png) | ![Pocket Moss](img/pocket-moss.png) |

The three pixel themes, from the same window and seed, at half size: two image pixels to an art pixel.

## Rules

**Light.** The sun comes from the upper left, and every shadow falls down and to the right: koi, lily pads, stones, flowers and leaves. `light.shadow_len` stretches the shadows (Evening Garden's are 1.7 times as long) and `light.diffuse` fades them for overcast themes. The sunlit side of a shape is warm (mixed toward `highlight`). The shaded side is cool (mixed toward the hue of `shadow`). In Evening Garden this gives orange-pink light and lavender shadows.

**Water.** Clear and luminous where it is shallow, so the sandy floor shows through with a turquoise tint. It deepens to a soft teal-blue oval in the middle, never near black. Depth is painted in three flat bands with short soft edges between them, not a smooth gradient.

**Pond floor.** Sand in ochre and cream with sage patches, and rounded pebbles that thin out toward the deep middle. Each pebble has a lit side, a shaded side and a small cast shadow.

**Painterly shading.** Light on the floor is posterized into three tones. A static paper and brush grain (a few percent of brightness) sits over everything, including the depth bands and the tone edges on stones and pads, so edges wobble a little like brushwork. It never animates, so still frames stay identical.

**Sunlight in the water.** A soft network of caustics moves slowly across the floor. Ripples focus light too (the analytic caustic from the height field). Ripple rings are drawn as a light line on the side facing the sun and a slightly darker line on the far side, like hand-drawn ripples. The brightest crests get a glint with a soft halo.

**Surroundings.** Rim stones of uneven size sit on the waterline in small clusters with the odd gap, warm cream on the side facing the sun and cool blue-grey on the far side, with moss patches (`scene.moss`). Hillside Summer has a plank boardwalk instead (`rim = "planks"`). The bank beyond is moss, grass or gravel (`scene.bank`). Two or three clusters of lily pads in several greens sit near the shore, some with a cupped pink or white lotus or a flat star-shaped water lily (`scene.flower`, `scene.flowers`). Foliage overhangs the corners named in `scene.foliage`, the first one largest, as broad leaves, maple, cherry with blossom, pine needles or bamboo (`scene.foliage_kind`). It casts slowly swaying dappled shadows into the water.

**Pond floor kinds.** `scene.floor` picks sand, pebbles (greyer and denser), moss (a green floor, used by the pixel themes so the water reads blue or teal) or slate (streaked grey). `scene.pebbles` scales how many pebbles there are.

**Surface life.** White summer clouds drift slowly across the water as faint reflections. Up to 24 petals and small leaves drift across the surface (`scene.petals`), picked from `scene.petal_kinds`: blossom, white, leaf or maple leaf.

**Motion.** Everything that moves on its own (clouds, leaf dapple, caustics, petals) is slow, and it advances in steps of 1/`style.anim_hz` s (1/8 s by default). Between steps the water image is identical, so the adaptive frame rate can send nothing, and at the calm 8 fps nothing is skipped.

**Weather.** `scene.weather` adds one of three kinds, scaled by `scene.weather_amount`. Rain drops about 11 small splashes a second at full strength into the height field, so its rings are real ripples that koi swim through. Mist mixes the water image toward `highlight` along the window edges, with slow drifting noise, and leaves the koi clear. Fireflies are up to 16 warm points (between `ogon` and `highlight`) that hover near fixed spots, wander a little and pulse. All three are gentle and cost almost nothing per frame.

**Koi** (drawn by `crates/koi-render/src/koi.rs`, listed here for the whole look): clean cel-shaded shapes in `style.koi_tones` tones, vermilion, cream, sumi and gold patterns, and long translucent fins. The lit ridge of the back mixes toward `highlight` by `light.warm`, the flanks and tail toward `shadow` by `light.cool`. The outline is drawn when each koi is posed, not in its texture, so the sun side stays toward the sun as the fish turns. `style.outline` picks it:

| Mode | Edge |
|---|---|
| `none` | No line. |
| `soft` | Anti-aliased, `outline` mixed with a little body and `mid`. The painted default. |
| `dark` | Solid `outline`. |
| `selout` | Body mixed toward `outline` on the sun side, solid `outline` on the far side. Pixel themes. |
| `rim` | A light `outline` rim on the sun side, the body darkened toward `shadow` on the far side. Moonlit Pond. |

The line runs round the body and the fin roots. The translucent fins have none. Their shadows on the floor are soft and offset down and right, and they darken the water column above the floor too, so they show over deep water. A koi that follows sinking seeds dives: it fades toward `deep` by up to 0.3, and its shadow moves in under it and sharpens.

## How the palette slots are used

Every theme resolves the same 22 color slots. The first 14 are required, the other 8 are derived from them when a theme leaves them out (the rules are in docs/THEMES.md). The water renderer blends them in linear light.

| Slot | Used for |
|---|---|
| `deep`, `mid`, `shallow` | Water color by depth band. `shallow` also tints the floor seen through the water. |
| `highlight` | Sunlight: lit sides, caustics, ripple lines, glints. |
| `shadow` | Shade: cast shadows, the cool side of stones, unlit floor. Its hue at full brightness is the shade tint. |
| `stone_light`, `stone_dark` | Rim stones (warm top, cool underside), sand, pebbles. |
| `lily_dark`, `lily_light` | Pads, moss, the bank, foliage. |
| `lily_flower`, `koi_white`, `ogon` | Lotus and water lily petals and centers, drifting petals, maple and cherry foliage. |
| `koi_white`, `koi_red`, `koi_sumi`, `ogon` | Koi. |
| `outline` (derived) | Koi outlines, the koi's eyes, and the dark edge on stones and pads in `dark` and `selout` themes. |
| `cloud` (derived) | Cloud reflections. |
| `asagi_blue`, `asagi_red` (derived) | The Asagi koi. |
| `food` (derived) | Ochre in the sand and pebbles, plank wood, and the food pellets. |
| `ui_text`, `ui_dim`, `ui_accent` (derived) | The top line, and the HUD's text, stones and volume dots. |

`swatches` lists the colors `style.palette_lock` snaps to. With no `swatches`, every slot color is one.

## Themes

Each theme is one TOML file in `themes/`, compiled into the binary. `themes/summer-garden.toml` is the root: it lists every key with its default and a comment, and every other theme lists only what it changes. docs/THEMES.md describes every key, the merge rules and how to keep koi readable. `koi --list-themes` lists them.

| Id | Family and time | Look |
|---|---|---|
| `morning-mist` | garden, dawn | Cool dawn haze: blue-grey water, rose-gold first light. |
| `summer-garden` | garden, noon | The default. Bright afternoon, sandy shallows, pink lotus. |
| `evening-garden` | garden, evening | Low golden sun, lavender shadows, long cast shadows. |
| `moonlit-pond` | garden, night | Blue-violet moonlight, never black. |
| `cedar-shade` | its own | Heavy canopy, blue-teal water, bright pads. |
| `maple-afternoon` | its own | Autumn sun, olive pads, rust leaves. |
| `petal-spring` | its own | Pink light, pastel water. |
| `rainy-afternoon` | its own | Overcast: no cast shadows or caustics, darker water. |
| `ink-and-vermilion` | its own | Ink wash on paper; the koi are the only strong color. |
| `hillside-summer` | pixel-garden, noon | Pixel farm pond, Resurrect 64 colors. |
| `lantern-dusk` | pixel-garden, dusk | Pixel dusk in the eight SLSO8 colors. |
| `pocket-moss` | its own | Five handheld greens. |

`t` and `T` step to the next and previous family, keeping the time of day when the family has it. `l` and `L` step to a later or earlier time in the same family. The switch is instant and keeps the koi, the waves and the pond layout. The HUD peeks the scene stone with the new theme's name, or the time stone with the new time. The pond remembers the last theme, the volume and mute between runs.

Pick the starting theme with `[theme] name = "..."` in config.toml or `koi --theme NAME`.

Your own themes go in `~/.config/koi-pond/themes/<id>.toml`. A file with a built-in's id replaces it. This is enough for a new theme:

```toml
name = "My Garden"
[palette]
koi_red = "#EE6343"
```

It extends `summer-garden`. `extends = "evening-garden"` starts from another theme. Tables merge key by key and lists replace whole. `name`, `family`, `time`, `description`, `credit` and `hidden` are never inherited, and neither are derived palette slots, so a child derives them again from its own colors. An unknown key is a warning. A bad color, a wrong type, a missing parent or an `extends` cycle keeps the current theme and shows the error on the top line for 6 s. Theme files and config.toml are checked once a second and reload on change; `r` reloads at once.

## Pixel themes

`style.pixel_px` above 0 makes a pixel theme: one art pixel is that many screen pixels (4 to 8). The water is rendered at the window's size divided by `pixel_px`, and the koi are posed on the same grid: hard-edged bodies, the nearest texel, no anti-aliasing, and each koi placed on a multiple of `pixel_px`, so fish and water never mix pixel sizes. Ghostty scales every image with a linear filter, so the game scales the water and each koi up itself, by repeating pixels, and sends them at exactly the screen size. The food sprites are drawn on the art grid too.

Water depth bands have hard edges. `style.dither` (`bayer2` or `bayer4`) nudges the depth by an ordered dither before banding, so where two bands meet there is a thin checkered seam; stones and pads are never dithered. `highlight_style = "dashes"` draws glints as short one-pixel strokes, 2 to 4 long, where a crest tilts toward the sun past `glint_threshold`. `style.palette_lock` snaps the water image (`scene`) or the water and the koi (`all`) to `palette.swatches`.

The simulation keeps its own grid, so switching between a painted and a pixel theme, or between pixel sizes, keeps the koi where they are and carries the waves over to the new grid.

## How it is built

`crates/koi-render/src/water.rs` paints everything that never moves once per theme, on the CPU, in bands of rows on every core: the floor and its depth, static sunlight (shore shading, pad shadows on the floor), foliage cover, paper grain, the wave damping map, and the surface layer (bank, rim, pads, flowers, foliage). It also paints one tiling 256x256 texture holding clouds, leaf dapple and caustic cells. Both backends use these same arrays, and only the per-frame shading is written twice, in `water.rs` (CPU) and `water.wgsl` (GPU). A paint takes about 5 ms for a 400x225 pond and 70 ms for 1920x1080 (a 4K window at `water_px = 2`) on a 48-thread machine.

The pond's shape, the rim stones and the lily pads come from `Layout::new(w, h, seed)`, which does not see the theme, so a theme switch cannot move them. A planks theme hides the stones and `pad_size` scales each pad around its own center. Each foliage corner has its own random stream, so a corner looks the same in every theme that names it. Petal k starts and drifts the same way in every theme.

Koi outlines are drawn in the pose pass (`koi.wgsl` and its CPU twin in `Poser::pose`). The koi painter marks the edge texels of each body, and the pose pass colors them by mode, with the sun direction turned into the fish's frame.

The theme reaches the renderer in two ways. `paint()` takes the whole `Theme` and reads the palette, `[light]` (sun, warm, cool, shadow_len, diffuse) and the paint-time `[style]` keys (tone_steps, depth_bands, band_softness, grain, dither, dither_strength). The per-frame `[light]` and `[style]` numbers (sun, ambient, shadow_len, diffuse, tone_steps, band_softness, grain, caustics, caustic_scale, caustic_softness, glint, glint_threshold, bloom, cloud_reflections, leaf_shadows, wash, anim_hz, highlight_style, the palette lock, and the weather's mist and fireflies) travel in the `Params` uniform, and the CPU path reads the same struct. The palette lock is a 32x32x32 table built once per theme that maps every color, at 5 bits per channel, to its nearest swatch in OKLab. Both backends look each pixel up in the same table, so they snap alike. The parity test (`crates/koi-render/tests/parity.rs`) renders one frame of eight themes on both backends and fails if any channel differs by more than 2/255. They cover the soft, dark, rim and selout outlines, sand, moss, planks, water lilies, grass, mist, rain and fireflies, and the three pixel themes, which also move the water to the art grid mid-way.

The surface layer is part of the water image, so koi currently swim over lily pads and stones rather than under them.
