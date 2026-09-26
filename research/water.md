# Water

## Recommendation

1. Simulate the surface as a damped 2D height field on the linear wave equation, two buffers swapped each step (the classic "Hugo Elias" ripple). It costs almost nothing: a 480x240 grid steps in about 20 microseconds on this machine.
2. Size the grid to the *display* pixel grid, not the terminal cell grid. For half-block rendering that is cols x (rows*2). For Kitty graphics, simulate at a modest fixed size (for example 480x270) and let the image scale up.
3. Run the sim at a fixed 60 steps per second and tune wave speed down (`k` around 0.1 to 0.25, not the 0.5 of the textbook version). Slow, long-lived ripples read as calm; fast ones read as busy.
4. Inject food drops as small Gaussian dents, and fish wakes as a moving Gaussian dent under each fish plus a small alternating push at the tail. Absorb waves at the edges with a sponge band so the pond settles.
5. Shade from the height field's derivatives only: a tilt-based light term from the gradient, a refraction offset when sampling the pond floor and fish, and a caustic term from the curvature (analytic area ratio). Quantize the final color so still water produces identical cells frame to frame and the diff renderer sends nothing.

Skip: full fluid solvers (Navier-Stokes, SPH), FFT ocean spectra (made for open sea, not a pond), and iWave unless the plain wave equation's wakes turn out to look wrong (details below).

## Budget

At 60 fps a frame is 16.7 ms. Terminal output is the expensive part (covered in the rendering research), so water should stay under about 1 ms. Measured single-threaded, `rustc -O -C target-cpu=native`, Rust 1.93, AMD Threadripper 7960X, f32:

| Grid | Sim step | Shading (gradient + light + caustic) |
|---|---|---|
| 240x120 (half-block, 240x60 terminal) | 6 us | 46 us |
| 480x240 | 20 us | 164 us |
| 960x480 | 76 us | 637 us |

The shading loop was naive (a division and clamps per cell). It would get faster with SIMD or by approximating `1/J`, but it doesn't need to. A laptop CPU may be 2x slower; the numbers still fit. Neither loop needs threads or a GPU.

## 1. The height field

Each cell holds one number, `h`, the height of the water above rest. Two arrays hold the current and previous step. The discrete wave equation, with damping applied to velocity:

```
// k = (c*dt/dx)^2, stable for k <= 0.5 on the 4-neighbour stencil
// damp in (0,1), applied to velocity (h - h_prev)
for y in 1..H-1:
  for x in 1..W-1:
    lap = cur[x-1,y] + cur[x+1,y] + cur[x,y-1] + cur[x,y+1] - 4*cur[x,y]
    prev[x,y] = cur[x,y] + (cur[x,y] - prev[x,y]) * damp + k * lap
swap(cur, prev)
```

