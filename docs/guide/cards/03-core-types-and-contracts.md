# Core types and contracts

> What are the main types, and what does each function promise?

koi is built from a handful of nouns. A `School` holds the koi and the food, and moves them forward in small fixed steps. Each `Koi` keeps its state private and hands out a `Pose`, a copy of where it is, for drawing. A `Theme` is one resolved look, and a `Catalog` holds every theme. `Water` turns the pond into an image, and a `Poser` turns a pose into a koi sprite.

Around that core, `Config` is what the user wrote and `State` is what the pond remembers between runs. The audio thread takes an `Event` and answers with a `Status`. The terminal reports `Caps`, what it can draw, and `Input`, the keys and clicks. In the browser, one `Pond` holds all of it.

The same seed lays out the same pond. Units are plain numbers, so the types don't keep pixels, cells and seconds apart.

::: medium
### The types

| Type | Crate | What it is | Defined |
|---|---|---|---|
| `School` | koi-sim | The koi, food, bubbles and pending splashes | `crates/koi-sim/src/lib.rs:418` |
| `Koi` | koi-sim | One fish. Private state, three public fields for painting | `crates/koi-sim/src/lib.rs:276` |
| `Pose` | koi-sim | A `Copy` snapshot of one koi, for one frame | `crates/koi-sim/src/lib.rs:325` |
| `FoodKind` | koi-sim | The five foods | `crates/koi-sim/src/lib.rs:48` |
| `Theme` | koi-theme | One look: palette, light, style, scene | `crates/koi-theme/src/lib.rs:457` |
| `Catalog` | koi-theme | Every theme, built-in and user | `crates/koi-theme/src/lib.rs:476` |
| `Water` | koi-render | The pond surface, drawn as RGBA | `crates/koi-render/src/water.rs:34` |
| `Poser` | koi-render | Koi sprites, painted once, posed per frame | `crates/koi-render/src/koi.rs:69` |
| `Event`, `Status` | koi-audio | Messages to and from the audio thread | `crates/koi-audio/src/lib.rs:45` |
| `Caps`, `Input` | koi-term | What the terminal can draw, and what it sent | `crates/koi-term/src/lib.rs:499` |
| `Config`, `State` | koi (binary) | Settings, and what's remembered | `src/config.rs:138`, `src/state.rs:10` |
| `Pond` | koi-web | Everything above, for one canvas | `crates/koi-web/src/lib.rs:110` |

In the terminal, `Scene` groups the per-window parts (`src/main.rs:203-210`).

### Simulating

`School::new(w, h, count, seed)` builds `count` koi on a `w` x `h` grid, and its doc promises that the same seed gives the same school (`crates/koi-sim/src/lib.rs:498-500`). `School::step(&mut self)` takes no time. It always advances by `DT`, a sixtieth of a second (`crates/koi-sim/src/lib.rs:10-12`, `crates/koi-sim/src/lib.rs:681-683`). Deciding how many steps a frame needs is the caller's job.

`Koi::pose(&self) -> Pose` returns a value, not a reference (`crates/koi-sim/src/lib.rs:317`). `Pose` is `Copy`, so the loop can keep last step's poses after the school moves on, and blend towards the new ones with `Pose::lerp` (`crates/koi-sim/src/lib.rs:351`).

### Drawing

`Water::render(&mut self, shadows: &[Shadow]) -> &[u8]` returns straight RGBA, `w` x `h` (`crates/koi-render/src/water.rs:496-498`). The bytes are Water's own buffer, lent out. While the caller holds them, the borrow checker won't let anything else touch the `Water`. The web page copies them out before moving on (`crates/koi-web/src/lib.rs:214`). `Poser::pose` lends its sprite the same way (`crates/koi-render/src/koi.rs:251`).

### Loading what can fail

@excerpt src/config.rs:303-306 mark=306

Problems come in two sizes. A bad key is a warning in the `Vec<String>`, and its default stays. Only a file that can't be read or parsed is an `Err`. At startup that exits (`src/main.rs:122-127`). On a hot reload it becomes a message and the running config stays (`src/main.rs:595`). The theme code splits things the same way. `Catalog::load` can't fail at all and returns its warnings beside the catalog (`crates/koi-theme/src/lib.rs:494`). `Catalog::resolve` returns `Result<Theme, String>` for a missing or broken theme (`crates/koi-theme/src/lib.rs:542`), and a `Theme` that did resolve still carries its own `warnings` (`crates/koi-theme/src/lib.rs:468-469`).

`State::load` can't fail. A missing file means nothing is remembered (`src/state.rs:27-28`).

### Reading the terminal

`read_input(timeout) -> Vec<u8>` waits up to `timeout` and returns up to 4 KiB. Its doc warns that an escape sequence can be split across two reads (`crates/koi-term/src/lib.rs:572-574`). `parse_input` is shaped around that:

@excerpt crates/koi-term/src/lib.rs:800-804 mark=804

The loop keeps a `pending` buffer. It appends each read, parses, and removes only the bytes that were used:

@excerpt src/main.rs:443-451 mark=443,450-451

