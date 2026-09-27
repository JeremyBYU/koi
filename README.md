# koi

A small koi pond simulator, meant to be calming and simple.

There are a few koi in a pond. You can feed them, pet them, and change the scene or the time of day. Some quiet music plays with a bit of water sound underneath. There's no score and nothing to win.

It runs in the terminal, and you can also [try it in the browser](https://jeremybyu.github.io/koi/), on a computer or a phone.

![Summer Garden: koi gather around a treat in the middle of the pond, with the HUD stones along the bottom](docs/img/summer-garden.jpg)

<img src="docs/img/feeding.webp" alt="Flakes land in the pond and the koi turn and gather to eat them" width="800">

## Install

koi runs on Linux and macOS, and on Windows as an experiment. It looks best in [Ghostty](https://ghostty.org) or [Kitty](https://sw.kovidgoyal.net/kitty/), which can show real images. Other terminals get a rougher picture (see [Terminals](#terminals)).

Download the archive for your system from the [latest release](https://github.com/JeremyBYU/koi/releases/latest), unpack it and run `koi`:

```sh
tar xzf koi-*-x86_64-linux.tar.gz
./koi-*-x86_64-linux/koi
```

| System | Archive |
|---|---|
| Linux on Intel or AMD | `koi-<version>-x86_64-linux.tar.gz` |
| Linux on ARM | `koi-<version>-aarch64-linux.tar.gz` |
| macOS (Apple silicon and Intel) | `koi-<version>-macos.tar.gz` |
| Windows on Intel or AMD (experimental) | `koi-<version>-windows-x86_64.zip` |

It's a single 14 MB file with three songs built in. Put it anywhere on your `PATH`, like `~/.local/bin`. You don't need a GPU: koi uses one if it can (Vulkan, Metal or DX12) and otherwise draws on the CPU, and it looks the same either way.

- On Linux it needs glibc 2.17 or newer and `libasound.so.2`, which every desktop has.
- On macOS the binary is not signed. If you downloaded the archive with a browser, clear the quarantine flag once: `xattr -d com.apple.quarantine koi`.
- Windows support is new and hasn't been tried on a real screen yet. Use Windows Terminal 1.22 or newer; other terminals get the blocky version. The config goes in `%APPDATA%\koi-pond`, and the music pack unpacks into `%LOCALAPPDATA%\koi-pond`, giving `%LOCALAPPDATA%\koi-pond\music`.

### More music

koi comes with three songs. The full set of 19, all by Kevin MacLeod, is a separate [music pack](https://github.com/JeremyBYU/koi/releases/tag/music-v1) of about 170 MB. To install it:

```sh
mkdir -p ~/.local/share/koi-pond
curl -L https://github.com/JeremyBYU/koi/releases/download/music-v1/koi-music.tar.gz | tar xz -C ~/.local/share/koi-pond
```

You can also set `audio.music_dir` in the config to any folder of mp3 or ogg files.

### Build from source

```sh
sudo apt install libasound2-dev    # ALSA headers, on Debian and Ubuntu only
cargo build --release
target/release/koi
```

This needs Rust 1.90 or newer.

## Playing

Click the water to drop food. Click and hold on a koi to pet it.

| Key | What it does |
|---|---|
| click, `f` | Drop food where you click, or somewhere random |
| click and hold a koi | Pet it. Drag and it follows |
| `p` | Call the nearest koi over to be petted |
| `1` to `5` | Pick the food: pellets, flakes, petals, seeds, treat |
| `t`, `T` | Next or previous scene |
| `l`, `L` | Later or earlier time of day |
| `n`, `m`, `+`, `-` | Next track, mute, volume |
| `Tab` | Hide or show the HUD |
| `?` | Show the help card |
| `q`, Ctrl-C | Quit |

`koi --help` lists every key and option. koi remembers your theme and volume between runs.

### Food

Each food does something a little different.

| Key | Food | What the koi do |
|---|---|---|
| `1` | Pellets | A koi rushes over for each pellet and gulps it. |
| `2` | Flakes | Several koi come over slowly and graze. |
| `3` | Petals | A koi mouths each one once, then ignores it. |
| `4` | Seeds | They sink in a spiral, and koi follow them down. |
| `5` | Treat | Every koi circles it, and they take turns biting it. |

### Petting

Click and hold on a koi and it turns around and nuzzles your cursor, and blows a few bubbles. Drag and it follows. When you let go it hangs around for a bit, then swims off. Sometimes another koi comes over to see what's going on.

<img src="docs/img/petting.webp" alt="A koi glides to a hand in the middle of the pond, nuzzles it and blows a few bubbles" width="800">

### The HUD

The stones along the bottom are the controls: music, food, scene, time of day and help. Click one to see the choices.

![The HUD with the food tray open](docs/img/hud-food.jpg)

`Tab` hides them. With `hud.show = "auto"` in the config they stay hidden and only pop up for a moment when you use their key.

## Themes

There are eight scenes, six painted and two in pixel art, and each one comes at a few times of day. `t` changes the scene and `l` changes the time of day. The garden has dawn, noon, evening and night (Morning Mist, Summer Garden, Evening Garden and Moonlit Pond), the pixel garden has noon and dusk, and the rest have noon, dusk and night. At night the crickets come out.

| Morning Mist | Evening Garden | Moonlit Pond |
|---|---|---|
| ![Morning Mist](docs/img/morning-mist.jpg) | ![Evening Garden](docs/img/evening-garden.jpg) | ![Moonlit Pond](docs/img/moonlit-pond.jpg) |
| **Rainy Afternoon** | **Ink and Vermilion** | **Petal Spring** |
| ![Rainy Afternoon](docs/img/rainy-afternoon.jpg) | ![Ink and Vermilion](docs/img/ink-and-vermilion.jpg) | ![Petal Spring](docs/img/petal-spring.jpg) |

The pixel themes:

| Hillside Summer | Lantern Dusk | Pocket Moss |
|---|---|---|
| ![Hillside Summer](docs/img/hillside-summer.png) | ![Lantern Dusk](docs/img/lantern-dusk.png) | ![Pocket Moss](docs/img/pocket-moss.png) |

### Your own theme

A theme is a TOML file with just the things it changes. Save this as `~/.config/koi-pond/themes/autumn-evening.toml` and it shows up when you press `t`:

```toml
name = "Autumn Evening"
description = "Evening Garden with maple leaves on the water."
extends = "evening-garden"

[palette]
koi_red = "#D9481F"

[scene]
foliage_kind = "maple"
petal_kinds = ["maple", "leaf"]
petals = 12
```

If you edit a theme while koi is running, the pond updates right away. If there's a mistake you'll get a warning at the top and the last working version stays. [docs/THEMES.md](docs/THEMES.md) explains every setting, and [themes/summer-garden.toml](themes/summer-garden.toml) has them all with their defaults.

## Configuration

You don't need a config file. If you want to change something, this writes out the full config with a comment on each setting (on Windows the folder is `%APPDATA%\koi-pond`):

```sh
mkdir -p ~/.config/koi-pond
koi --print-default-config > ~/.config/koi-pond/config.toml
```

For example:

```toml
[pond]
koi = 8
calmness = 1.5     # lazier turns and longer glides

[render]
backend = "cpu"

[audio]
volume = 0.4
```

A bad value just gives you a warning and the default gets used. Most changes take effect while koi is running. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#config-keys) has a table of every setting.

## Terminals

When it starts, koi checks what your terminal can draw and picks the best option. Press `d` to see which one it's using.

| Protocol | Terminals | Frames per second | What you see |
|---|---|---|---|
| `kitty` | Ghostty, Kitty | up to 60 | The full pond. The koi, water and food are separate images, sent through shared memory. |
| `kitty-direct` | Ghostty or Kitty over SSH | up to 30 | The same pond, with the images compressed and sent inline, since shared memory does not reach the other machine. |
| `sixel` | xterm, foot, mlterm, WezTerm | up to 20, 30 in a pixel theme | The full pond as one image, redrawn where it changed, in 256 colours. The bottom row is plain coloured cells. |
| `blocks` | any other terminal, such as GNOME Terminal | up to 60 | The pond at two pixels per character cell, drawn with coloured half-block characters. |

WezTerm answers the Kitty queries, but it falls behind and garbles the koi, so koi uses sixel there. xterm has sixel only when started as a VT340 with enough colours, such as `xterm -ti vt340 -xrm 'XTerm*numColorRegisters: 256'`. A terminal that answers nothing gets blocks after 2 seconds.

`koi --protocol NAME` picks one for a run, whatever the config says, and `render.protocol` in the config picks one for good. A protocol the terminal did not say it has is still tried, with a warning. If the terminal keeps refusing Kitty images, or cannot keep up with sixel or inline images, koi switches to the next protocol and says so on the top line.

## tmux

koi runs inside tmux 3.3 or newer when tmux passes images through to the terminal. Add this line to your tmux config (`~/.tmux.conf` or `~/.config/tmux/tmux.conf`), and run it once as `tmux set -g allow-passthrough on` for the server that is already running:

```
set -g allow-passthrough on
```

The pond then stays in its pane through splits and window switches. Without the setting, the Kitty images never reach the terminal, so koi draws with blocks and shows a warning with the line above. Inside tmux each frame is one image of the whole pane, so it costs more than running directly in the terminal. koi does not use sixel inside tmux, and draws with blocks when the terminal outside has no Kitty graphics. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#tmux) explains how it works.

## How it works

koi runs the simulation at 60 steps a second. Each frame it draws the water as one low-resolution image and each koi as its own small image, and sends only what changed. In Ghostty and Kitty the images go through shared memory, so very little goes through the terminal itself. The drawing runs on the GPU with wgpu, and there's a CPU version of the same code that a test keeps in line with it.

It runs at up to 60 frames a second while you're using it, and drops to 8 when the window is in the background and nothing is happening. [docs/PERFORMANCE.md](docs/PERFORMANCE.md) has the numbers.

The web version is the same Rust code compiled to WebAssembly, drawn on a canvas.

| Crate | What it does |
|---|---|
| `koi-sim` | The koi and the food: steering, moods, feeding and petting. No GPU, terminal or audio code. |
| `koi-theme` | Reads themes, resolves `extends` and steps through scenes and times. |
| `koi-render` | Renders the water and the koi on the GPU or the CPU, and paints food sprites and HUD stones. |
| `koi-term` | Raw mode, asking the terminal what it can draw, input parsing and sending Kitty images through shared memory or inline. |
| `koi-synth` | The generated ambient layer, the chimes and the petting sound. |
| `koi-audio` | Music with crossfades, and the ambient layer on the audio thread. |
| `koi-web` | The pond for the web page in `site/`, compiled to WebAssembly. |
| `koi-pond` (`src/`) | The `koi` binary: the frame loop, config, HUD and image layers. |

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) goes through the frame loop, each crate's API and every test. [docs/STYLE.md](docs/STYLE.md) covers the art direction.

## Development

The [development guide](https://jeremybyu.github.io/koi/guide/) walks through how koi is put together, one topic at a time: the frame loop, the main types, configuration, dependencies, tests, releases and more. Read it in about 10 minutes for the outline, or an hour or more for the detail. Its source is in [docs/guide](docs/guide/).

With [just](https://github.com/casey/just), `just` lists the common tasks: `just test`, `just check` (what CI runs), `just run`, and `just serve` for the web page. Without it:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

The tests that compare the GPU and CPU renderers run when `VK_ICD_FILENAMES` names a Vulkan driver. lavapipe, a Vulkan driver that runs on the CPU, works anywhere:

```sh
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test --workspace
```

`scripts/build-site.sh` builds the web page into `target/site` (it needs `rustup target add wasm32-unknown-unknown`, the matching `wasm-bindgen-cli`, ffmpeg and jq), and `node scripts/site-smoke.mjs` checks it in headless Chrome. Serve it with `python3 -m http.server -d target/site`.

`scripts/vshot.sh` runs koi in Ghostty on a private virtual display and saves a screenshot and a short clip, without anything appearing on your screen. `scripts/release.sh` builds the release archives for x86_64 and aarch64 with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), linked against glibc 2.17. `scripts/release.sh --music` also packs the full music set.

## Credits and license

The music is by Kevin MacLeod ([incompetech.com](https://incompetech.com)), licensed under [Creative Commons: By Attribution 4.0](https://creativecommons.org/licenses/by/4.0/). [assets/music/CREDITS.md](assets/music/CREDITS.md) lists every track. The pixel themes use Resurrect 64 by Kerrie Lake and SLSO8 by Luis Miguel Maldonado.

koi is licensed under either the [MIT license](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE), at your option.
