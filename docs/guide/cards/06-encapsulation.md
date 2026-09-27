# Encapsulation

> Can you change one thing in one place?

Most of koi's types keep their workings private and show callers only a few methods. `Water` is the clearest case. When it's built, it decides whether the GPU or the CPU runs the waves, and it keeps that choice in a private field. Either path can change without touching a caller.

`School`, which holds the koi and the food, is the exception. Its lists of koi, food, bubbles and splashes are public fields, and so are its speed and calmness. Callers reach in. The terminal sets the speed directly, and the splashes are moved from the school to the water by code outside the simulation, in `koi-render`. So changing how splashes are handed over means editing two crates.

::: medium
### What stays hidden

`Water` has two public fields, its width and height. The rest is private, including the backend:

@excerpt crates/koi-render/src/water.rs:59-65 mark=59,62-65

Callers build it with `Water::new` and then call `splash`, `step` and `render`. Each of those matches on the backend inside the file (`crates/koi-render/src/water.rs:448`, `crates/koi-render/src/water.rs:556`).

`Koi` hides more. Its position, heading, hunger and mood are private. A renderer can read three fields it needs to paint the fish once: length, variety and pattern seed (`crates/koi-sim/src/lib.rs:285-290`). For each frame it asks for a `Pose`, a copy of where the koi is now (`crates/koi-sim/src/lib.rs:317`). The steering code can be rewritten without another crate noticing.

`Audio` hides its thread. The sender and the thread handle are private, and the one public field is the channel that status messages come back on (`crates/koi-audio/src/lib.rs:86-91`). Callers can only `send` an `Event`.

`Layers` hides how a finished frame reaches the terminal: sixel, half blocks, or a Kitty image inside tmux. That's a private `Sink` enum (`src/layers.rs:152`).

### What's open

@excerpt crates/koi-sim/src/lib.rs:415-433 mark=422,424,426,429,431,433

`koi-sim` depends on nothing, so the school can't give splashes to the water itself. `advance` in `koi-render`, which both front ends call, drains them after each step:

@excerpt crates/koi-render/src/frame.rs:19-22 mark=20-22

The terminal also writes `speed` and `calmness` straight after building the school (`src/main.rs:278-279`) and again when config.toml reloads (`src/main.rs:588`).

| To change | You touch |
|---|---|
| How the CPU shades the water | `water.rs`, and `water.wgsl` to keep the GPU the same |
| How a koi steers | `koi-sim` only |
| How splashes reach the water | `koi-sim` and `advance` in `koi-render` |
| What `speed` means | `koi-sim`, and the two places `src/main.rs` sets it |

::: check The pond is running and you push a new koi onto `school.fish`. It's a public `Vec`, so the compiler allows it. What goes wrong?
`Poser` painted one body per koi when it was built (`crates/koi-render/src/koi.rs:396-400`). Its doc says the school's koi must not change afterwards (`crates/koi-render/src/koi.rs:192-193`). Only that comment guards the rule.

::: inferred
The new koi's index has no body, so `self.bodies[k]` in `pose` would index past the end and panic (`crates/koi-render/src/koi.rs:255`).
:::
:::
:::

::: high
### Open fields that carry no rules

Some types are plain data, with every field public: `Caps`, `Grid`, `Pose`, `Splash`, the audio `Settings`. There's nothing to protect in them, and callers use that. When a tier fails, the loop switches off the capability that failed by writing to `caps` directly (`src/main.rs:662-667`).

`School` mixes the two kinds. Its random state and whose turn it is at the treat are private, and only its methods change them (`crates/koi-sim/src/lib.rs:434-440`). Its lists are open. The defaults for `speed` and `calmness` are 1.0 (`crates/koi-sim/src/lib.rs:555-556`). The web page never sets them, so the browser pond always swims at those.

### Private inside a crate, shared across files

`koi-render` keeps every module private and chooses what leaves the crate with `pub use` (`crates/koi-render/src/lib.rs:15-19`). The `Gpu` handle shows the middle ground. Its device and queue are `pub(crate)`, so the water and the koi code can both build on them, and no other crate can (`crates/koi-render/src/gpu.rs:5-6`). Outside the crate, `Gpu` is something you make with `Gpu::new` and pass along.

### One rule, asked in two places

Whether a tier draws everything into one frame is a method on `Tier`:

@excerpt src/layers.rs:64-68

`Layers::new` asks it to decide whether it needs a `Sink` (`src/layers.rs:233`), and `build` in main.rs asks it to size the koi sprites (`src/main.rs:285`). Both depend on one answer, so a new tier changes it in one place.

`Tier` itself is public, and main.rs matches on it in several places. The next card follows what that means for adding one.
:::
