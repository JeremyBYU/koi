# Hand-painted anime backgrounds as a style reference

A survey of how Japanese animation background painters (Studio Ghibli's in particular) paint light, water and mood, turned into rules and palettes the pond renderer can use. The goal is the general look, not copies of any film's frames.

## Key takeaways

1. **Few paints, many mixes.** Kazuo Oga worked from about 21 jars of cheap poster color. A theme should be a small set of base colors (the existing 21 slots are about right) plus rules for mixing them, not a big list of one-off colors.
2. **Soft, happy colors, never raw primaries.** Miyazaki asked Oga for colors "that make you feel relieved when you see them." Big areas (water, floor, bank) stay at low to medium chroma. The strongest chroma goes on small things: koi, flowers, a lantern.
3. **Shadows have color.** Ghibli's color designer Michiyo Yasuda: "there is color even in shadows." Every theme needs one explicit shadow color. Shade by mixing toward it, never by multiplying toward black.
4. **Shadow color comes from the environment.** Makoto Shinkai tints shadows green under trees and blue under open sky. For the pond: blue-violet in open sun, green-teal under foliage, violet at dusk, deep blue at night.
5. **Separate light from shadow first, then add midtones.** Depth comes from a clear light/shadow split. Three value groups per material (light, base, shadow), plus one small accent highlight. Four steps is the ceiling.
6. **Big soft wash first, crisp details last.** Painters lay the base coat wet-into-wet in 30 to 60 minutes, then add sharp details with a pointed brush. In shader terms: soft edges on large fields (depth bands, sky reflection, dapple), hard edges on small near things (pad rims, ripple lines, stone tops, koi).
7. **Water is never one flat color.** Shore water gets a different hue from open water. Reflections are darker and less saturated than what they reflect. Sparkle is a few small glints, not a sheen over everything.
8. **Time and weather are told by light, not by redrawing.** The same scene changes mood through the light color, shadow color, contrast and saturation. That maps directly onto a palette swap plus a few look knobs.
9. **Abstract to what a person notices.** Group leaves into clumps, rain into streaks, light into a few shapes. Detail goes where the eye goes (the koi), and everywhere else is simplified.
10. **Air and transparency.** Yasuda aimed for colors where you can "sense the air." Muted is fine, murky is not: keep the darkest water well above black in day themes.

## The painters' method

### Materials

Ghibli backgrounds were painted in Nicker poster color: cheap, opaque, water-based paint closer to gouache than watercolor, bought in jars for school use. Oga said: "Poster colors can show brightness or depth of color and, above all, it is easy-to-use" ([Gurney Journey](http://gurneyjourney.blogspot.com/2017/03/demo-by-kazuo-oga.html)). He had around 21 colors, such as bleu celeste and olive green, and said "with a small number of colors, you can create a wide range of colors through subtle mixing" ([Animation Obsessive](https://animationobsessive.substack.com/p/what-kazuo-oga-thinks-about-when)). A listed Nicker set includes carmine, scarlet lake, cobalt violet, opera, cobalt blue, cerulean, prussian blue, viridian, chrome green, light green, chrome yellow, lemon yellow, yellow ochre, burnt sienna, French grey, black and white ([Mleczny Mlecz](https://www.mlecznymlecz.com/how-to-paint-like-ghibli-ghibli-color-analysis/)). Ponyo's art director Noboru Yoshida used poster color plus colored pencil ([Mike Blake](http://monisawa.blogspot.com/2010/04/ponyo-kazuo-oga-and-poster-paint.html)).

Takeaway for us: a theme is a small base palette and the mixes between them. Adding a theme should mean choosing about a dozen colors, not a hundred.

### Order of work and edges

- Wet both sides of the paper, drop paint into the wet surface and blend while wet. Large soft passages first, details last ([Gurney, Oga demo](http://gurneyjourney.blogspot.com/2017/03/demo-by-kazuo-oga.html)).
- The base coat (ji-nuri) must be finished before the paper dries: "30 minutes to 1 hour." A weak base cannot be saved by later detail ([Animation Obsessive](https://animationobsessive.substack.com/p/what-kazuo-oga-thinks-about-when)).
- Paint is used nearly full-bodied, wet into wet, without much water or white, which gives an even surface with subtle transitions ([Gurney, Painting in the Ghibli Style](https://gurneyjourney.blogspot.com/2018/05/painting-in-ghibli-style.html)).
- For skies, four colors are premixed: a light and a dark blue for the gradient, a white and a grey for lit and shaded cloud. The paper is pre-wet for soft cloud edges (same source).
- Two brushes: a flat brush and a pointed sakuyo brush.

Takeaway: two edge regimes. Soft (wet-into-wet) for fields and atmosphere. Hard (dry brush, pointed) for small foreground marks. Mixing them in one element looks wrong; keeping them apart looks painted.

### Light, shadow and value

- Separate shadow from light early; that separation is what makes depth. Darker areas recede, bright shapes come forward. Cast shadows go in early because they anchor things ([GVAAT](https://gvaat.com/blog/how-to-paint-ghibli-backgrounds/)).
- In anime style, form shadow (陰) and cast shadow (影) are usually painted with the same color ([Clip Studio Tips, via search summary](https://tips.clip-studio.com/en-us/articles/2964)). One shadow color per theme covers both.
- Yasuda: "there is color even in shadows," so deep or vivid colors can replace grey and black ([Animation Obsessive, The Artist Who Colored Ghibli](https://animationobsessive.substack.com/p/the-artist-who-colored-ghibli)). She also used brown instead of black for outlines in Grave of the Fireflies to soften the look ([Wikipedia](https://en.wikipedia.org/wiki/Michiyo_Yasuda)). This backs the current rule that koi outlines are darker body color, not black.
- Shinkai: "If a character is in a green forest below a blue sky, I'll adjust the color of the character's shadow to be green or blue" ([summary of his interviews](https://anifusion.ai/style/makoto-shinkai-style/)).
- Miyazaki makes highlights with thin, diluted paint rather than white ([Animation Obsessive, How to Paint Like Hayao Miyazaki](https://animationobsessive.substack.com/p/how-to-paint-like-hayao-miyazaki)). Highlights stay tinted by the light color.
- Oga learned to watch how "the color of a pillar also changes according to how the light hits it" ([Animation Obsessive, Totoro](https://animationobsessive.substack.com/p/defining-the-world-of-my-neighbor)). The lit side and the shaded side differ in hue, not just brightness.

### Color and mood

- Miyazaki's brief for Totoro: "Not piercing, raw primary colors, but softer in some way, colors that make you feel relieved when you see them," and "subdued but beautiful colors" (Animation Obsessive, Totoro, above).
- Yasuda's later work aimed for a "feeling of transparency" where you "sense the air." Muted palettes risked murkiness; restraint done right read as depth (Animation Obsessive, Yasuda, above). She and Miyazaki named special colors for "underwater color" and "glass-transparency color," which is exactly our problem with koi under water.
- The Ghibli Museum's color exhibition: scenes "express time and weather through lighting," and colors carry the texture of materials ([Ghibli Museum](https://www.ghibli-museum.jp/en/exhibitions/013128/)).
- Only Yesterday contrasts lighter, fuzzier colors for memories with robust colors for the present ([Tokyo Art Beat](https://www.tokyoartbeat.com/en/articles/-/kazuo-oga-the-man-who-drew-totoros-forest)). Saturation and edge softness together can carry mood.
- James Gurney's gamut masking: pick the palette as a shape on the color wheel before painting. A triangle off to one side of the wheel, not covering the center, gives a moody, unified scheme. He uses it most when color-scripting a film or book ([Gurney Journey, Gamut Masking](http://gurneyjourney.blogspot.com/2011/09/part-3-gamut-masking-method.html)).

### Water

Direct technique notes on Ghibli water are scarce in English sources. The general background-painting guidance is consistent:

- Don't paint water in one solid color; water near the bank is a different shade or hue ([mclelun](http://www.mclelun.com/2016/02/lake-reflection-tutorial.html)).
- Reflections are a bit darker and less saturated than the object reflected (same source).
- Shallow stream beds are painted as stones and gravel first, water over them (search summary of the same tutorials).
- Glints are small star shapes with a glow, added last (same source).
- Wave and ripple patterns are drawn as a separate layer of lines, not simulated shading.

### Rain

Shinkai's rain in The Garden of Words is built from layers: blurred background streaks that cycle loosely, sharp foreground streaks that last one drawing each, a dark-light-dark value structure, and small blurred splash highlights. His stated aim: abstracting so it becomes "what a person sees in rain" ([canmom, Animation Night 132](https://canmom.art/films/animation-night/132-makoto-shinkai-2)). Rain in anime reads with cool blue-grey ambient light and diffused highlights ([search summary](https://brinkhelsinki.com/logs/color-script-in-animation/)).

### Light through leaves (komorebi)

Tutorials agree on the structure: decide the light direction, paint the whole area in shadow color, then paint the light patches in, or paint around them (negative painting) ([Cat Lencke](https://www.catlenckewatercolor.com/blog/paintinglight), [Clip Studio Tips 4694](https://tips.clip-studio.com/en-us/articles/4694)). The patches are round to oval (pinhole images of the sun), grouped, with soft edges.

## Rules for the renderer

These are my reading of the sources, made concrete. Numbers are starting points, checked against the palettes below with OKLCH.

**Value structure**

- Three value groups per material: light, base, shadow. One extra accent step (glint, lit rim) allowed on small elements only.
- Water: three depth bands (matches STYLE.md). The deepest band in day themes stays at OKLCH L 0.45 or above; night can go to 0.30 but never below.
- Koi own the extremes. `koi_white` is the lightest thing on screen after glints, `koi_sumi` the darkest. Background values sit between them.
- Night: overall key drops, but keep a value range of at least 0.55 L between darkest and lightest, and put the brightest values in two or three small places (moon glint, lantern, white koi).

**Saturation budget (OKLCH chroma)**

| Element | Max chroma | Why |
|---|---|---|
| Water bands | 0.10 | Largest area, must stay calm. |
| Floor, stones | 0.07 | Neutral carrier; the water tint does the work. |
| Foliage, pads | 0.14 | Greens can be rich; Oga's strength was green. |
| Shadow color | 0.10 | Colored, not loud. |
| Highlight / light color | 0.10 | Tinted light, not neon. |
| Koi red, ogon, flowers | 0.19 | The only strong accents. Small areas. |

A theme where more than about 15% of the screen is above 0.14 chroma will read as busy.

**Hue shifts**

- Shadows shift toward the theme's shadow hue (blue-violet by day, violet at dusk, green-teal under trees), and drop about 0.15 to 0.25 L.
- Lit sides shift toward the light hue (cream by day, orange-pink at dusk, pale blue by moon) and rise about 0.08 to 0.15 L.
- The hue shift between lit and shadow side of the same material should be 30 to 90 degrees. Less than that and it looks like plain darkening.
- Reflections: desaturate by about a third and drop L by about 0.05 to 0.10 compared with the object.

**Edges**

- Soft (falloff of a few pixels, grain-wobbled): depth bands, sky and cloud reflections, dapple patches, fog, rain streaks far away.
- Hard (one pixel, maybe with a one-pixel darker rim): koi, pad edges, stone tops, ripple lines, petals, near rain streaks.
- No element uses both.

**Tone steps and pixelation**

- Painterly themes: 3 tones per material, smooth-ish depth bands, grain 2 to 4 percent.
- Pixel themes: the same 3 tones but quantized hard, no grain (the pixel grid is the texture), hue-shifted ramps (each step darker also rotates toward the shadow hue). This is the same rule painters use, applied with a hard edge.

**Detail placement**

- Group foliage into clumps; no individual leaves except a few drifting ones.
- Detail density falls off away from the center of the pond where the koi are.
- One or two "on a whim" details per theme (Oga's orchids): a single iris clump, a frog on a stone. Small, unexpected, never repeated.

## Mood palettes

Each palette fills the slots the water renderer uses today, plus one proposed new slot, `shadow`, for the explicit shadow color (today STYLE.md derives it from the hue of `deep`). Values were checked in OKLCH: water chroma stays at or under 0.10, only koi and one accent go above 0.15.

Koi red against `mid` has low lightness contrast in some themes (1.0 to 1.5). That matches the existing palettes and is carried by hue (orange against teal is nearly opposite) plus the koi outline. If a theme feels weak, darken `mid` rather than brightening the red.

The turtle and UI slots are left out; they can be derived from these or copied from `summer-garden`.

### 1. Summer Noon

Bright, high-key, clear. The default mood. Cream light, blue-violet shadow.

| Slot | Hex |
|---|---|
| deep | `#2A7F9A` |
| mid | `#4DB3A2` |
| shallow | `#8FDCCB` |
| highlight | `#FFF6D8` |
| shadow | `#3C5A8C` |
| stone_light | `#E6D6AE` |
| stone_dark | `#72807C` |
| lily_dark | `#3F8C4C` |
| lily_light | `#A6D271` |
| lily_flower | `#F5A9BC` |
| koi_white | `#FBF6EC` |
| koi_red | `#E8502E` |
| koi_sumi | `#2A2630` |
| ogon | `#F2B63D` |

Look: 3 tones, grain 3%, full caustics, strong dapple, white clouds.

### 2. Morning Mist

Pale, soft, low contrast. Everything slightly desaturated as if seen through haze.

| Slot | Hex |
|---|---|
| deep | `#3F8795` |
| mid | `#6AAEA8` |
| shallow | `#A8D6CC` |
| highlight | `#F4F1E4` |
| shadow | `#5E7896` |
| stone_light | `#DCD8C6` |
| stone_dark | `#7D8A8A` |
| lily_dark | `#4E8A62` |
| lily_light | `#A9C98E` |
| lily_flower | `#F2C4CF` |
| koi_white | `#F7F4EE` |
| koi_red | `#E0613E` |
| koi_sumi | `#2E2E36` |
| ogon | `#EDC05A` |

Look: soft edges everywhere except koi, weak caustics, a pale fog layer over the far edges, no hard cloud shapes.

### 3. Golden Evening

Warm orange light and lavender shadows. Close to the existing `evening-garden`, with the shadow made explicit.

| Slot | Hex |
|---|---|
| deep | `#4B5C8E` |
| mid | `#5E8FA6` |
| shallow | `#9CC0B4` |
| highlight | `#FFC98F` |
| shadow | `#6B5A95` |
| stone_light | `#E4BE98` |
| stone_dark | `#6E6484` |
| lily_dark | `#466E4E` |
| lily_light | `#9DB066` |
| lily_flower | `#F7A0B2` |
| koi_white | `#FBEADA` |
| koi_red | `#F05E38` |
| koi_sumi | `#2B2538` |
| ogon | `#F6B04A` |

Look: long shadows (offset larger), low sun glints, clouds tinted peach.

### 4. Summer Rain

Overcast, cool, quiet. Greens get richer in rain; highlights go almost grey. Hydrangea violet replaces pink.

| Slot | Hex |
|---|---|
| deep | `#36687A` |
| mid | `#4F8A8A` |
| shallow | `#7FAFA6` |
| highlight | `#DDE8E4` |
| shadow | `#3E5670` |
| stone_light | `#B4BAAE` |
| stone_dark | `#56625F` |
| lily_dark | `#2F7A48` |
| lily_light | `#7DBA5E` |
| lily_flower | `#9A92D4` |
| koi_white | `#EEF0EA` |
| koi_red | `#E0502F` |
| koi_sumi | `#26282E` |
| ogon | `#E9B544` |

Look: no caustics, no cast shadows (diffuse light), raindrop ripple rings as the main motion, wet stones (stone_light darker than in sun), grey sky reflection instead of clouds.

### 5. Blue Night

Moonlight. Blue-violet, never black. Small warm accents allowed (a lantern glow at `#F2B25A` if one is added).

| Slot | Hex |
|---|---|
| deep | `#1F3056` |
| mid | `#2E4E78` |
| shallow | `#4F7A98` |
| highlight | `#CFE0F2` |
| shadow | `#1B2244` |
| stone_light | `#8E9AB4` |
| stone_dark | `#3A4260` |
| lily_dark | `#2A4A52` |
| lily_light | `#577A6E` |
| lily_flower | `#C8B8DE` |
| koi_white | `#DCE2EE` |
| koi_red | `#C8583E` |
| koi_sumi | `#151827` |
| ogon | `#D8B25C` |

Look: one moon reflection, very faint caustics, fireflies as the only moving lights, white koi carry the scene (contrast 10:1 against deep).

### 6. Autumn Afternoon

Low warm sun, ochre and olive plants, maple red as the accent (the `lily_flower` slot holds drifting maple leaves here).

| Slot | Hex |
|---|---|
| deep | `#2F6E86` |
| mid | `#4E9A94` |
| shallow | `#93C7B2` |
| highlight | `#FFE6B8` |
| shadow | `#4E5A86` |
| stone_light | `#E0C89A` |
| stone_dark | `#7A6E66` |
| lily_dark | `#6E7A3A` |
| lily_light | `#C7A94E` |
| lily_flower | `#D9573A` |
| koi_white | `#FAF2E4` |
| koi_red | `#E4542E` |
| koi_sumi | `#2A2428` |
| ogon | `#F0B040` |

Look: fewer lotus flowers, more drifting leaves, sparser foliage, medium caustics.

### 7. Sakura Spring

Pastel, airy, pale pink petals on the surface. The lightest and gentlest theme.

| Slot | Hex |
|---|---|
| deep | `#3A7FA0` |
| mid | `#62B2B0` |
| shallow | `#A4DCD0` |
| highlight | `#FFF4E8` |
| shadow | `#6A73A8` |
| stone_light | `#EADCC4` |
| stone_dark | `#7E7F8C` |
| lily_dark | `#4F8F55` |
| lily_light | `#B2D77E` |
| lily_flower | `#F7C1CE` |
| koi_white | `#FBF6F0` |
| koi_red | `#E85A3A` |
| koi_sumi | `#2C2832` |
| ogon | `#F2BC4A` |

Look: many petals drifting, small young lily pads, soft clouds, grain 2%.

### 8. Forest Shade

A pond under a heavy canopy: deep green water, yellow-green sun spots, green-teal shadows. The dapple is the whole show.

| Slot | Hex |
|---|---|
| deep | `#1E5E62` |
| mid | `#347F74` |
| shallow | `#6BA890` |
| highlight | `#F1F0C0` |
| shadow | `#1F3F4E` |
| stone_light | `#C9C7A0` |
| stone_dark | `#4F625A` |
| lily_dark | `#2E6E3E` |
| lily_light | `#8FC060` |
| lily_flower | `#F4F0E0` |
| koi_white | `#F4F2E6` |
| koi_red | `#E4562E` |
| koi_sumi | `#22262A` |
| ogon | `#EDB83E` |

Look: foliage cover over most of the edges, large slow dapple patches as the main light, caustics only inside sun spots, white flowers.

## What this means for the theme system

- A theme is two things: a palette (the slots above) and a look (tone steps, edge softness, grain, caustic strength, dapple strength, shadow offset, weather layer, foliage density, flower type). The sources show mood changes coming mostly from light and color, so most themes can share one scene layout and differ only in these two tables.
- Add `shadow` as a real slot. The "shadow from the hue of `deep`" rule works for summer but fails for forest shade and rain, where the shadow should come from the environment.
- Keep the three-tone rule shared between painted and pixel themes. Only the edge regime and grain change.

## Sources

- [Gurney Journey: Demo by Kazuo Oga](http://gurneyjourney.blogspot.com/2017/03/demo-by-kazuo-oga.html)
- [Gurney Journey: Painting in the Ghibli Style](https://gurneyjourney.blogspot.com/2018/05/painting-in-ghibli-style.html)
- [Gurney Journey: Gamut Masking Method](http://gurneyjourney.blogspot.com/2011/09/part-3-gamut-masking-method.html)
- [Animation Obsessive: What Kazuo Oga Thinks About When He Thinks About Backgrounds](https://animationobsessive.substack.com/p/what-kazuo-oga-thinks-about-when)
- [Animation Obsessive: Defining the World of My Neighbor Totoro](https://animationobsessive.substack.com/p/defining-the-world-of-my-neighbor)
- [Animation Obsessive: The Artist Who Colored Ghibli](https://animationobsessive.substack.com/p/the-artist-who-colored-ghibli)
- [Animation Obsessive: How to Paint Like Hayao Miyazaki](https://animationobsessive.substack.com/p/how-to-paint-like-hayao-miyazaki)
- [Ghibli Museum: Painting the Colors of Our Films](https://www.ghibli-museum.jp/en/exhibitions/013128/)
- [Wikipedia: Michiyo Yasuda](https://en.wikipedia.org/wiki/Michiyo_Yasuda)
- [Tokyo Art Beat: Kazuo Oga, The Man Who Drew Totoro's Forest](https://www.tokyoartbeat.com/en/articles/-/kazuo-oga-the-man-who-drew-totoros-forest)
- [Mike Blake: Ponyo, Kazuo Oga, and poster paint](http://monisawa.blogspot.com/2010/04/ponyo-kazuo-oga-and-poster-paint.html)
- [Mleczny Mlecz: Ghibli color analysis](https://www.mlecznymlecz.com/how-to-paint-like-ghibli-ghibli-color-analysis/)
- [GVAAT: How to Paint Ghibli Backgrounds](https://gvaat.com/blog/how-to-paint-ghibli-backgrounds/)
- [canmom: Animation Night 132, Makoto Shinkai](https://canmom.art/films/animation-night/132-makoto-shinkai-2)
- [Anifusion: Makoto Shinkai style (quotes on shadow color)](https://anifusion.ai/style/makoto-shinkai-style/)
- [mclelun: Lake Reflection Tutorial](http://www.mclelun.com/2016/02/lake-reflection-tutorial.html)
- [Clip Studio Tips: anime shading](https://tips.clip-studio.com/en-us/articles/2964)
- [Clip Studio Tips: Komorebi](https://tips.clip-studio.com/en-us/articles/4694)
- [Cat Lencke: Painting Light, Komorebi](https://www.catlenckewatercolor.com/blog/paintinglight)
- [Brink Helsinki: Color Script in Animation](https://brinkhelsinki.com/logs/color-script-in-animation/)
- [Cartoon Brew: Atsushi Okui interview](https://www.cartoonbrew.com/interviews/whisper-of-the-heart-cinematographer-atsushi-okui-interview-260963.html)
