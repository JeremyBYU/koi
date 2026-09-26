# Prior art: digital koi ponds, aquariums and ambient toys

What other calm fish toys do well and badly, and what our pond should take from them.

## Key takeaways

1. **Fish readability beats water detail.** Every well-liked pond or tank keeps the fish the most contrasting thing on screen. Separate them by luminance, not only hue: bright cream and vermilion koi over mid-value teal, never over near-black or over busy highlights.
2. **Caustics and dapple compete with the fish.** Moving light patterns are visual noise that masks moving targets. Sharp-edged, fine-scale, fast caustics are the worst case. Keep ours soft-edged, coarse and slow, and dim them where a fish is.
3. **Motion must not look synchronized.** The top complaint in koi app reviews is fish that "slide" and "waggle tail tips at the same speed, same rhythm". Randomize phase and frequency per fish and let the body bend, not just the tail.
4. **The loved interactions are small and physical.** Tap to make a ripple that scatters fish, hold still and fish come to nibble, drop food and they gather. Response should be immediate, then the pond should settle back on its own.
5. **Stylize, don't simulate.** Wind Waker water and A Short Hike age well because they draw water as a few flat bands plus hand-drawn lines. Reviewers of the realistic Koi Pond app kept asking for better water and "radical" look changes, which a style system gives cheaply.
6. **Let people change the look and the density.** Koi Pond's best-received update added day/night, color scheme, fish count and lily pad count. Pond Koi users asked to remove fish entirely. Themes plus a few sliders (fish count, pads, flowers) cover this.
7. **Pixelation needs outlines and flat shading to stay readable.** A Short Hike renders at low resolution with no anti-aliasing, flat shading and a soft outline, and exposes the pixel size as a setting. Our `water_px` knob is the same idea.
8. **Glints need dark around them.** Pixel art guidance: if every mark uses the brightest color, a quiet ripple and a sparkle look the same. Reserve `highlight` at full strength for rare crest glints.
9. **Calm is muted base plus one saturated accent.** Low-to-moderate saturation blues and greens for the environment, with koi red and gold as the only strong color. The accent then carries all the attention, which is what we want.
10. **Never interrupt, never loop visibly.** Aquarium screensavers lose calm through short obvious loops and anything that interrupts (ads, pop-ups, UI churn). A split-pane companion (fishtanks' `/zen`) should hide UI until a key is pressed.

## Details

### Koi Pond (iOS, The Blimp Pilots, 2008)

The app that defined the genre. A dynamic water simulation with refracting koi, fish that cast shadows, ripples with splash sounds that scatter the fish and make lily pads shimmer. Holding a finger still makes the fish approach and "nibble" with a vibration. Shaking the phone drops food. Version 2 added four layerable nature sounds, day/night lighting, color scheme, brightness, and fish and lily pad counts.

- Loved: the hold-still-and-they-come interaction, layered ambient sound, and customization of mood.
- Criticized: water texture not animated well enough; "tweak the pond, but not really improve it, or radically change it"; "close enough that we lose relaxation thinking about how we'd fix it". Realism raised expectations it could not meet.
- The fish were added "to make it feel like it was at least worth 99 cents": the water was the core. A therapist used it with Alzheimer's patients, which surprised the developers.

Lesson: the interaction set (ripple, attract, feed) is right. Offer look changes that are radical (a theme switch), not only brightness sliders.

Sources: https://www.macworld.com/article/193939/koipond-2.html, https://www.ilounge.com/index.php/reviews/entry/koi-pond-by-the-blimp-pilots, https://www.ilounge.com/index.php/reviews/entry/koi-pond-version-2-0-by-the-blimp-pilots, https://www.macworld.com/article/192532/koipond.html

### Pond Koi / "Koi Pond: Decompress and relax" (iOS)

Customizable fish, backgrounds and decorations, a large sound library, ripples and feeding. User complaints are the most concrete pitfall list we found:

- "They slide more than swim. Only the very ends of the fins seem to move."
- "Fish are stiff, cartoon-like, and movements are synchronized... they all waggle tail tips at the same speed, same rhythm."
- Users wanted to remove fish entirely to enjoy only the water.

Lesson: stylized fish are fine, synchronized fish are not. Our spine chain and per-fish phase (research/koi.md) address this directly. A "water only" theme or fish count of 0 is cheap to support.

Source: https://apps.apple.com/us/app/koi-pond-decompress-and-relax/id642875147

### Zen Koi and Koi (mobile, flash-style 2D)

Flat, simple 2D art: bright blues and greens with a small red koi as the only warm color. Reviewers credit the calm to the art plus rustling leaves, water drops and slow piano. The one complaint in the Koi review was a timed memory puzzle that "harshed my mellow": any failure state breaks calm.

Lesson: the complementary pair (teal-green field, red-orange fish) is the readability engine. Never add anything the user can fail at.

Sources: https://www.heypoorplayer.com/2016/04/20/koi-review/, https://apps.apple.com/us/app/zen-koi-2/id1280254438

### Whimsy Koi (itch.io)

Click to feed, fish grow, decorations and lighting change the pond. A dragonfly automates feeding so the user does not have to click constantly. Players asked for visual maturity indicators and pond switching.

Lesson: pond switching is something players of this genre ask for; our theme hotkey is that.

Source: https://tinggame.itch.io/whimsy-koi

### asciiquarium and terminal aquariums

asciiquarium (Perl, 2003, ports in Rust and Python) draws a side view: a wavy surface line, seaweed, fish, sharks, a whale, bubbles. It has no options and randomizes each run. Its charm is the surprise of life in a terminal and the variety of rare visitors (the shark, the submarine), not fidelity. fishtanks (Rust, ratatui) calls itself a split-pane companion: fish drift and blow bubbles, get "zoomies" when stimulated, and `/zen` hides all UI until any key is pressed. It works down to 40x14 cells.

Lessons: rare visitors (a dragonfly, a turtle, a falling leaf) give long-session variety without raising baseline motion. A zen mode that hides the status line suits us. Test at small window sizes.

Sources: https://github.com/cmatsuoka/asciiquarium, https://github.com/Yazelix/asciiquarium-rs, https://opensource.com/article/18/12/linux-toy-asciiquarium, https://github.com/daniel-retamal/fishtanks, https://pkg.go.dev/github.com/0magnet/termanim/aquarium

### Aquarium screensavers (SereneScreen Marine Aquarium, Dream Aquarium)

Marine Aquarium (20M+ downloads, bundled with Windows XP Plus!) rendered fish in real time, up to 30 at once, with random darting and bubbles. Dream Aquarium, by a VFX artist, is praised for fish behavior (pecking, schooling, chasing) and animated gills and eyes, with shifting light rays and a day or night lighting choice. A guide to aquarium screensavers names the pitfalls: short obvious loops, ads or autoplay that interrupt, and AI video where fish morph and coral flickers.

Lesson: behavior variety matters more than rendering detail. Our procedural sim never loops, which is an advantage over video. Flicker of any kind is disqualifying.

Sources: https://serenescreen.com/about.html, https://www.dreamaquarium.com/index_DA1.html, https://www.uscenes.com/blogs/guides/aquarium-screensaver

### Stylized toon water (Wind Waker and its imitators)

Wind Waker's sea is a saturated cobalt blue with a lattice of thin white hand-drawn lines, built from stacked scrolling layers wiggled by a displacement map, with blotchy Voronoi foam. Amplitude is gentle: "the surface should ripple, not churn". A retrospective argues the water aged well because waves are literally drawn lines, not detail. Toon water shaders for Unity and Godot reproduce it with quantized depth color bands, Voronoi foam at the shoreline, and a thresholded specular.

Lessons for us: our three flat depth bands and hand-drawn ripple lines are already this approach. Shoreline foam from a Voronoi edge is a cheap theme option. Line networks read as calm only if lines wander, vary in thickness and move slowly.

Sources: https://sourcegaming.info/2017/08/10/holism-the-wind-wakers-cel-shaded-graphics/, https://github.com/paulriordanfb/stylized-water/blob/master/WATER_SPEC.md, https://github.com/CleverInfinity/Toony-Water-Shader, https://godotshaders.com/shader/wind-waker-water/, https://www.shadertoy.com/view/ltfGD7

### Pixelated 3D and pixel water (A Short Hike, pixel art tutorials)

A Short Hike renders 3D at low resolution with "flat cohesive shading and no anti-aliasing", adds "a soft outline effect to objects to help them stand out, and stay readable with so few pixels", and lets players tune the pixel size. Pixel art water guidance: glints need darker water around them; sparkle loops of about 6 frames; reflections tinted blue because water absorbs red and green first; for 2D water surfaces, displace the sample position with sines and keep vertical displacement about half the horizontal.

Lessons: a "pixel" theme is more than a bigger `water_px`. It should also snap shading to fewer tones, turn off soft edges, and add a 1 px darker outline on koi and pads. Glint budget stays small.

Sources: https://blog.playstation.com/2021/08/05/crafting-a-tiny-open-world-a-look-behind-the-scenes-at-the-creation-of-a-short-hike/, https://www.slynyrd.com/blog/2018/10/12/pixelblog-10-water-in-motion, https://injuly.in/blog/water-shader/index.html, https://lospec.com/pixel-art-tutorials/tags/water

### Shadertoy and procedural ponds

A procedural Unreal pond by Oday Abuzaeed breaks shapes into mosaic tiles so it reads like an LED panel, with many parameters for variations. It shows that one strong stylization filter over a simple scene can define a whole theme. Shadertoy has koi pond experiments (for example "Koi Pond #1") but nothing we found with a documented look worth copying.

Lesson: a mosaic or tile quantization pass is a candidate "look" axis alongside pixelation.

Source: https://80.lv/articles/stunning-mosaic-like-procedural-animated-pond-shader-with-swimming-koi

### Desktop ambient games (Rusty's Retirement)

An idle farm that sits in a strip of the desktop. Pixel art in soothing earth tones "that complement rather than distract", slow ambient events (falling leaves, bees, butterflies), and no babysitting. The criticism was of actions that desync visually, which reads as broken.

Lesson: small occasional life (a dragonfly crossing, a leaf landing) is the right scale of event. Whatever moves must be causally consistent: food eaten by a fish should vanish at the mouth.

Sources: https://www.siliconera.com/review-rustys-retirement-offers-a-calming-and-simplistic-respite/, https://store.steampowered.com/app/2666510/Rustys_Retirement/

### Evidence on motion noise and fish visibility

A study on reef fish found that moving caustics made them significantly slower to spot moving prey than static caustics. Sharp boundaries and fine scale each made it worse, and fine plus sharp was worst. The authors frame it as signal to noise: environmental motion masks target motion. The same logic applies to a viewer watching koi.

Stardew Valley draws pond fish as flat dark silhouettes, and a popular mod exists purely to show them in full color. People want to see the fish.

Sources: https://pmc.ncbi.nlm.nih.gov/articles/PMC7209061/, https://www.nexusmods.com/stardewvalley/mods/47433

### Calm color and pacing in cozy games

A cozy design write-up cites color research that blues and greens lower arousal compared with reds and high saturation, recommends luminance-first readability, and notes that in a muted palette one saturated accent carries great weight. Stardew keeps moderate saturation and avoids retro neon. Treat the physiology claims as directional, not precise.

Source: https://guul.games/blog/pixels-and-peace-the-sensory-science-of-calming-game-design

## Concrete lessons for our pond

| Lesson | What to do | Theme knob it suggests |
|---|---|---|
| Fish must pop | Koi luminance at least clearly above or below local water; soft darker outline | `koi_outline` strength, per theme |
| Caustics are noise | Coarse cells, soft edges, slow; attenuate under and near koi | `caustic_scale`, `caustic_sharpness`, `caustic_strength` |
| Glints are rare | Full `highlight` only on the top crests | `glint_threshold` |
| No sync | Per-fish tail phase and frequency jitter | already in koi sim |
| Radical look changes | Named themes switched by hotkey, palette plus look together | `theme` |
| Pixel themes | Bigger `water_px`, fewer tone steps, hard edges, 1 px outlines | `water_px`, `tone_steps`, `edge_softness`, `outline_px` |
| Density control | Fish count (including 0), pad clusters, flowers | `koi_count`, `lily_clusters`, `flowers` |
| Toon foam option | Voronoi-edge foam at the rim | `shore_foam` |
| Rare visitors | Dragonfly, turtle, falling leaf on long timers | `visitors` |
| Zen mode | Hide status line until a key is pressed | `ui = "hidden"` |
| No failure, no interrupts | No timers, prompts or popups in the pond view | design rule |
