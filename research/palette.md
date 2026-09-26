# Palette

## Recommendation

Use **Jade Morning** as the default palette. It reads as clear, clean water rather than murky water, all five koi varieties stand out against it, and the UI text is easy to read.

Keep **Dusk Indigo** as a second palette for evenings. It has the strongest koi contrast of the five and feels the quietest. The two palettes share a structure, so a slow time-of-day blend between them is cheap if you want one later.

Whichever palette wins, two rules matter more than the exact hex values:

1. Keep the water in a narrow, dark luminance band. The koi live in the brightest part of the range. If the shallow water gets as bright as the red koi patches, the fish melt into it.
2. Make sure 24-bit color survives tmux. Without the `RGB` feature set, tmux falls back to the 256-color cube and every water gradient here turns into grey bands (details below).

Open `research/palettes.html` in a browser to compare all five side by side: the same pond, fish, turtle, ripple, and status bar, drawn in each palette.

## How to read the contrast numbers

The numbers are WCAG 2.x contrast ratios (1:1 is identical, 21:1 is black on white). WCAG asks for 4.5:1 for body text and 3:1 for large text and graphics ([W3C, Understanding SC 1.4.3](https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html), [SC 1.4.11](https://www.w3.org/WAI/WCAG21/Understanding/non-text-contrast.html)). For a pond these are guides, not pass/fail rules:

- The ratio measures lightness only. Red koi on teal water can score 1.7 and still stand out, because the hues are opposites. What drops out first is lightness contrast in peripheral vision and at small cell sizes, so a low ratio is a warning worth checking.
- UI text has to pass for real: aim for 4.5:1 or better.
- Sumi (black) patches on dark water always score low. That matches real koi seen from above, where black patches merge into deep water. Showa and sanke still read clearly because of their white and red patches.

The "vs shallow" column is the worst case. Most of the pond is deep or mid water.

## Koi reference colors

Common descriptions of the five varieties, which set the colors each palette has to cover:

- **Kohaku**: white body with red (hi) patches ([Kodama Koi Farm](https://www.kodamakoifarm.com/koi-varieties/)).
- **Taisho Sanke**: white base with red patches and small black (sumi) spots ([Aquascape](https://www.aquascapeinc.com/water-gardening/pond-fish/13-essential-koi-varieties)).
- **Showa**: black base with red and white patches, and black on the head ([Wikipedia: Showa](https://en.wikipedia.org/wiki/Showa_(fish))).
- **Ogon**: a single metallic color, most often gold (Yamabuki) or platinum ([Kodama Koi Farm](https://www.kodamakoifarm.com/koi-varieties/)).
- **Asagi**: blue-grey back with a net pattern of scales, red-orange flanks and belly, and a pale head ([Blue Ridge Koi](https://blueridgekoi.com/koi-goldfish-resources/koi-variety-guide/)).

Each palette uses six koi colors: `koi_white`, `koi_red`, `koi_sumi`, `ogon`, `asagi_blue`, `asagi_red`. In every palette the koi colors are adjusted a little toward the water's temperature (warmer whites on cool water, for example), so the fish look like they belong in that water and aren't pasted on top.

## The five palettes

Each palette has the same 21 slots. Water: `deep`, `mid`, `shallow`, `highlight` (ripple crests, glints, wake foam). Pond life: lily pads, lily flower, rim stones, turtle, food pellet. UI: text, dim text, accent.

### 1. Jade Morning (recommended default)

Mood: a garden pond just after sunrise. Clear green-teal water that you can see into. Fresh, bright, and still calm. It comes closest to the "clear and readable, not murky" goal in IDEAS.md.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#0E3B43` | lily_dark | `#2F6B3A` |
| mid | `#16606A` | lily_light | `#6FA35A` |
| shallow | `#1F7474` | lily_flower | `#F2C4CE` |
| highlight | `#BFE8DF` | stone_dark | `#5B6166` |
| koi_white | `#F4F1EA` | stone_light | `#A7A9A3` |
| koi_red | `#EE6343` | turtle_shell | `#5E5A3A` |
| koi_sumi | `#1B1B1F` | turtle_skin | `#8C8A55` |
| ogon | `#E8B84A` | food | `#C8894A` |
| asagi_blue | `#6F8FA8` | ui_text | `#E9F2EE` |
| asagi_red | `#D9774B` | ui_dim | `#86A8A3` |
| | | ui_accent | `#F0B35E` |

Contrast vs deep / vs shallow: white 10.8 / 4.9, red 3.8 / 1.7, ogon 6.6 / 3.0, asagi 3.6 / 1.2, food 4.1 / 1.4. UI text on deep 10.7, dim 4.7, accent 6.6.

Notes: the red is pushed slightly toward coral (`#EE6343`) so it separates from the teal in lightness as well as hue. My first draft had shallow at `#2A8C8C`, which put red koi at 1.07 against shallow water; they vanished at the pond edge. Asagi is the weakest fish over shallow water, but asagi sit low in the real pond too, so that is acceptable. The turtle sits close to the water's lightness (1.7) on purpose. It should be something you notice, not something that jumps out.

### 2. Dusk Indigo (recommended night palette)

Mood: evening, lanterns lit. Deep blue-violet water with warm fish glowing on it. The calmest of the five, and the best one for a dark room next to a dark editor.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#121A33` | lily_dark | `#24463F` |
| mid | `#1E2B52` | lily_light | `#4E7A63` |
| shallow | `#33477A` | lily_flower | `#E7B7D8` |
| highlight | `#A9B8E8` | stone_dark | `#3A3F52` |
| koi_white | `#EDE6DA` | stone_light | `#767B8F` |
| koi_red | `#F06A4A` | turtle_shell | `#4A4A3A` |
| koi_sumi | `#0A0C14` | turtle_skin | `#7A7658` |
| ogon | `#F2C56B` | food | `#D69A5C` |
| asagi_blue | `#7C93C4` | ui_text | `#E6E9F5` |
| asagi_red | `#E0855C` | ui_dim | `#7F89AD` |
| | | ui_accent | `#F2C56B` |

Contrast vs deep / vs shallow: white 13.9 / 7.3, red 5.6 / 3.0, ogon 10.6 / 5.6, asagi 5.6 / 2.9, food 7.1 / 3.7. UI text 14.2, dim 5.0, accent 10.6.

Notes: every fish clears 3:1 against deep water, and most do against shallow water. Lily pads and the turtle are dim (1.0 to 1.9). That suits night, but give the pads a lighter rim or vein line so they don't turn into holes. Ripple highlights in periwinkle look like moonlight.

### 3. Moss Garden

Mood: an old shaded garden pond. Green-brown water, weathered stone, cream lotus. Earthy and warm. The most naturalistic of the five, and closest to how real ponds look.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#1F2E24` | lily_dark | `#3E5A2A` |
| mid | `#2F4535` | lily_light | `#8BA55A` |
| shallow | `#3F5B43` | lily_flower | `#F5E6C8` |
| highlight | `#CFDCB8` | stone_dark | `#6B665C` |
| koi_white | `#F2EEE3` | stone_light | `#B5AE9E` |
| koi_red | `#D9482B` | turtle_shell | `#574B2E` |
| koi_sumi | `#151613` | turtle_skin | `#8E8457` |
| ogon | `#E3A93A` | food | `#B7834A` |
| asagi_blue | `#7E98A3` | ui_text | `#ECEBDD` |
| asagi_red | `#C9683F` | ui_dim | `#9AA38A` |
| | | ui_accent | `#E3A93A` |

Contrast vs deep / vs shallow: white 12.3 / 6.5, red 3.3 / 1.8, ogon 6.8 / 3.6, asagi 4.7 / 2.0, food 4.3 / 1.8. UI text 11.9, dim 5.4, accent 6.8.

Notes: this is the one most at risk of looking "murky", which IDEAS.md rules out. It works on a good display at full brightness. On a dim laptop screen, the deep and mid water nearly merge (1.9 apart). Lily pads blend into the water by hue as well as lightness, so they need an outline.

### 4. Sumi-e Paper

Mood: an ink-wash painting. Pale grey-blue water like rice paper, black ink outlines, vermilion koi. The only light palette. It looks striking and gallery-like, and it would stand out on screen.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#9FB4B8` | lily_dark | `#5E7552` |
| mid | `#BCCBCB` | lily_light | `#93A785` |
| shallow | `#D8E0DC` | lily_flower | `#E8A9A0` |
| highlight | `#F6F4EC` | stone_dark | `#6E6A64` |
| koi_white | `#FBF7EE` | stone_light | `#A9A49A` |
| koi_red | `#C8372D` | turtle_shell | `#4F4A3E` |
| koi_sumi | `#22201D` | turtle_skin | `#847C66` |
| ogon | `#B8862B` | food | `#8A5A36` |
| asagi_blue | `#5E7A8C` | ui_text | `#22201D` |
| asagi_red | `#C06A45` | ui_dim | `#6E6A64` |
| | | ui_accent | `#C8372D` |

Contrast vs deep / vs shallow: white 2.0 / 1.3, red 2.4 / 3.9, sumi 7.5 / 12.1, ogon 1.5 / 2.4, asagi 2.1 / 3.4. UI text 7.5, dim 2.5, accent 2.4.

Notes: this palette depends on a dark ink outline around every fish (the HTML mock draws one only here). White koi on pale water score 1.3 without it. That needs a clean 1-cell outline, which is hard with half-block text cells and easy with Kitty graphics. The UI dim and accent colors fail on the status bar; use a paler bar (`#E6E9E4`) if you pick this one. It is also the brightest palette on screen, which works against "leave it open next to work" in a dark terminal. It makes a great screenshot but a poor default.

### 5. Prussian Wave

Mood: a Hokusai print. Prussian blue water, cream foam, vermilion and gold. More graphic and saturated than the others. It feels crafted rather than photographic.

| Slot | Hex | Slot | Hex |
|---|---|---|---|
| deep | `#0B2545` | lily_dark | `#3C6E47` |
| mid | `#134074` | lily_light | `#7FA36B` |
| shallow | `#1A527A` | lily_flower | `#F0B7A4` |
| highlight | `#E8DCC2` | stone_dark | `#4B4E57` |
| koi_white | `#F3EAD8` | stone_light | `#9A9486` |
| koi_red | `#D1432F` | turtle_shell | `#5C5333` |
| koi_sumi | `#141414` | turtle_skin | `#948A5C` |
| ogon | `#E9B949` | food | `#C98A4B` |
| asagi_blue | `#8DA9C4` | ui_text | `#F3EAD8` |
| asagi_red | `#D8743F` | ui_dim | `#8DA9C4` |
| | | ui_accent | `#E9B949` |

Contrast vs deep / vs shallow: white 12.9 / 6.9, red 3.3 / 1.8, ogon 8.4 / 4.5, asagi 6.3 / 2.8, food 5.3 / 2.4. UI text 12.9, dim 6.3, accent 8.4.

Notes: the strongest UI contrast after Dusk Indigo, and the cream highlight makes ripples and wakes very visible. That helps show off the water simulation, but it can get busy when many ripples overlap. The saturated blue is the least "tranquil" water of the five over a long session. Try it if Jade feels too soft.

## Terminal and tmux notes

**24-bit color through tmux.** Ghostty supports 24-bit color directly. Inside tmux, tmux must be told the outer terminal supports RGB. The tmux FAQ says: "tmux must be told that the terminal outside supports RGB colour. This is done by specifying the `RGB` or `Tc` terminfo(5) flags" ([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)). For Ghostty (which sets `TERM=xterm-ghostty`) on tmux 3.2 or later:

```
set -as terminal-features ",xterm-ghostty:RGB"
```

Without it, tmux maps each RGB color to the nearest entry in the 256-color palette (`colour_find_rgb` in [tmux colour.c](https://github.com/tmux/tmux/blob/master/colour.c)). I ran that mapping on the water colors:

| Palette | deep | mid | shallow |
|---|---|---|---|
| Jade Morning | `#303030` (grey) | `#005F5F` | `#005F5F` |
| Dusk Indigo | `#1C1C1C` (grey) | `#303030` (grey) | `#5F5F87` |
| Moss Garden | `#262626` (grey) | `#3A3A3A` (grey) | `#585858` (grey) |
| Prussian Wave | `#262626` (grey) | `#005F87` | `#005F87` |

Every palette falls apart. Dark, low-saturation colors all land in the greyscale ramp, and the smooth deep-to-shallow gradients the water simulation depends on become two or three flat steps. The game should check `COLORTERM=truecolor` at startup, and if it is missing, print one line saying how to enable RGB in tmux, rather than rendering a grey pond. Other guides describe the same setup ([sunaku](https://sunaku.github.io/tmux-24bit-color.html), [jdhao](https://jdhao.github.io/2018/10/19/tmux_nvim_true_color/)).

**Kitty graphics.** If the renderer uses Kitty graphics images instead of colored text cells, these hex values become pixel colors and the tmux RGB setting stops mattering for the pond itself (it still matters for the status bar text). Images inside tmux need `allow-passthrough` instead. That belongs in the rendering research, not here.

**Half-block rendering.** With half-block characters, each text cell has one foreground and one background color, so fine detail like a 1-pixel ink outline or a small sumi spot gets lost. The dark-water palettes (Jade, Dusk, Prussian) don't depend on outlines, so they survive this better. Sumi-e depends on outlines.

**Build the palette as data.** Put all 21 slots in one struct and have the renderer read only from it. Water shading should blend between `deep`, `mid`, `shallow` and `highlight` in linear light, not in sRGB, or ripple crests will look muddy. Keeping the palette as data also makes swapping palettes, and a later day-to-night blend, a matter of reading a different struct.

## Files

- `research/palette.md`: this file.
- `research/palettes.html`: a static page with no JavaScript and no external dependencies. It shows every palette as a mock pond, its status bar, and a collapsible swatch and contrast table.