Writing into `prev` in place is safe because each cell reads its own old `prev` value only. This is the same scheme as Hugo Elias's ripple tutorial, where `new = (sum of 4 neighbours)/2 - new` is exactly the case `k = 0.5` ([Maciej Matyka's writeup](http://maciejmatyka.blogspot.com/2018/03/2d-water-algorithm.html), [PixelEuphoria](https://pixeleuphoria.com/blog/index.php/2021/01/19/playing-around-with-a-2d-wave-algorithm/)). Matthias Müller's GDC 2008 talk covers the same method with a velocity array, boundaries and object coupling ([slides](https://media.gdcvault.com/gdc08/slides/S6509i1.pdf)).

### Parameters that matter

- **Wave speed.** A disturbance travels about `sqrt(2k)` cells per step. At `k = 0.5`, ripples cross a 240-wide grid in about 5.6 s at 60 steps/s. That already looks fast in a small pond. Start at `k = 0.15` (about 0.55 cells/step). If you need slower waves than `k` alone gives cleanly, step the sim at 30 Hz and interpolate heights for display.
- **Damping.** Published values run 0.94 to 0.99 per step when damping the whole height ([GameDev.net thread](https://www.gamedev.net/forums/topic/608562-hugo-elias-water-ripple-question/)). For a calm pond you want a ripple to live 3 to 5 seconds. Velocity damping of about 0.995 to 0.998 per step at 60 Hz gets there. Tune by eye.
- **Fixed timestep.** The step is not stable for arbitrary `dt`. Run exactly one step per 1/60 s tick (accumulator loop) and never scale by frame time.
- **Checkerboard noise.** The 4-neighbour stencil lets a 2-cell-wavelength "checkerboard" mode survive, which shows up as sparkle. Avoid injecting it: never poke single cells, always stamp smooth bumps at least 3 cells wide. If it still appears, blend in a little of a 9-point Laplacian (`(4*edge + diag - 20*center)/6`) or add a tiny smoothing term `h += 0.01 * lap` once per step.

### Grid resolution

The sim grid should match what the eye can see, which is the pixel grid you draw:

- **Half-block cells** (`▀` with fg = top pixel, bg = bottom pixel): the display is `cols x 2*rows` roughly square pixels. A 240x60 terminal gives 240x120. Simulate at exactly that size. Wavelengths shorter than about 4 display pixels turn into flicker, so keep drop radius at 2 to 4 cells.
- **Kitty graphics image**: pixels are free to simulate but expensive to send (the rendering research covers bandwidth). Simulate at the image size you transmit, for example 480x270, and let Ghostty scale the image to the window. Bilinear upscaling of smooth water looks fine.

Keep the grid independent of window size where you can: resizing then only changes the drawing scale, not the physics.

## 2. Boundaries and obstacles

Store a mask: water, or solid (pond wall, rock, lily pad stem, turtle shell).

- **Solid cell, reflecting.** Before each step, set every solid cell's `cur` to the value of its nearest water neighbour (Neumann boundary). Waves bounce off rocks and walls without phase flip. Setting solid cells to 0 also reflects, but inverted, which looks slightly harsher.
- **Sponge edge.** Multiply `damp` by a factor that ramps from 1.0 to about 0.9 over the 6 to 10 cells nearest the pond wall. Ripples then fade at the shore rather than ringing around the pond forever. Keep a little reflection (do not ramp to 0) so ripples still visibly touch the edge.
- **Floating objects** (lily pads, fish at the surface): treat them as a soft mask by multiplying their cells' velocity by 0.5 each step. They absorb and scatter waves a little without hard reflection.

## 3. Disturbances

### Food drop (ripple tank impulse)

Stamp a smooth dent into `cur` only (not `prev`), so the dent has an initial velocity and rings outward:

```
fn drop(cx, cy, radius r, strength s):
  for each cell within 2r of (cx, cy):
    d2 = (x-cx)^2 + (y-cy)^2
    cur[x,y] -= s * exp(-d2 / (r*r))
```

r = 1.5 to 3 cells, s about 1.0 in height units where shading treats 1.0 as a strong ripple. Use the sub-cell position (float cx, cy) so drops don't snap to the grid. A pellet hitting water gives one ring. For a nicer drop, add a second, weaker stamp of the opposite sign 6 to 10 steps later (the rebound).

### Fish wakes (moving source)

Apply a small pressure under each fish every step:

```
for each fish:
  // body: shallow dent that moves with the fish
  stamp_gaussian(body_pos, r = body_width/2, s = 0.02 * speed)
  // tail: alternating push from the tail beat
  stamp_gaussian(tail_pos, r = 1.5, s = 0.03 * sin(tail_phase))
```

Stamp into `cur` each step at small strength. The body dent alone makes a bow wave and a trail. The tail term makes a string of little rings that read as the fish swimming, even when it moves slowly. Scale both by depth if fish swim at different depths: deep koi should barely touch the surface.

**What shape the wake takes.** The linear wave equation has no dispersion: every wavelength travels at the same speed `c`. So a source slower than `c` makes rings that bunch up ahead of it (Doppler), and a source faster than `c` makes a Mach cone with half-angle `asin(c/v)`. Real deep water is dispersive, and gives the Kelvin wake, a V with half-angle about 19.47 degrees (arcsin 1/3) whatever the speed ([Wikipedia: Kelvin wake pattern](https://en.wikipedia.org/wiki/Kelvin_wake_pattern)). For koi that cruise slowly and only sometimes dart, the wave equation's Doppler rings plus tail rings look right. If a proper V behind a darting fish matters, there are two options:

- Tune `c` so a darting fish (for example 2x cruise speed) is faster than the waves. The Mach cone is a decent stand-in for a V.
- Switch to **iWave** (Tessendorf), which swaps the Laplacian for a convolution with a precomputed "vertical derivative" kernel of size (2P+1)^2 and so gets real water dispersion, Kelvin wakes included. P = 6 is the smallest kernel that looks like water, so each cell costs 169 multiply-adds instead of 5, about 30x the step cost. From the table above that is roughly 0.6 ms at 480x240, still affordable. The paper reports 128x128 at over 30 fps in software on 2000s-era CPUs ([Tessendorf, Interactive Water Surfaces](https://people.computing.clemson.edu/~jtessen/reports/papers_files/Interactive_Water_Surfaces.pdf)). Build the wave-equation version first. The sim is behind one function, so swapping it later is cheap.

### Ambient motion

A perfectly flat pond looks dead. Two cheap sources of life:

- Every 0.5 to 2 s, drop a very weak (s about 0.05), wide (r about 4) disturbance at a random spot: wind on the surface, an insect. It reads as a gentle shimmer.
- An ambient caustic pattern that does not come from the sim (section 4.4) keeps the floor moving even when the surface is still.

## 4. Shading

Everything is derived from `h` with central differences. No rays and no mesh.

```
gx  = (h[x+1,y] - h[x-1,y]) * 0.5
gy  = (h[x,y+1] - h[x,y-1]) * 0.5
hxx =  h[x+1,y] + h[x-1,y] - 2h
hyy =  h[x,y+1] + h[x,y-1] - 2h
hxy = (h[x+1,y+1] - h[x-1,y+1] - h[x+1,y-1] + h[x-1,y-1]) * 0.25
```

Compose back to front:

### 4.1 Refraction (the floor and the fish wobble)

Draw the pond floor (stones, sand, weeds) and the koi into an **underwater buffer** first. Then sample it at an offset proportional to the slope:

```
under = underwater[x - R*gx, y - R*gy]   // R ~ 2..6 pixels at peak slope
```

This texture-offset trick is the one used by most 2D ripple demos and Elias's original. It is the single most convincing effect: koi bend and wobble as ripples pass over them. Use bilinear sampling, or nearest-neighbour with R large enough that the shift is at least a pixel, otherwise small ripples do nothing visible at terminal resolution.

### 4.2 Caustics (light focused on the floor)

A curved surface focuses sunlight. The brightness on the floor is the ratio of the area a patch of surface covers to the area its refracted light lands on. Evan Wallace's WebGL water computes this per triangle using screen-space derivatives ([Rendering realtime caustics in WebGL](https://medium.com/@evanwallace/rendering-realtime-caustics-in-webgl-2a99a29a0b2c), [demo](https://madebyevan.com/webgl-water/)). On a height field with vertical light, that ratio is analytic. Light through (x,y) lands near `(x,y) + D * grad(h)`, where D is depth times the refraction factor, so the Jacobian is:

```
J = (1 + D*hxx) * (1 + D*hyy) - (D*hxy)^2
caustic = clamp(1 / max(|J|, 0.2), 0, 3)      // 1.0 = no change
```

For small D this reduces to `1 - D * laplacian`: troughs brighten, crests darken. Multiply the underwater color by `mix(1, caustic, 0.3..0.6)`. Tune D until bright bands appear just behind each ripple ring. This is sim-driven, so it only shows where the water moves.

### 4.3 Surface light (tilt and glints)

Treat `n = normalize(-gx*S, -gy*S, 1)` as the surface normal (S scales slope; about 4 to 8 for heights near 1.0). With a fixed sun direction `L`, add:

- **Tilt tint**: `under += A * dot(n.xy, L.xy)`. Slopes facing the sun lighten slightly and the far side darkens, which gives ripple rings their 3D look. A = small; this is the main way rings show on plain water.
- **Sky reflection**: `mix(under, sky_color, fresnel)` with `fresnel = F0 + (1-F0) * (1 - n.z)^5`, F0 about 0.02. Top down, n.z stays near 1, so this is mostly a subtle sheen on steep ripple faces. You can drop it if it muddies the palette.
- **Specular glints**: `spec = pow(max(dot(reflect(-L, n), V), 0), 40)`, V = (0,0,1). Add a near-white highlight. At terminal resolution glints are a single pixel or cell, so threshold them: show a glint only when `spec > 0.6`. That gives crisp sparkles instead of grey noise.

### 4.4 Ambient caustic pattern

For life in still water, add a slow procedural caustic to the floor: the classic trick is the minimum of two or three scrolling cellular (Worley) noise layers, or `abs(sin)` of two domain-warped sine fields. Evaluate it at low resolution (it is smooth) and animate it slowly (a period of 10 s or more). Keep it faint so the sim caustics from real ripples stay the brighter feature.

### 4.5 Depth tint

Tint the underwater layer toward the water color by depth: `mix(floor_color, water_color, depth_factor)`. Deeper koi get more tint and lower contrast. This one knob makes it read as a pond rather than fish on a floor. It is also where palette work plugs in.

## 5. Making it look good at terminal resolution

- **Amplitude and wavelength.** Ripples must be at least 4 display pixels per wavelength and change the color by a clearly visible step. Tune `R`, `S`, and the caustic mix so a fresh drop shifts brightness by 15 to 25 percent at its peak.
- **Quantize the output color.** Snap the final color to a fixed palette ramp (for example 32 to 64 steps per hue family, computed in a perceptual space like OKLab) before writing cells. Still water then produces identical colors frame after frame, and a diff-based terminal renderer sends zero bytes for it. Without this, float noise near zero changes the low bits of every cell on every frame, and each changed cell costs about 40 bytes of escape codes at 24-bit color. Also add a dead zone: treat |h| below about 0.002 as 0.
- **Temporal stability.** Do not add per-frame random dithering. It reads as flicker in a terminal. If banding shows, use a fixed (not animated) ordered dither pattern.
- **Half-block specifics.** Each cell carries two pixels. Shading already works per pixel, so draw pixel (x, 2r) as fg and (x, 2r+1) as bg of `▀`. A glint can instead use a character (`·`, `˙`, `∙`) on top of the water color, which looks sharper than a white pixel. Use this sparingly.
- **Kitty image specifics.** Simulate and shade at the image resolution, then send the frame. Water fills the whole image, so a rippling frame changes almost every pixel. Frame size and send rate, not the sim, set the limit. Keep the image small and let Ghostty scale it.

## 6. tmux implications

The water sim itself does not care about tmux. The output path does:

- **Half-block text** works under tmux as is (tmux supports 24-bit color with `Tc`/`RGB` set on the outer terminal). tmux parses and re-renders every cell, so the byte savings from quantization (section 5) matter even more inside tmux.
- **Kitty graphics** under tmux need `set -g allow-passthrough on` (tmux 3.3+), and every upload and placement must be wrapped in the tmux DCS passthrough envelope ([ranger PR using tmux passthrough for kitty](https://github.com/ranger/ranger/pull/3086), [tmuxai: allow-passthrough](https://tmuxai.dev/tmux-allow-passthrough/)). tmux does not track images, so pane switches, scrolling and redraws can leave images stale or misplaced. Kitty's Unicode placeholder placement is the known workaround for keeping images anchored to cells under tmux ([kitty discussion #4021](https://github.com/kovidgoyal/kitty/discussions/4021), [tmux issue #4902](https://github.com/tmux/tmux/issues/4902)).
- Practical consequence for water: design the shading so the half-block path looks good on its own (it always works, in or out of tmux), and treat the Kitty image path as the higher-resolution upgrade when not in tmux or when passthrough is on.

## 7. Frame order

```
every 1/60 s tick:
  apply drops and fish wakes into cur
  copy solid-cell neighbours (boundaries)
  step wave equation (cur, prev) and swap
  draw floor + ambient caustics + koi into underwater buffer
  for each display pixel: refraction sample, sim caustic, tilt, glint, depth tint
  quantize to palette
  hand the pixel grid to the renderer (half-block diff or kitty image)
```

Surface things like lily pads, floating food and turtles' backs draw after water shading, on top, unrefracted.

## Sources

- Hugo Elias 2D water algorithm, as written up by [Maciej Matyka](http://maciejmatyka.blogspot.com/2018/03/2d-water-algorithm.html) and [PixelEuphoria](https://pixeleuphoria.com/blog/index.php/2021/01/19/playing-around-with-a-2d-wave-algorithm/); damping values from [GameDev.net](https://www.gamedev.net/forums/topic/608562-hugo-elias-water-ripple-question/)
- Matthias Müller-Fischer, "Fast Water Simulation for Games Using Height Fields", GDC 2008: [slides](https://media.gdcvault.com/gdc08/slides/S6509i1.pdf), [transcript](https://archive.org/details/GDC2008Fischer)
- Jerry Tessendorf, [Interactive Water Surfaces (iWave)](https://people.computing.clemson.edu/~jtessen/reports/papers_files/Interactive_Water_Surfaces.pdf)
- Evan Wallace, [Rendering realtime caustics in WebGL](https://medium.com/@evanwallace/rendering-realtime-caustics-in-webgl-2a99a29a0b2c) and [WebGL Water](https://madebyevan.com/webgl-water/)
- [Kelvin wake pattern (Wikipedia)](https://en.wikipedia.org/wiki/Kelvin_wake_pattern)
- tmux and Kitty graphics: [ranger PR #3086](https://github.com/ranger/ranger/pull/3086), [tmuxai allow-passthrough](https://tmuxai.dev/tmux-allow-passthrough/), [kitty discussion #4021](https://github.com/kovidgoyal/kitty/discussions/4021), [tmux issue #4902](https://github.com/tmux/tmux/issues/4902)
- Timings: measured in this session with a small Rust benchmark (single thread, f32, `-O -C target-cpu=native`, Threadripper 7960X).
