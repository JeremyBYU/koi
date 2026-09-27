# Purpose and glossary

> What is koi for, and what do its own words mean?

koi is a small koi pond you leave running. A few koi swim in a pond. You can drop food, pet a koi, and change the scene or the time of day. Quiet music plays over a generated water sound. There is no score and nothing to win.

It runs in a terminal, and draws best in one that can show real images, like Ghostty or Kitty. Other terminals get a rougher picture made of sixel images or coloured characters. The same pond also runs in a web page.

The code has its own words for the parts of the picture. The two you meet first are the **tier**, which is how a frame gets to the terminal, and **water pixels**, the coarse grid the whole simulation lives on.

::: medium
### The words

| Word | What it means | Where |
|---|---|---|
| tier | How the pond reaches the terminal: `kitty`, `kitty-direct`, `sixel` or `blocks`, best first | `src/layers.rs:29` |
| protocol | The setting and flag that pick a tier. The same four names, plus `auto` | `src/config.rs:164` |
| backend | Where images are rendered: `gpu` or `cpu` | `src/config.rs:157` |
| water pixels | The simulation's grid, `water_px` pixels per cell width. Koi, food and splashes all use it | `crates/koi-sim/src/lib.rs:2` |
| screen pixels | The terminal's own pixels, which the images are scaled up to | `docs/ARCHITECTURE.md:59` |
| art grid | A pixel theme's grid: `pixel_px` screen pixels to one art pixel | `crates/koi-theme/src/lib.rs:210` |
| painted, pixel | The two kinds of theme. `pixel_px = 0` means painted | `crates/koi-theme/src/lib.rs:210` |
| family | Themes that are the same place at different times | `crates/koi-theme/src/lib.rs:446` |
| time | A theme's time of day, dawn to night, in order | `crates/koi-theme/src/lib.rs:402` |
| school | All the koi, plus the food and bubbles in the water | `crates/koi-sim/src/lib.rs:418` |
| pose | Everything a drawn koi depends on at one step. Frames blend two | `crates/koi-sim/src/lib.rs:325` |
| splash | A disturbance of the water surface. Negative `amount` is a koi's wake | `crates/koi-sim/src/lib.rs:14` |
| HUD stones | The controls along the bottom: music, food, scene, time, help | `src/hud.rs:64` |
| pill, tray, chip | The music stone, which shows the track title; the list of choices a stone opens; the label above a stone | `src/hud.rs:94` |
| peek | A stone showing for a moment, with a chip, after something changed | `src/hud.rs:138` |
| toast | A message on the top line, 2.5 s, or 6 s for an error | `src/main.rs:308` |
| bed | The steady low part of the ambient sound: the drone and the water's rumble | `crates/koi-synth/src/lib.rs:132` |

### Family and time, in the program's own output

`--list-themes` prints each theme's id, its name, then its family and time. Look at the third column: four themes share the family `garden`, one per time of day.

@run ./target/release/koi --list-themes | sed -n 1,8p

`t` steps to the next family and `l` to the next time in the same family (`crates/koi-theme/src/lib.rs:663`, `crates/koi-theme/src/lib.rs:696`).

::: check You press `t` while on Moonlit Pond. Which theme do you get, going by the output above?
Cedar Shade at Night. `t` moves to the next family, `cedar-shade`, and keeps the current time, night, when that family has it (`crates/koi-theme/src/lib.rs:681-685`).
:::
:::

::: high
### Words that change meaning

**Scene** means three things. To a player it is a family: `t` is "next scene", and the README counts eight. In `koi-theme`, `Scene` is the `[scene]` section of a theme file: floor, rim, pads, weather (`crates/koi-theme/src/lib.rs:367`). In `main.rs`, `Scene` is the set of live objects for one pond: water, school, poser and layers (`src/main.rs:203`). A search for `Scene` finds all three.

**Backend** is two enums. The public one in the config picks GPU or CPU (`src/config.rs:157`). A private one inside `Water` holds that path's state (`crates/koi-render/src/water.rs:62`).

**Water pixels** shift inside `koi-render`. In `koi-sim` they are the simulation grid. Inside `Water`, "water pixels" are the water image's own pixels, which in a pixel theme are art pixels. `per_sim` converts between the two:

@excerpt crates/koi-render/src/water.rs:25-27

**Screen pixels** are not always the screen's. In tmux the frame's pixels are `fish_px` to a cell, and in `blocks` a cell is two pixels tall (`src/main.rs:255-258`). The code downstream treats those as screen pixels anyway.

**Stones** are the HUD's controls, but also a kind of pond rim (`crates/koi-theme/src/lib.rs:279`).

**Pose** is a noun in `koi-sim` (`Pose`, a koi's state) and a verb in `koi-render`: `Poser::pose` paints a koi sprite from a `Pose` (`crates/koi-render/src/koi.rs:251`).

:::
