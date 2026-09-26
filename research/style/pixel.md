# Pixel style survey

How calm 2D and pixel-art games draw nature and water, and what that means for pixel themes in the koi pond. Inspiration only: take general techniques, never specific frames, characters or logos.

## Key takeaways

1. **Ghostty blurs our pixels today.** Ghostty samples Kitty images with a linear filter (`samplerOptions` in `src/renderer/OpenGL.zig`: `min_filter = .linear, mag_filter = .linear`). We send about 2 image pixels per cell width, so Ghostty stretches each one 4 to 5x with bilinear blending. That is part of why the current look reads soft and painted. A pixel theme cannot look crisp this way.
2. **Fix: do the upscale ourselves.** Render the art at low resolution, then upscale with nearest-neighbor on the GPU to the exact device-pixel size of the placement (`cols * cell_w` by `rows * cell_h`). At 1:1, Ghostty's linear filter changes nothing. Cost: about 6 MB per frame at 1600x945 RGBA, which shared memory (`t=s`) handles.
3. **Pixel size is a device-pixel setting, not a cells setting.** Pick `pixel_px` (screen pixels per art pixel, an integer such as 3 to 8), make the art grid `floor(W / pixel_px)` by `floor(H / pixel_px)`, and fill the leftover strip with bank color. Cells are not square, so tying art pixels to cells gives non-square or uneven pixels.
4. **One grid for everything.** Koi, ripples, petals and foliage must all snap to the same art-pixel grid. Mixed pixel sizes ("mixels") are the most common mistake in pixel games. Koi move and turn, so they need nearest sampling at art resolution (or pre-rotated directions), not a smooth overlay at screen resolution.
5. **Water reads from restraint.** Two or three flat depth bands, a few 1-pixel highlight dashes, and the brightest color in only a few pixels. Animate highlights in short loops (about 4 to 8 frames). This matches STYLE.md's flat bands and 1/8 s motion steps already.
6. **Outlines carry readability on bright water.** Our bright shallow water leaves red koi at only 1.1 to 1.4 contrast. Pixel games solve this with a colored outline one step darker than both the object and the water (Lospec's rule), and selective outlining: lighter on the sunlit side, darker away from the sun. A Short Hike adds a soft outline for the same reason.
7. **Hue-shift every ramp.** Shadows move toward blue or violet, lights toward warm yellow. This matches STYLE.md's warm-light, cool-shadow rule and is the core of the Ghibli-like feel at low color counts.
8. **Dither only where bands meet, and keep it static.** A 2x2 or 4x4 Bayer pattern anchored to the art grid is deterministic, so still frames stay identical. Paper grain from the painted themes should be off (or per art pixel) in pixel themes: sub-pixel noise fights the grid.
9. **Borrow proven palettes.** Apollo (46 colors, smooth hue shifting), Resurrect 64, SLSO8, Nyx8, Vinik24, Sweetie 16 and Endesga 32 from Lospec all have ramps that suit calm water. Credit the author in the theme file.
10. **A theme is palette plus a handful of look knobs.** Proposed knobs: `pixel_px`, `palette_lock`, `bands`, `dither`, `outline`, `grain`, `anim_step`, `highlight_style`. Six themes are proposed below.

## 1. Crisp pixels in the terminal

**What Ghostty does.** Kitty images are drawn as textured quads. On Linux Ghostty uses OpenGL, and the sampler for images is linear for both minification and magnification (Ghostty main, `src/renderer/OpenGL.zig`, `samplerOptions`; the image shader `src/renderer/shaders/glsl/image.f.glsl` is a plain `texture(image, tex_coord)`). There is no config option to switch to nearest. Installed version here is 1.3.1; confirm visually that it behaves the same as main.

**What that means.** Linear magnification by a factor `f` puts an `f`-pixel-wide color ramp at every texel edge. With `water_px = 2` and cells about 9 to 10 device pixels wide, `f` is about 4.5 and not an integer, so edges blur and also vary in width across the screen. Pixel art needs integer, nearest-neighbor scaling ([notkey.studio](https://notkey.studio/en/tutorials/choosing-the-right-render-resolution-for-a-pixel-art-game/), [Celeste 320x180 at 4x/6x/8x](https://studiominiboss.tumblr.com/post/625911527684390912/celestes-pixel-art-by-pedro-medeiros-brazilian)).

**Options, best first.**

| Option | How | Result |
|---|---|---|
| Present pass at 1:1 | Render at art resolution, then one more GPU pass that nearest-samples it up to exactly `cols*cell_w x rows*cell_h` and read that back. | Perfectly crisp. About 6 MB/frame at 1600x945, 48 MB/s at the calm 8 fps. Fine over shared memory. |
| Integer pre-upscale | Nearest-upscale by `k` so the remaining factor `f = pixel_px / k` is about 1 to 1.5. | Edges soften by 1 to 2 device pixels. Fewer bytes. A fallback if 1:1 is too heavy at 60 fps. |
| Sharp-bilinear in our own pass | Needed only if we ever scale by a non-integer ourselves. Clamp the fractional texel position near edges ([Pixel Art Filtering, jorenjoestar](https://jorenjoestar.github.io/post/pixel_art_filtering/), which compares the Quilez, Golus and CSantos formulas). | Not needed if `pixel_px` is an integer and we send 1:1. |

**Picking the grid.** Query the cell size in device pixels (`CSI 16 t`, already noted in research/rendering.md). Then `art_w = floor(cols*cell_w / pixel_px)`, `art_h = floor(rows*cell_h / pixel_px)`. The remainder (under `pixel_px` pixels on right and bottom) is filled with bank or foliage color. The water simulation can run on the art grid directly: at `pixel_px = 5` on a 1600-pixel-wide window the grid is 320 wide, the same as today's `water_px = 2` at 160 columns. So a pixel theme costs about the same to simulate; the difference is quantizing colors and presenting with nearest.

**Mixels.** All art must share one pixel grid; nothing should look scaled by 2x next to something at 1x ([Unity/Godot advice on mixels via itch.io thread](https://itch.io/post/8448610)). For koi this means drawing them into the art-resolution image (or a same-resolution layer), not at screen resolution. Rotated sprites at low resolution get jagged; RotSprite and Clean Rotate exist for pre-rendering clean rotations ([RotSprite thread](https://shmups.system11.org/viewtopic.php?f=9&t=32679), [Clean Rotate](https://astropulse.itch.io/clean-rotate-for-aseprite)). For procedural koi, sampling the SDF body at art-pixel centers with no anti-aliasing gives the same effect, and a 1-pixel outline hides most jaggies.

## 2. Prior art: games

| Game | What it does | What to take |
|---|---|---|
| **A Short Hike** (Adam Robinson-Yu) | 3D rendered at low resolution with point filtering, flat cohesive shading, no anti-aliasing, a soft outline on objects for readability, palette sampled from autumn photos. Players can change pixel size. ([PlayStation Blog](https://blog.playstation.com/2021/08/05/crafting-a-tiny-open-world-a-look-behind-the-scenes-at-the-creation-of-a-short-hike/)) | Closest model for us: procedural rendering, then pixelate. Expose `pixel_px` as a live setting. Soft outline for koi. |
| **Eastward** (Pixpil) | Pixel art over a 3D lighting system in a custom engine; fog layers and sunbeams at dawn and dusk; bump maps hand-painted per asset; LUTs to soften colors. Inspired by 90s Japanese animation. ([Game Developer](https://www.gamedeveloper.com/art/eastward-s-creators-share-insights-on-making-pixel-art-adventures), [80.lv](https://80.lv/articles/eastward-charming-chinese-pixel-art-adventure)) | Modern light on a pixel grid is fine if the light is quantized to the grid too. A final color LUT per theme is a cheap way to shift mood. |
| **Sea of Stars** (Sabotage) | Dynamic lighting that moves across pixel scenes, real-time reflections on water, layered rendering for depth. ([Sabotage press kit](https://sabotagestudio.com/presskits/sea-of-stars/), [Megavisions](https://www.megavisions.net/the-art-of-sea-of-stars-a-sea-of-pixels/)) | Cloud reflections and moving light can stay; draw them in art pixels and posterize them. |
| **Hyper Light Drifter** (Heart Machine) | Flat color base with large gradients or vignettes laid over it; a limited set of greys and greens lit by vivid pinks. Inspired by older anime. ([Game Developer](https://www.gamedeveloper.com/business/the-ultra-modern-stylings-of-hyper-light-drifter), [Wikipedia](https://en.wikipedia.org/wiki/Hyper_Light_Drifter)) | A soft full-screen gradient over flat pixel fills adds atmosphere without adding detail. Good for time of day. |
| **Celeste** (Pedro Medeiros) | 320x180 canvas chosen to scale by integers to common screens; 8x8 tiles. ([Miniboss](https://studiominiboss.tumblr.com/post/625911527684390912/celestes-pixel-art-by-pedro-medeiros-brazilian), [Aran P. Ink tileset breakdown](https://aran.ink/posts/celeste-tilesets), [Saint11 on consistency](https://saint11.art/blog/consistency/)) | Choose resolution by integer scale first. Consistency of pixel size and outline weight matters more than detail. |
| **Stardew Valley** (ConcernedApe) | 16x16 tiles; water is semi-transparent tiles over scrolling layers, with small sparkle highlights. ([forum](https://forums.stardewvalley.net/threads/sprite-size-first-time-modding-sv.7128/), [ConcernedApe interview](https://mentalnerd.com/blog/getting-started-pixel-art-interview/)) | Layered water: a floor layer, a tinted water layer, and a sparse sparkle layer. Our shader already has the same layers. |
| **Owlboy** (D-Pad Studio) | "Hi-bit" pixel art: modern resolution and color counts, many animation frames (8+ for swaying plants). ([Hundstrasse](https://hundstrasse.com/2017/02/25/owlboy-pixelart-elegance/), [D-Pad blog](https://dpadstudio.tumblr.com/post/697123665705402368/the-animations-of-owlboy-part-2-today-we)) | Pixel style does not require few colors. A "hi-bit" theme with `pixel_px = 3` and a 30 to 40 color palette is valid. |
| **Unpacking** (Witch Beam) | Clean, cozy pixel art in a warm pastel palette. ([Angus Doolan interview](https://www.muchopixels.com/post/angus-doolan-interview), [Disegno](https://disegnojournal.com/newsfeed/unpacking-cosy-video-games)) | Calm comes from warm pastels and clean shapes, not from low contrast everywhere. |
| **Kind Words** (Popcannibal) | Soft, muted colors so the scene is never busy; the art sits behind the activity. ([Wikipedia](https://en.wikipedia.org/wiki/Kind_Words_(video_game)), [Indie Hive](https://indie-hive.com/kind-words-2/)) | The pond is a background you leave open: keep motion slow and sparse. |
| **Terraria** (Re-Logic) | Tile water that fills partially and flows like a cellular automaton. ([Terraria wiki](https://terraria.wiki.gg/wiki/Liquid_duplication)) | Mostly side-view mechanics; little to borrow for a top-down pond beyond flat water tiles with a lighter surface line. |

## 3. Artists and palettes

**Artists.**
- **1041uuu (Toyoi Yuuta)**: quiet everyday Japanese scenes as looping GIFs (rain, falling leaves, steam), often in about 32 colors. He says pixel art must be drawn "in an easy-to-understand manner" to make up for its low resolution. That matches "intuitive and clear". ([Neocha](https://neocha.com/magazine/japan-pixelated/), [Tumblr](https://www.tumblr.com/1041uuu/tagged/pixel%20art))
- **Waneella**: animated scenes where light sources and shadow do most of the work. ([Retronator](https://medium.com/retronator-magazine/the-mysterious-world-of-waneella-281d5b52a68), [site](https://waneella.com/))
- **Raymond Schlitter (SLYNYRD)**: the best-documented tutorials on ramps and water in motion. ([Pixelblog 1: palettes](https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes), [Pixelblog 10: water](https://www.slynyrd.com/blog/2018/10/12/pixelblog-10-water-in-motion))

**Lospec palettes suited to calm water** (all hex values fetched from the palette pages):

| Palette | Author | Why | Water-relevant colors |
|---|---|---|---|
| [Apollo](https://lospec.com/palette-list/apollo) (46) | AdamCYounis | Smooth hue-shifted families: blues, greens, sands, a soft pink. | `#172038 #253a5e #3c5e8b #4f8fba #73bed3 #a4dddb`, greens `#25562e #468232 #75a743 #a8ca58`, sand `#d7b594 #e7d5b3` |
| [Resurrect 64](https://lospec.com/palette-list/resurrect-64) | Kerrie Lake | Teal ramp plus warm peach lights and violet shadows. | teal `#0b5e65 #0b8a8f #0eaf9b #30e1b9 #8ff8e2`, peach `#fca790 #fdcbb0`, shadow `#625565` |
| [SLSO8](https://lospec.com/palette-list/slso8) (8) | Luis Miguel Maldonado | One ramp from deep blue to warm cream. Instant dusk. | `#0d2b45 #203c56 #544e68 #8d697a #d08159 #ffaa5e #ffd4a3 #ffecd6` |
| [Nyx8](https://lospec.com/palette-list/nyx8) (8) | Javier Guerrero | Night blues with warm skin tones. | `#08141e #0f2a3f #20394f #4e495f #816271 #997577 #c3a38a #f6d6bd` |
| [Vinik24](https://lospec.com/palette-list/vinik24) | Vinik | Soft pastel ramps sharing one shadow and one highlight. | `#387080 #68aca9 #7ca1c0 #416aa3 #6eaa78 #93a167 #c38890` |
| [Sweetie 16](https://lospec.com/palette-list/sweetie-16) | GrafxKid | TIC-80 default; clear blues and greens. | `#29366f #3b5dc9 #41a6f6 #73eff7 #257179 #38b764 #a7f070` |
| [Endesga 32](https://lospec.com/palette-list/endesga-32) | ENDESGA | Widely used; strong but slightly saturated for "calm". | `#193c3e #124e89 #0099db #2ce8f5 #265c42 #3e8948 #63c74d` |
| [Island Joy 16](https://lospec.com/palette-list/island-joy-16) | Kerrie Lake | Tropical, bright; good for a playful theme. | `#1e8875 #11adc1 #6df7c1 #5bb361 #a1e55a #f48cb6` |

## 4. Techniques

**Water.**
- Keep most of the surface in two or three flat tones. Put the brightest color in only a few short strokes; restraint makes the highlights read ([Pixel-Editor 8-bit water](https://www.pixel-editor.com/articles/rendering-8-bit-pixel-water)).
- Sparkles loop in about 6 frames of simple linear motion; flow is shown with short pulsing lines, not whole-surface motion ([SLYNYRD water](https://www.slynyrd.com/blog/2018/10/12/pixelblog-10-water-in-motion)).
- Ripple rings: 1-pixel rings, light arc on the sun side and a darker arc on the far side, which is STYLE.md's rule drawn at pixel scale ([SqdPxl ripples](https://www.deviantart.com/sqdpxl/art/Pixel-tutorial-simple-water-ripples-669815989), [Lospec water tutorials](https://lospec.com/pixel-art-tutorials/tags/water)).
- Caustics: threshold the existing caustic field to two levels and draw only the brightest as 1-pixel lines. Shader pixel-water uses `smoothstep` with a very narrow width to make stepped light and specular bands ([Godot pixel water shader](https://godotshaders.com/shader/pixel-art-water-shader/)).
- Snap every UV and every animated offset to the art grid before sampling, so nothing drifts by half a pixel.

**Outlines.**
- An outline must raise contrast: darker than both the object and what is behind it. Use a darker shade of the object's own hue, not black ([Lospec, Outlines part 2](https://lospec.com/articles/pixel-art-outlines-part-2-using-color/)).
- Selective outline (sel-out): full dark outline, then replace it with a lighter body shade on the lit side (upper left for us) ([Pixnote](https://pixnote.net/en/learn/outlines/), [PixelJoint challenge](http://pixeljoint.com/2007/10/15/2346/Pixel_Art_Challenge-_Selective_Outlining.htm)).
- For us: koi always get an outline in pixel themes. Lily pads get a sel-out. Stones get only a bottom-right shadow edge. Water ripples never get outlines.

**Hue shifting.** A good ramp shifts hue as it darkens and lightens, not only brightness; straight ramps look flat and clash with other ramps. SLYNYRD uses about 20 degrees of hue shift per step and an "X pattern" where darker steps get more saturated ([SLYNYRD palettes](https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes), [Pixel-Editor color theory](https://www.pixel-editor.com/articles/color-theory-for-pixel-art), [color-ramps tool](https://github.com/depuschm/color-ramps)). For the pond, water ramps drift from blue (deep) to green-teal (shallow) to warm cream (highlight), and foliage shadows lean blue-green.

**Dithering.**
- Ordered (Bayer) dithering adds a fixed threshold per pixel before choosing the nearest palette color. It is deterministic and runs in a shader; Return of the Obra Dinn uses a 4x4 Bayer matrix this way ([Ditherpunk](https://surma.dev/things/ditherpunk/), [Alex Charlton, Dithering on the GPU](https://alex-charlton.com/posts/Dithering_on_the_GPU/), [Godot palette dithering shader](https://godotshaders.com/shader/arbitrary-color-reduction-ordered-dithering/)).
- Checkerboard dithering between two colors makes a third tone; use it sparingly in pixel art, mostly for large gradients ([drububu](https://drububu.com/tutorial/pixel-art-and-dithering.html)).
- For us: dither only across the band edges and the shore falloff, 2 to 3 art pixels wide. Anchor the matrix to art-pixel coordinates. Never temporal dithering (research/rendering.md already rules it out).

**Pixel scale.** Lowest resolution that still says what you need ([notkey.studio](https://notkey.studio/en/tutorials/choosing-the-right-render-resolution-for-a-pixel-art-game/)). For a koi about 60 art pixels long at `pixel_px = 3`, pattern patches and fins read well. At `pixel_px = 6` a koi is about 30 pixels: still clearly a kohaku, but patterns are blobs. Above 8 the fish become icons.

## 5. Look knobs for the theme system

These are the settings the pixel themes below use. Each is independent, so themes mix them freely.

| Knob | Values | Effect |
|---|---|---|
| `pixel_px` | 1 (off, current look) or integer 2 to 8 | Device pixels per art pixel. 1 keeps today's painted look. |
| `palette_lock` | `off`, `on` | Quantize every final pixel to the theme's palette list (not only the 21 slots). |
| `bands` | 2 to 5 | Flat water depth bands. |
| `floor_tones` | 2 to 4 | Posterize steps for floor light. STYLE.md uses 3. |
| `dither` | `none`, `bayer2`, `bayer4` | Only at band and shore edges. |
| `dither_width` | art pixels | Width of the dithered zone. |
| `outline` | `none`, `dark`, `selout` | Koi and pads. |
| `outline_color` | hex | Default darkest ramp color; per-object shade when `selout`. |
| `grain` | 0 to 0.05 | Paper grain. Pixel themes use 0, or grain sampled per art pixel. |
| `anim_step` | seconds | STYLE.md uses 1/8. Pixel themes may use 1/6 or 1/4 for a chunkier, calmer loop. |
| `highlight_style` | `soft`, `dashes` | Soft glints with halo, or 1-pixel dashes. |
| `overlay_gradient` | two hex + angle + strength | HLD-style soft wash for time of day. |

## 6. Proposed pixel themes

All six follow STYLE.md's light direction (sun upper left, warm light, cool shadow). Slots not listed (turtle, ui) inherit from summer-garden. The contrast column is WCAG ratio versus `mid` water; low red numbers are expected on bright water and are why these themes all outline the koi (outline versus shallow is 5.5 or better in every theme).

| Theme | Mood | `pixel_px` | bands | dither | outline | anim_step | white/red/ogon vs mid |
|---|---|---|---|---|---|---|---|
| moss-pond | Clear blue garden pond, hi-bit | 3 | 3 | bayer2 | selout | 1/8 | 3.0 / 1.2 / 2.1 |
| hillside-summer | Teal, peach light, violet shadow | 4 | 3 | none | selout | 1/8 | 4.2 / 1.1 / 2.5 |
| rice-paper-noon | Pixel twin of summer-garden | 3 | 3 | none | selout | 1/8 | 2.5 / 1.4 / 1.5 |
| rain-garden | Overcast, soft greens, rain rings | 4 | 3 | bayer2 | dark | 1/6 | 3.4 / 1.1 / 1.9 |
| lantern-dusk | 8 colors, deep evening | 6 | 3 | bayer4 | dark (light-on-dark) | 1/4 | 9.9 / 3.8 / 6.1 |
| ink-vermilion | Sumi-e on paper, very pixelated | 6 | 2 | bayer4 | dark | 1/4 | 1.5 / 2.4 / 1.3 |

### moss-pond (from Apollo, AdamCYounis)

`palette_lock = on` with the full Apollo list; `grain = 0`; `highlight_style = dashes`.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#3c5e8b` | lily_dark | `#25562e` |
| mid | `#4f8fba` | lily_light | `#75a743` |
| shallow | `#73bed3` | lily_flower | `#df84a5` |
| highlight | `#ebede9` | stone_dark | `#577277` |
| koi_white | `#ebede9` | stone_light | `#e7d5b3` |
| koi_red | `#cf573c` | food | `#be772b` |
| koi_sumi | `#151d28` | asagi_blue | `#819796` |
| ogon | `#e8c170` | asagi_red | `#da863e` |
| outline | `#172038` | | |

### hillside-summer (from Resurrect 64, Kerrie Lake)

Warm peach sunlight on teal, violet-grey shadows. Closest to the Ghibli "warm light, cool shadow" feel. `palette_lock = on`.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#0b5e65` | lily_dark | `#239063` |
| mid | `#0b8a8f` | lily_light | `#91db69` |
| shallow | `#0eaf9b` | lily_flower | `#ed8099` |
| highlight | `#fdcbb0` | stone_dark | `#625565` |
| koi_white | `#ffffff` | stone_light | `#fdcbb0` |
| koi_red | `#ea4f36` | food | `#cd683d` |
| koi_sumi | `#2e222f` | asagi_blue | `#9babb2` |
| ogon | `#f9c22b` | asagi_red | `#f57d4a` |
| outline | `#2e222f` | | |

### rice-paper-noon (authored)

The default summer-garden look, pixelated. Hue-shifted by hand: water blue to green to cream, stone shadow toward violet. `palette_lock = off` (slots only), `grain = 0`.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#2f7f98` | lily_dark | `#3f7f4a` |
| mid | `#4fae9f` | lily_light | `#a3cf6e` |
| shallow | `#8fd6c2` | lily_flower | `#f3a9bd` |
| highlight | `#fff4d6` | stone_dark | `#7a7f8f` |
| koi_white | `#fbf5e8` | stone_light | `#eadcb4` |
| koi_red | `#e4583a` | food | `#c9924a` |
| koi_sumi | `#2b2733` | asagi_blue | `#6f90b4` |
| ogon | `#f0b843` | asagi_red | `#e07d4f` |
| outline | `#26414a` | | |

### rain-garden (authored)

Grey-green overcast light, no hard sun shadows, clouds off, rain drops as 1-pixel ring ripples. `overlay_gradient` cool grey at low strength.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#3d6470` | lily_dark | `#3c5e43` |
| mid | `#5b8a86` | lily_light | `#7fa065` |
| shallow | `#8fb3a3` | lily_flower | `#e3b3c0` |
| highlight | `#dfe8dc` | stone_dark | `#5f6670` |
| koi_white | `#f1efe6` | stone_light | `#b9b8a4` |
| koi_red | `#df5f45` | food | `#b0874f` |
| koi_sumi | `#23262d` | asagi_blue | `#7f97ad` |
| ogon | `#e0b04e` | asagi_red | `#d58158` |
| outline | `#23313a` | | |

### lantern-dusk (SLSO8, Luis Miguel Maldonado)

Strict 8 colors, `palette_lock = on`. Water is very dark, so koi glow against it and the outline is inverted: a warm light rim on the sun side. Lily pads are violet, not green. Sumi patches merge into deep water, which research/palette.md accepts as realistic.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#0d2b45` | lily_dark | `#544e68` |
| mid | `#203c56` | lily_light | `#8d697a` |
| shallow | `#544e68` | lily_flower | `#ffaa5e` |
| highlight | `#ffd4a3` | stone_dark | `#203c56` |
| koi_white | `#ffecd6` | stone_light | `#8d697a` |
| koi_red | `#d08159` | food | `#ffaa5e` |
| koi_sumi | `#0d2b45` | asagi_blue | `#8d697a` |
| ogon | `#ffaa5e` | asagi_red | `#d08159` |
| outline | `#ffd4a3` (rim) | | |

### ink-vermilion (authored)

Sumi-e: pale paper water, ink-grey stones and pads, vermilion and gold as the only saturated colors. About 10 colors, `palette_lock = on`, big pixels, Bayer 4x4 for the ink wash at the shore. The koi red is the brightest thing in chroma, not in lightness, so the dark outline does the work.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#8fb0aa` | lily_dark | `#5d6f5c` |
| mid | `#b4ccc2` | lily_light | `#8a9a80` |
| shallow | `#d9e0d0` | lily_flower | `#d4553a` |
| highlight | `#f7f1e1` | stone_dark | `#3b3a40` |
| koi_white | `#f7f1e1` | stone_light | `#efe6d2` |
| koi_red | `#d4553a` | food | `#d9a441` |
| koi_sumi | `#2a292e` | asagi_blue | `#6d7f8c` |
| ogon | `#d9a441` | asagi_red | `#d4553a` |
| outline | `#2a292e` | | |

## Open questions

- Is 6 MB per frame at 60 fps acceptable over shared memory while koi move? If not, send at `k = pixel_px - 1` and accept a 1 to 2 pixel soft edge only while the fish move fast.
- Should `pixel_px` be a live key (like A Short Hike's pixel-size option) independent of theme? It is cheap if the grid is rebuilt on change, as on resize.
- Kitty terminal filtering was not checked; only Ghostty's source was read.

## Sources

- Ghostty source: [OpenGL.zig samplerOptions](https://github.com/ghostty-org/ghostty/blob/main/src/renderer/OpenGL.zig), [image.f.glsl](https://github.com/ghostty-org/ghostty/blob/main/src/renderer/shaders/glsl/image.f.glsl)
- [A Short Hike, PlayStation Blog](https://blog.playstation.com/2021/08/05/crafting-a-tiny-open-world-a-look-behind-the-scenes-at-the-creation-of-a-short-hike/)
- [Eastward, Game Developer](https://www.gamedeveloper.com/art/eastward-s-creators-share-insights-on-making-pixel-art-adventures), [80.lv](https://80.lv/articles/eastward-charming-chinese-pixel-art-adventure)
- [Sea of Stars press kit](https://sabotagestudio.com/presskits/sea-of-stars/), [Megavisions](https://www.megavisions.net/the-art-of-sea-of-stars-a-sea-of-pixels/)
- [Hyper Light Drifter, Game Developer](https://www.gamedeveloper.com/business/the-ultra-modern-stylings-of-hyper-light-drifter)
- [Celeste pixel art, Miniboss](https://studiominiboss.tumblr.com/post/625911527684390912/celestes-pixel-art-by-pedro-medeiros-brazilian), [Aran P. Ink](https://aran.ink/posts/celeste-tilesets), [Saint11](https://saint11.art/blog/consistency/)
- [Stardew forum on sprite sizes](https://forums.stardewvalley.net/threads/sprite-size-first-time-modding-sv.7128/), [ConcernedApe interview](https://mentalnerd.com/blog/getting-started-pixel-art-interview/)
- [Owlboy, Hundstrasse](https://hundstrasse.com/2017/02/25/owlboy-pixelart-elegance/)
- [Unpacking, Mucho Pixels](https://www.muchopixels.com/post/angus-doolan-interview)
- [Kind Words, Wikipedia](https://en.wikipedia.org/wiki/Kind_Words_(video_game))
- [Terraria liquids wiki](https://terraria.wiki.gg/wiki/Liquid_duplication)
- [1041uuu, Neocha](https://neocha.com/magazine/japan-pixelated/), [Waneella, Retronator](https://medium.com/retronator-magazine/the-mysterious-world-of-waneella-281d5b52a68)
- [SLYNYRD palettes](https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes), [SLYNYRD water](https://www.slynyrd.com/blog/2018/10/12/pixelblog-10-water-in-motion)
- [Lospec outlines part 2](https://lospec.com/articles/pixel-art-outlines-part-2-using-color/), [Pixnote outlines](https://pixnote.net/en/learn/outlines/)
- [Pixel-Editor 8-bit water](https://www.pixel-editor.com/articles/rendering-8-bit-pixel-water), [Pixel-Editor color theory](https://www.pixel-editor.com/articles/color-theory-for-pixel-art)
- [Godot pixel water shader](https://godotshaders.com/shader/pixel-art-water-shader/), [Godot palette dithering shader](https://godotshaders.com/shader/arbitrary-color-reduction-ordered-dithering/)
- [Ditherpunk](https://surma.dev/things/ditherpunk/), [Dithering on the GPU](https://alex-charlton.com/posts/Dithering_on_the_GPU/), [drububu dithering](https://drububu.com/tutorial/pixel-art-and-dithering.html)
- [notkey.studio render resolution](https://notkey.studio/en/tutorials/choosing-the-right-render-resolution-for-a-pixel-art-game/), [Pixel art filtering](https://jorenjoestar.github.io/post/pixel_art_filtering/)
- [RotSprite](https://shmups.system11.org/viewtopic.php?f=9&t=32679), [Clean Rotate](https://astropulse.itch.io/clean-rotate-for-aseprite)
- Lospec palettes: [Apollo](https://lospec.com/palette-list/apollo), [Resurrect 64](https://lospec.com/palette-list/resurrect-64), [SLSO8](https://lospec.com/palette-list/slso8), [Nyx8](https://lospec.com/palette-list/nyx8), [Vinik24](https://lospec.com/palette-list/vinik24), [Sweetie 16](https://lospec.com/palette-list/sweetie-16), [Endesga 32](https://lospec.com/palette-list/endesga-32), [Island Joy 16](https://lospec.com/palette-list/island-joy-16)