::: check `parse_input` returns `(events, used)`. What goes wrong if the caller drops the unused bytes instead of keeping them?
Say a read ends inside a mouse report, after `\x1b[<0;1`. `parse_input` stops there and leaves it unused. Dropped, the start is lost, and the next read begins `2;7M`. With no `\x1b` in front, each byte parses as a key (`crates/koi-term/src/lib.rs:860`), and `2` is a food key by default (`src/config.rs:134`). A click becomes a food change. `pending.drain(..used)` at `src/main.rs:451` is what prevents that, and the test `split_reads_carry_over` splits one input at every byte to check it (`crates/koi-term/src/lib.rs:1018-1033`).
:::
:::

::: high
### Contracts only a comment holds

`Poser` refers to koi by index. Its doc says the school's koi must not change after it's built, because index `k` means `school.fish[k]` (`crates/koi-render/src/koi.rs:192-193`). `recolor` asks for "the one this poser was made for" (`crates/koi-render/src/koi.rs:207`). Nothing in the types ties a `Poser` to its `School`. The Encapsulation card follows what breaks when that's ignored.

::: inferred
A lifetime could tie them, as a `Poser` that borrows the `School`. But the loop steps the school mutably every tick, so a shared borrow couldn't be held across frames. An index plus a comment is the shape that fits.
:::

`School::drop_food` returns how many pieces landed. The doc says that when none land, the caller should skip the chime (`crates/koi-sim/src/lib.rs:565-568`). The loop does, with `> 0` (`src/main.rs:513`). A `usize` can't say that. `School::pet` is clearer: `Option<usize>` means "this koi, or none in reach" (`crates/koi-sim/src/lib.rs:618`).

### Units in plain numbers

Every coordinate is an `f32` or a `usize`, but there are several unit systems:

- **Simulation units.** Where koi and food live. koi-sim calls them water pixels.
- **Water image pixels.** What `Water::render` draws. `per_sim` converts to them, and `Water::splash` does the conversion on the way in (`crates/koi-render/src/water.rs:428-432`).
- **Sprite pixels.** `Poser::new` takes `scale` as sprite pixels per water pixel (`crates/koi-render/src/koi.rs:191`).
- **Cells.** `Input` gives 1-based columns and rows. The loop turns them into simulation units itself (`src/main.rs:506-507`).
- **Body lengths.** `pet`'s `reach` is in BL (`crates/koi-sim/src/lib.rs:614-615`).
- **Time.** Seconds for `DT`, but the web page's `tick` takes milliseconds (`crates/koi-web/src/lib.rs:192`).

Only the doc comments carry the units, so they have to stay right. `School` works in water pixels, the simulation's grid, and in a pixel theme the water draws on its own art grid and converts (`crates/koi-sim/src/lib.rs:415-417`, `crates/koi-render/src/water.rs:25-27`). `build` sizes the two grids differently when the theme is pixel art (`src/main.rs:266-267`).

### Seeds

A seed is a `u64` into `School::new` and `Water::new`. The web constructor takes a `u32` and widens it (`crates/koi-web/src/lib.rs:144-147`). "0 means a new pond every start" is a config rule, not a type. `main` swaps 0 for the clock before anything sees it (`src/main.rs:380-383`).

::: inferred
koi-sim has no clock and no other source of randomness. Its only random state is the `rng` field, seeded as `seed | 1` (`crates/koi-sim/src/lib.rs:503`). That keeps xorshift away from 0, where it would stay 0 forever. It also means seeds 2 and 3 give the same school. The water's layout mixes the seed in differently (`crates/koi-render/src/water.rs:846`), so those two ponds likely differ in their stones but not their koi.
:::

### `String` errors are for people

Every `Result<_, String>` above holds a sentence for a toast, the status line or stderr. `State::save` says so (`src/state.rs:32-33`), and `resolve` says its error names the file and key (`crates/koi-theme/src/lib.rs:540-541`). A caller can't match on what went wrong, only show it or fall back. The web page falls back: an unknown theme name becomes the root theme (`crates/koi-web/src/lib.rs:146`).

### Where types do carry meaning

`FoodKind` is a `Copy` enum. Its name, its index and every number about it (`spec`) are a `match` each (`crates/koi-sim/src/lib.rs:91-92`), so a sixth food won't compile until all of them cover it. `ALL` is the exception. It's a plain array that fixes the order of the HUD keys (`crates/koi-sim/src/lib.rs:66-67`), and nothing makes it list a new kind.

`Mood` is private, and each variant holds only what that mood needs (`crates/koi-sim/src/lib.rs:233-257`). `Approach` holds a food `id`, not a position in `school.food`.

::: inferred
Eaten food is removed from `school.food`, so positions shift. The ids come from a counter that never reuses one soon (`crates/koi-sim/src/lib.rs:589-590`), so a koi's target stays the right piece.
:::

`Caps::sixel` is `Option<Sixel>` (`crates/koi-term/src/lib.rs:505`). A terminal's sixel limits only exist when it supports sixel at all. And the HUD answers each input with a `Reply` (`src/hud.rs:51-60`). `Pass`, `Took` or `Act(Action)` tells the loop whether to handle the input, count it as activity, or do something.
:::
