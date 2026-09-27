# Boundaries

> What does koi talk to while it runs, and in what formats?

koi talks to four things: the terminal, a handful of files, the audio device, and on Linux, the shared-memory folder `/dev/shm`. It makes no network calls. The only other program it starts is tmux, to ask about one setting.

The terminal is both screen and keyboard. koi writes escape sequences and images to it and reads raw bytes back. Big images skip the terminal stream: koi writes the pixels to a file in `/dev/shm` and sends only its name.

Outside input comes from terminal bytes, your TOML files and music files. A bad value in any of them becomes a warning or is skipped.

In the browser, the page reads two URL parameters, keeps three settings in `localStorage`, and plays sound through Web Audio.

::: medium
### Everything that crosses the edge

| What | Direction | Format | Code |
|---|---|---|---|
| Terminal output | out | escape sequences, Kitty graphics, sixel, half blocks | `crates/koi-term/src/lib.rs:77` |
| Terminal input | in | raw bytes, up to 4 KiB a read | `crates/koi-term/src/sys_unix.rs:75` |
| `/dev/shm/koi-pond-*` | out, read by the terminal | raw RGBA pixels | `crates/koi-term/src/lib.rs:645` |
| config.toml, themes/*.toml | in | TOML | `src/config.rs:306` |
| state.toml | in and out | TOML | `src/state.rs:28-43` |
| `~/.cache/koi-pond/loudness.json` | in and out | JSON: path to file size and gain | `crates/koi-audio/src/lib.rs:294` |
| Music folder | in | mp3 and ogg, plus an optional tracks.json | `crates/koi-audio/src/lib.rs:138` |
| Audio device | out | the default output, through rodio | `crates/koi-audio/src/lib.rs:161` |
| tmux | out and in | `tmux display-message` | `src/main.rs:150` |
| stderr | out | warnings, printed after the pond closes | `src/main.rs:191-193` |

### The terminal

Taking over the terminal is one write: alternate screen, hidden cursor, focus reports, a clear, and mouse reports if they're on:

@excerpt crates/koi-term/src/lib.rs:77-80

Then `probe` asks what the terminal can draw and reads the answers (`crates/koi-term/src/lib.rs:452`). On exit the restore turns each of these off again (`crates/koi-term/src/lib.rs:94`).

A Kitty image through shared memory goes like this. The pixels are copied into a mapped slot file, the slot gets a fresh hard-linked name, and the terminal is sent only that name, in base64:

@excerpt crates/koi-term/src/lib.rs:700-712 mark=706-707,712

The terminal reads the file and unlinks the name. Over SSH the terminal can't see this machine's `/dev/shm`, so koi sends the pixels inline instead, zlib-compressed (`crates/koi-term/src/lib.rs:687-698`).

### Input from the terminal

`parse_input` turns bytes into `Input` events: keys, focus, SGR mouse reports and Kitty answers. Anything it doesn't know becomes `Input::Other`. A read can end in the middle of a sequence, so the loop keeps the unused tail for the next read (`src/main.rs:450-451`).

### Shared-memory files

Each slot is a file that only your user can read or write:

@excerpt crates/koi-term/src/lib.rs:645-646

Dropping the ring deletes its files. A koi process killed with SIGKILL never drops it, so each start first removes files left by koi processes that no longer exist (`crates/koi-term/src/lib.rs:580-592`).

::: check koi was killed with `kill -9` mid-frame. What is left in `/dev/shm`, and when does it go away?
The slot files and any names the terminal hadn't unlinked yet, since `Drop for ShmRing` never ran. The next koi to start removes them: `main` calls `remove_stale_shm` (`src/main.rs:174`), which deletes any `koi-pond-<pid>-…` file whose pid has no `/proc` entry (`crates/koi-term/src/lib.rs:588`). Only Linux does this.
:::

::: check Your config.toml sets `pond.koi = 500`. What happens?
A warning, and the default number of koi. `check` caps koi at 50 because each one costs shared memory, which is RAM (`src/config.rs:361-363`). The value is tried on its own, so the rest of the file still applies.
:::

### In the browser

The page reads `?scene=` and `?ambient=` from the URL (`site/main.js:10`, `site/main.js:64`). It stores `koi.theme`, `koi.food` and `koi.volume` in `localStorage`, and both reading and writing are wrapped in `try`, since private windows may refuse (`site/main.js:19-32`). It fetches `music/tracks.json` from its own site (`site/main.js:326`) and plays through an `AudioContext` (`site/main.js:355`).
:::

::: high
### How each input is kept in bounds

- **Terminal bytes.** A read is at most 4 KiB (`crates/koi-term/src/sys_unix.rs:85`). A Kitty or XTVERSION answer only counts if it is printable text, and koi stops waiting for its end after 512 bytes (`crates/koi-term/src/lib.rs:841`).
- **TOML.** Each key is checked on its own, and `check` also refuses values that would panic later, like a negative number of seconds (`src/config.rs:355-359`).
- **JSON.** A tracks.json or loudness.json that doesn't parse is treated as empty (`crates/koi-audio/src/lib.rs:139`, `crates/koi-audio/src/lib.rs:296`).
- **Music files.** symphonia decodes them on the audio thread. A decode error becomes an error toast (`crates/koi-audio/src/lib.rs:273-276`). A panic there leaves the pond running, because the panic hook only restores the terminal for the main thread (`crates/koi-term/src/lib.rs:66-70`).

### Creating shm files

The slot files are opened with `create` and `truncate`, not exclusively. Only the names are checked: `hard_link` fails if a name already exists (`crates/koi-term/src/lib.rs:707`). The probe's test object, and every image on macOS, is created with `O_CREAT | O_EXCL` and mode `0o600`:

@excerpt crates/koi-term/src/sys_unix.rs:93

On Windows nothing goes through shared memory at all (`crates/koi-term/src/sys_windows.rs:150-154`).

::: inferred
Slot names are predictable, since they hold the pid. A file already sitting at a slot's name would be reused instead of refused. The contents are only pond pictures. The stale-file cleanup checks only that the pid is running, so if the pid was reused by another process, its files stay until that process ends.
:::

### No networking

No crate koi depends on is a network client, and the binary imports no socket functions. This counts the socket calls it links against. Expect 0:

@run nm -D --undefined-only target/release/koi | grep -c -w -E "socket|connect|getaddrinfo" || true

The one download in the repo is `scripts/fetch-music.sh`, which fetches the music pack with curl (`scripts/fetch-music.sh:41`). That's a separate script, run by hand.

### Where stderr goes

While the pond is up, stderr points at `/dev/null`, because ALSA prints to it and it would land on the pond (`crates/koi-term/src/lib.rs:74-75`). koi collects its own warnings and prints them once the terminal is restored. The cost is that a library's messages during the run are lost.
:::
