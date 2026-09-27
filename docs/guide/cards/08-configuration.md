# Configuration

> What can you change, where do you change it, and which setting wins?

Settings come from three places: config.toml, a few command-line flags, and state.toml, a small file the game writes itself to remember your last scene and volume. Themes are separate TOML files.

Every default lives in one commented block of TOML inside the program. `koi --print-default-config` prints it, and your config.toml is merged over that same text.

A flag beats the files. The scene and volume you last picked in the game beat config.toml. For the scene, that lasts until you change the theme in config.toml. A bad value never stops the game: it shows a warning and keeps the default. Most changes to config.toml apply within a second, while koi is running.

::: medium
### Which setting wins

| Setting | First | Then | Then |
|---|---|---|---|
| Theme | `--theme` | state.toml, if config.toml hasn't changed its theme since | `theme.name`, then the default |
| Renderer, drawing method | `--backend`, `--protocol` | `[render]` in config.toml | the default |
| Volume, mute | state.toml | `audio.volume` | the default |
| Everything else | config.toml | the default | |

::: check You pick Moonlit Pond with `t` and quit. config.toml says `summer-garden`. Which theme opens next time? And after you edit config.toml to say `evening-garden`?
Moonlit Pond first, since state.toml remembers it and config.toml still names the same theme as when it was saved. After the edit, Evening Garden, because config.toml changed. The rule is `State::theme_for` (`src/state.rs:47-49`).
:::

### The defaults are the documentation

This is the start of what `--print-default-config` prints. It's also, byte for byte, the base your file is merged over (`src/config.rs:307`), so an explanation can't drift from its value:

@run lang=toml ./target/release/koi --print-default-config | head -24

### How your file is merged

Each key in your file is tried on its own, in a full copy of the config. If it fits, it's kept. If not, it becomes a warning naming the key, and the default stays:

@excerpt src/config.rs:327-334

So a typo in one key never throws away the rest of your file. Only a file that isn't valid TOML at all is fatal.

### Where the files are

config.toml and your themes folder are in `~/.config/koi-pond`, state.toml in `~/.local/state/koi-pond`. The `XDG_*` variables move them, and Windows uses `%APPDATA%` and `%LOCALAPPDATA%`. One function works all of this out (`src/config.rs:285`).

state.toml is saved by writing a temporary file and renaming it over the old one (`src/state.rs:34-43`), so a crash mid-save can't leave a broken file.

### Changing settings while koi runs

Once a second the loop checks the modification times of config.toml and the theme files (`src/main.rs:572`). If one changed, it reloads them. Some settings can't change on a running pond:

@excerpt src/config.rs:71-72

::: check You change `audio.volume` in config.toml while koi runs, then restart. Why might the volume still not change?
If you ever used the volume keys, state.toml holds a volume, and that wins over `audio.volume` (`src/main.rs:179`).
:::
:::

::: high
### Themes are configuration as data

A theme is a TOML file with a palette, light and style. It can `extends` another and change only a few values. The built-in themes are compiled into the binary (`crates/koi-theme/src/lib.rs:23`), and a file in your themes folder with the same id replaces one. Debug builds read the built-ins from disk instead (`crates/koi-theme/src/lib.rs:500`), so theme edits show without a rebuild.

### Environment variables

There are no `KOI_*` variables. The environment only locates folders (`XDG_*`, `HOME`, `APPDATA`) and identifies the terminal (`TMUX`, `COLORTERM`, `WT_SESSION`).

### The web page has its own settings

The browser build never reads config.toml. It has URL parameters (`?scene=` and `?ambient=`, `site/main.js:10`), `localStorage` for the last scene, food and volume (`site/main.js:21`), and constants that repeat the terminal's defaults:

@excerpt crates/koi-web/src/lib.rs:19-22

The default volume is repeated the same way: 0.6 in the page (`site/main.js:336`) and 0.6 in the terminal's defaults (`src/config.rs:100`).

::: inferred
These are copies, so they can drift. The page can't read the terminal's defaults, which live in the binary, and no test compares the two.
:::
:::
