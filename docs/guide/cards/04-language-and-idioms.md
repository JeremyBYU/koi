# Language and idioms

> Which language features carry the design?

koi is Rust, edition 2024, and it sticks to a small, concrete part of the language. It defines no traits of its own. Where a program might hide choices behind an interface, koi uses an enum and a `match`: which tier draws the frame, whether the water runs on the GPU or the CPU, what the audio thread is asked to do.

Two other habits run through the code. Anything that might be missing, like the GPU or the audio device, is an `Option`, and the code carries on without it. And cleanup belongs to ownership: the terminal is restored and the shared-memory files are removed when their owners are dropped.

::: medium
### Enums, matched in full

A `match` on an enum must cover every variant, or the crate doesn't compile. koi leans on that. The four tiers are one enum, and each fact about a tier is a `match`:

@excerpt src/layers.rs:44-51

The GPU and CPU water paths work the same way. `Water` holds a private `enum Backend { Gpu(..), Cpu(..) }` (`crates/koi-render/src/water.rs:62-65`), and each method matches on it. Callers never see which one runs.


### `Option` for what may be absent

The GPU is the clearest case. A failure to get one becomes a warning and `None` (`src/main.rs:163-171`). From there on, the code takes `Option<&Gpu>` and matches it where it matters, for example in `Water::new` (`crates/koi-render/src/water.rs:290-292`). The audio handle is an `Option<&Audio>` the same way (`src/main.rs:327`).

::: check Someone writes a new function that needs the GPU and is handed `Option<&Gpu>`. Can they forget the case where there is none?
No. An `Option<&Gpu>` isn't a `&Gpu`, so the compiler rejects using it as one. They have to `match` it, or say what happens on `None` with something like `if let Some(gpu)`. The only way to skip the question is `unwrap` or `expect`, which is visible in review and panics on a machine with no GPU.
:::

### Ownership does the cleanup

`run` holds a `Terminal` it never uses (`src/main.rs:375`). Its `Drop` restores the terminal (`crates/koi-term/src/lib.rs:86-89`), on return, on error and on a panic. `ShmRing` does the same for its shared-memory files (`crates/koi-term/src/lib.rs:732`).

The audio thread stops through ownership too. `shutdown` takes `self`, drops the sending end of the channel, and waits:

@excerpt crates/koi-audio/src/lib.rs:109-112

The thread sees the channel close and leaves its loop (`crates/koi-audio/src/lib.rs:238`). There is no stop message.

### `Result` and `?`

Errors are plain: `io::Result`, or `Result<_, String>` with a message written for the user. There is no error crate. `?` passes them up, and `map_err` adds the file name first, as in `config::load` (`src/config.rs:311-312`). `main` prints whatever reaches it and exits with a failure code (`src/main.rs:194-199`).
:::

::: high
### Threads without shared state

There are two kinds of threads. The audio thread is long-lived and talks only through `mpsc` channels (`crates/koi-audio/src/lib.rs:97-99`). The loop reads its replies with `try_recv`, so it never waits on sound.

Painting uses scoped threads. `std::thread::scope` lets each thread borrow its own `chunks_mut` band of the output, with no `Arc` or lock, and joins them all before returning (`crates/koi-render/src/water.rs:1092`). The web build has no threads, so every spawn goes through one small function that runs the work inline on wasm:

@excerpt crates/koi-render/src/lib.rs:21-28 mark=23-24

`cfg!` is an ordinary `bool`, so both branches are compiled and type-checked on every target.

### `unsafe` is fenced in one crate

Every crate but `koi-term` starts with `#![forbid(unsafe_code)]`, for example `crates/koi-sim/src/lib.rs:6`. In `koi-term`, the Unix `unsafe` blocks map shared memory, write into it and unmap it, and each has a `SAFETY` comment saying why it holds (`crates/koi-term/src/lib.rs:737-738`). The Windows console calls in `sys_windows.rs` are `unsafe` too.

### Data compiled in

The built-in themes and songs are `const` arrays of `include_str!` and `include_bytes!` (`crates/koi-theme/src/lib.rs:22`, `crates/koi-audio/src/lib.rs:117`), so the binary needs no files beside it. A debug build still prefers the file on disk, through `cfg!(debug_assertions)` (`crates/koi-theme/src/lib.rs:500`).

### Edition 2024

The one edition 2024 feature in daily use is the let chain, `if let ... && let ...`:

@excerpt src/main.rs:134-135

Let chains are only stable in edition 2024, so `edition = "2024"` and `rust-version = "1.90"` (`Cargo.toml:8-9`) are not just defaults. `let ... else` is common too, for skipping a bad config key (`src/config.rs:320`).

### A panic hook that checks the thread

`Terminal::enter` installs a panic hook that restores the terminal only if the panic is on the thread that entered (`crates/koi-term/src/lib.rs:65-70`). A panic on the audio thread leaves the pond running, so the terminal must stay as it is.

::: inferred
Theme switches that need a new grid move the whole `Scene` into `build(.., Some(scene), ..)` (`src/main.rs:617`). `build` keeps the water, school and last poses (`src/main.rs:270-274`). The old poser and layers are dropped at the end of that match arm, so the old ring's files go before the new ring is made.
:::
:::
