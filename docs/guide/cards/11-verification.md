# Verification

> How do we know koi works, and what would catch a mistake?

koi has 61 tests, spread over every crate, and `just test` runs them all. CI runs them on Linux, macOS and Windows for every push.

Most tests check behaviour you could describe out loud, and their names say it: `a_pellet_draws_one_koi`, `calm_limits_hold`. They run the simulation over many random seeds rather than trusting one lucky run.

One test is unusual. It draws the same frame on the GPU renderer and the CPU renderer, for eight themes, and fails if any pixel differs by more than 2 out of 255. That's a check of accuracy, not just pass or fail.

What no test covers is how koi looks and sounds in a real terminal. That's checked by screenshots on a virtual display, and by ear.

::: medium
### Running them

The simulation's tests need no GPU, terminal or audio device, so they run anywhere in well under a second:

@run cargo test -q -p koi-sim --lib 2>&1 | grep "test result"

### A behaviour test

@excerpt crates/koi-sim/src/lib.rs:1194-1214 mark=1198,1212

It drops pellets into 20 differently seeded ponds and counts how many koi come for them. The assertion allows one extra, because a koi beaten to its pellet may turn to another. A test with a single seed could pass by luck.

### The parity test

The GPU and CPU renderers are two implementations of the same pictures, one in shaders and one in Rust. This test keeps them the same:

@excerpt crates/koi-render/tests/parity.rs:44-56 mark=44,56

The GPU half runs only when `VK_ICD_FILENAMES` points at a Vulkan driver. `just test` points it at lavapipe, a Vulkan driver that runs on the CPU, when it's installed (`justfile:5`), and Linux CI does the same. So no real GPU is needed.

::: check You change how the water shimmers in `water.wgsl`, the GPU shader, and forget the CPU path. Where is that caught?
On Linux CI, and locally if lavapipe is installed: the parity test fails on the first theme whose water changed. On macOS and Windows CI it isn't caught, because `VK_ICD_FILENAMES` isn't set there and the GPU half is skipped.
:::

### What CI runs

| Job | What it checks |
|---|---|
| linux | formatting, lints, all tests with the parity test's GPU half, and the docs build |
| macos, windows | lints and all tests, without the GPU half |
| macos-screenshot | runs koi in Ghostty on a Mac and saves a screenshot |
| pages | builds the web page and runs it in headless Chrome, as a desktop and as a phone |
:::

::: high
### Checked by eye and ear

Some things can't be asserted:

- **How the pond looks.** `scripts/vshot.sh` runs koi in Ghostty on a private virtual display and saves a screenshot and a short clip, so nothing opens on your screen. The macOS CI job does the same on a Mac, as a best effort.
- **The web page.** `scripts/site-smoke.mjs` opens every scene in headless Chrome and fails on any console error. It also taps the music stone and fails unless music plays, and taps the scene and time stones and fails unless the scene changes.
- **The sound.** The synth's levels come from a listening test, and the code records its date and outcome (`crates/koi-synth/src/lib.rs:75`).

### Random input

`parse_input` reads raw bytes from the terminal, so a test throws 5000 random byte strings at it, half of them made of the bytes escape sequences use, and checks it never panics or claims more bytes than it was given (`crates/koi-term/src/lib.rs:1039`).

### Gaps

- Windows is compiled and tested in CI, but nobody has watched koi run there.
- There are no benchmarks, so a slowdown wouldn't fail anything.
:::
