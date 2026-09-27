# Dependencies and licensing

> What does koi need to build and run, and are all its licenses compatible?

koi pulls in 133 crates from crates.io. Most come in through three direct dependencies: wgpu for the GPU renderer, rodio for playing music (it decodes with symphonia), and serde and toml for config and themes.

On Linux the finished binary needs one system library besides the C runtime: ALSA's `libasound.so.2`, for sound. Vulkan is optional. koi looks for it when it starts and uses the CPU renderer if it's missing.

koi itself is MIT or Apache-2.0, and almost everything it uses is too. There are two exceptions to know about. The symphonia audio decoders are MPL-2.0, which allows this use but asks that changes to those files be shared. And the three songs built into the binary are CC BY 4.0, which requires crediting Kevin MacLeod.

::: medium
### What each direct dependency is for

@excerpt Cargo.toml:20-31

| Crate | Why koi needs it |
|---|---|
| wgpu, bytemuck, pollster | The GPU renderer: device, buffers, and waiting on GPU work |
| rodio | Audio output, and decoding mp3 and Ogg Vorbis through symphonia, which it pulls in |
| serde, serde_json, toml | Reading config, themes, state and the loudness cache |
| rustix, signal-hook, windows-sys | Talking to the operating system: terminal modes, shared memory, signals |
| base64, miniz_oxide | Encoding images for the terminal's graphics protocol |

### Linked, or loaded when needed

A library is either linked, so the program won't start without it, or loaded at run time, so the program can carry on without it. The binary's own list of required libraries shows which is which:

@run readelf -d target/release/koi | grep NEEDED

The last four are the C runtime that every Linux program needs. ALSA is the only other one. Vulkan isn't on the list: wgpu opens `libvulkan.so.1` itself at startup, through the `libloading` crate. That's why koi still runs on a machine with no GPU driver.

::: check A friend runs the release binary on a fresh Linux install with no GPU driver. Does it start?
Yes. Vulkan isn't linked, so its absence only means the CPU renderer is used. The one library that must be there is `libasound.so.2`, which desktop Linux installs almost always have.
:::

### Licenses

| License | Crates | What it asks |
|---|---|---|
| MIT and/or Apache-2.0 | about 120 | Keep the copyright notice |
| Zlib, ISC, BSD, Unicode | a handful | Keep the notice; about the same as MIT |
| MPL-2.0 | the symphonia crates | Share changes to those files if you modify them |

Each release archive ships a `THIRD-PARTY.md` listing every crate and its license text. `scripts/release.sh:36` generates it for each target, so it matches what was actually built.

The music is data compiled into the binary:

@excerpt crates/koi-audio/src/lib.rs:115-121

CC BY needs credit wherever the music goes. The README, the `?` help card and `koi --help` all name the composer.
:::

::: high
### Build-time needs are separate

Building needs more than running. `libasound2-dev` supplies the headers and the `.so` link name for ALSA. A release needs zig and cargo-zigbuild, which cross-compile for aarch64 and target an old glibc (2.17), so one binary runs on most Linux systems. The web page needs `wasm-bindgen-cli`, pinned to the version in `Cargo.lock` (`justfile:33-35`).

### Nothing checks licenses automatically

`scripts/third-party.sh` lists licenses but doesn't judge them, and CI has no license check. A new dependency under the GPL, for example, would be listed in the next release's `THIRD-PARTY.md` and nothing would fail.

::: inferred
The MPL obligation only applies if someone changes symphonia's source files. koi uses the crates unmodified from crates.io, so today there's nothing extra to publish.
:::
:::
