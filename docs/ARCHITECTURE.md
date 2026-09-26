# Architecture

`koi` is a Rust binary built from a Cargo workspace of six crates. It runs in a terminal with the Kitty graphics protocol, such as Ghostty, directly or inside tmux. Build it with `cargo build --release` and run `target/release/koi`. Plain `cargo build`, `cargo test` and `cargo clippy` at the root cover every crate.

## Crates

| Crate | Path | What it contains | Depends on |
|---|---|---|---|
| `koi-sim` | `crates/koi-sim` | Koi steering, moods, feeding and food, the fixed step `DT`, and the `Splash` and `Shadow` records the water reads. No GPU, terminal or audio code. | nothing |
| `koi-theme` | `crates/koi-theme` | Theme files: the built-in themes (compiled in from `themes/`), user themes, `extends` resolution, derived palette slots, and stepping through families and times. `Palette`, `Light`, `Style`, `Scene`. | serde, toml |
| `koi-render` | `crates/koi-render` | The headless Vulkan device (`Gpu`), the water (`Water`, `water.wgsl`) and the koi sprites (`Poser`, `koi.wgsl`), each with a GPU and a CPU path, painted from a `Theme`. The pond layout comes from `Layout::new(w, h, seed)`, which never sees the theme. Koi outlines, and the fade of a diving koi, are drawn in the pose pass. Pixel themes: hard-edged koi, dither, dash glints and the palette lock table. Weather: rain, mist, fireflies. Food sprites (`food_sprite`) and HUD stones and icons (`hud::Look`). | `koi-sim`, `koi-theme`, wgpu |
| `koi-term` | `crates/koi-term` | Raw mode, alternate screen, focus and mouse reporting (`report_motion` adds pointer motion for the HUD's hover), restore on exit and on panic, signals, `winsize`, `read_input`, `parse_input` (keys, clicks, scroll, motion, a lone Esc), the shm ring, removal of shm left by killed runs, and for tmux the passthrough wrapper and Unicode placeholder cells. | libc, base64 |
| `koi-audio` | `crates/koi-audio` | Music with crossfades and per-track loudness normalization, the generated ambient layer and the food chimes, one per food kind on the yo scale, on their own threads. | `koi-sim`, rodio, symphonia |
| `koi-pond` | root, `src/` | The `koi` binary: `main.rs` (arguments, backend choice, the frame loop, input handling, theme switching and hot reload, the error toast, the adaptive frame rate, the `d` stats line), `hud.rs` (the HUD: its state machine and timers, hit tests, and its images, ids 60 to 67 at z=-2), `config.rs` (the TOML config), `state.rs` (theme, volume and mute, remembered between runs) and `layers.rs` (Kitty images and placements, the pixel themes' scaling up and snapping, and the single frame of tmux mode). | all of the above, serde, toml |

The water height field is in `koi-render`, not `koi-sim`, because on the GPU path it lives in GPU buffers and steps in `water.wgsl`. `koi-sim` only produces the splashes that disturb it.

Shared dependency versions are in `[workspace.dependencies]` in the root `Cargo.toml`.

`cargo doc --workspace --no-deps` builds the API docs. Every library crate has `#![warn(missing_docs)]`.

### Public API

- `koi-sim`: `School::new(w, h, count, seed)`, `step`, `drop_food(kind, x, y)`, `drop_food_random(kind)`, `FoodKind`, `shadows`, `max_speed_ratio`, and the public `fish`, `food`, `splashes`, `speed` and `calmness`. `Koi::pose` returns a `Pose`, and `Pose::lerp` blends two steps. `noise2` is the value noise shared by koi wandering and koi patterns.
- `koi-theme`: `Catalog::load`, `resolve`, `summaries`, `next_scene`, `next_time`; `Theme` with `Summary`, `Palette`, `Light`, `Style`, `Scene`, `warnings` and `files`; `Time`, `ROOT`, `Rgb`.
- `koi-render`: `Gpu::new`, `Water::new`, `set_theme`, `regrid`, `splash`, `step`, `render`, `Poser::new`, `recolor`, `largest`, `bounds`, `pose`, `food_sprite`, and the `hud` painters (`Look`, `Mark`, `Swatch`).
- `koi-term`: `Terminal::enter`, `report_motion`, `tmux_wrap`, `placeholders`, `QUIT`, `winsize`, `read_input`, `parse_input` returning `Input` events (`Focus`, `Click`, `Scroll`, `Move`, `Escape`, `Key`, `Other`), `remove_stale_shm`, and `ShmRing::new` and `transmit`.
- `koi-audio`: `Audio::start(Settings)`, `send(Event)`, the `status` receiver of `Status`, and `shutdown`.

### Data flow

```
stdin ──> koi-term::parse_input ──> main loop (koi-pond) ──> koi-audio Events
                                          │
                        koi-sim School::step: splashes, poses, shadows
                                          v
         koi-render: Water::step, Water::render, Poser::pose (Theme from koi-theme)
                                          │ RGBA images
                                          v
             layers.rs ──> koi-term::ShmRing ──> Kitty commands on stdout ──> Ghostty
```

Input goes to the HUD first (`Hud::input`). What it does not take (a click on the water, `f`, `q`, `d`, `r`) the main loop handles. The `Status` messages from `koi-audio` drive the HUD: a new track peeks the music pill, a volume change peeks it and is saved to the state file, and `NoMusic` hides the pill (as `audio.enabled = false` does) and closes up the row. An audio error shows on the top line; why no music plays shows on the `d` line.

### Themes

At start `main.rs` loads a `Catalog` (the built-in themes, then `~/.config/koi-pond/themes/*.toml` over them) and resolves the starting theme: `--theme`, else the theme remembered in `$XDG_STATE_HOME/koi-pond/state.toml` if config.toml still names the theme it named when that was saved, else `theme.name`. A theme that fails to resolve falls back to the built-in `summer-garden` with a warning. Debug builds read the built-in themes from `themes/` on disk, so edits show without a rebuild.

A switch (`t`, `T`, `l`, `L`, a HUD tray pick, `r`, or a changed file) repaints in place: `Water::set_theme` repaints the statics with the same seed and keeps the waves, `Poser::recolor` repaints the koi bodies, and `Layers::recolor` re-sends the food images. A switch that changes `style.pixel_px` (painted to pixel, or one pixel size to another) needs a new grid, so `build` runs with the current scene: `Water::regrid` moves the water to the new grid and resamples the waves onto it, and the poser and layers are made again, while the school carries over. The simulation is untouched, so every koi keeps its place. A switch to another theme is saved to the state file, and `Hud::set_theme` restyles the HUD and peeks the scene or time stone. Once a second the loop compares the modification times of the current theme's files, the user theme folder and config.toml; a change reloads config.toml and the catalog, then re-resolves. A reload applies `[fps]`, `[input]` (but `input.mouse`), `[theme]`, `[hud]` (but `hud.hover`), `pond.speed` and `pond.calmness`. `[render]`, `[audio]`, `hud.hover`, `input.mouse`, `pond.koi` and `pond.seed` apply on the next start.

The simulation works in water pixels: a grid of `water_px` pixels per cell width, with the height set by the real cell aspect. Koi, food and splashes all use it. `layers.rs` scales it to screen pixels. A painted theme renders the water on the same grid. A pixel theme renders it on its art grid, the window's screen pixels divided by `pixel_px` (rounded up), and `Water` scales the splashes and koi shadows it is given by `per_sim`, its pixels per simulation unit.

## One frame

1. Work out the target rate (see `[fps]` below), then wait for input until the next frame is due. Input wakes the loop early, so a click takes effect at once and raises the rate. The frame time is the scheduled deadline, not the moment the loop woke. A frame more than one interval late starts a new schedule from now.
2. Handle input: the HUD first, then focus in and out (`CSI I`, `CSI O`), clicks on the water, keys.
3. Switch theme if a key asked for it or a watched file changed (see "Themes").
4. Rebuild everything if the window size changed. The seed stays the same, so the pond keeps its layout. The koi start again from their seeded places.
5. Step the simulation in fixed 1/60 s steps up to the frame time: `School::step`, hand its splashes to the water, `Water::step`. The simulation runs at 60 Hz whatever the frame rate. Koi are drawn at a blend of the last two steps, at the frame time's fraction of a step.
6. If a water frame is due (`water_fps`), render the water image.
7. `Layers::encode` appends a fresh image for every koi, plus the water image if it differs from the last one sent, and the food and top line if they changed. `Hud::draw` appends whatever part of the HUD changed: an element's image when its look or fade step changed, a placement to show it again, a delete to hide it, and its text. A koi placement is rounded to a whole screen pixel. The leftover fraction goes into the pose as a fractional origin, so the koi slides inside its image by less than a pixel and moves evenly at any speed. In a pixel theme the placement is rounded to a multiple of `pixel_px` instead, the koi is posed one sprite pixel per art pixel, and the koi and the water are scaled up by repeating pixels to exactly the screen size, since Ghostty would blur any other size. Food sprites are drawn on the art grid and placed on it too.
8. If anything was appended, write it in one piece inside a synchronized update (mode 2026). Otherwise write nothing.

Images go through `ShmRing`: one ring of shm files for the koi, food and HUD, sized for the largest of them (the help card on HiDPI), and 6 for the water (a full-window image in a pixel theme), mapped and pre-faulted once. A theme switch sends all 40 food images in one write and the next frame sends every koi and up to 8 HUD images, so the main ring holds all of those plus 16 spare: 69 files with the default 5 koi. Each image is copied into the next file, hard-linked under a fresh name and sent with `t=s`. Ghostty unlinks that name after reading it. On exit, SIGINT, SIGTERM, SIGHUP, a write error or a panic, the ring removes its files and any names Ghostty has not read. A run killed with SIGKILL leaves its files behind, and the next start removes them.

## tmux

tmux keeps its own copy of each pane's cells and does not pass Kitty graphics commands to the terminal, so the layers above do not work there. With `render.tmux = "auto"` and `$TMUX` set (or `"on"`), koi draws differently:

1. Before entering raw mode, `main.rs` runs `tmux display-message -p -t $TMUX_PANE '#{allow-passthrough}'`. If that prints `off`, koi prints the config line to add and exits with status 2. If tmux cannot be run, koi goes on.
2. `build` makes the grid's cells the frame's cells: `fish_px` pixels wide (as `fish_px = 0` resolves on the backend in use) and as tall as keeps the cell's shape. Everything downstream treats those as screen pixels, so the koi, food, HUD stones and a pixel theme's art grid are sized as usual. The terminal scales the frame to its real cells.
3. `Layers::compose` scales the latest water image to the frame (linear between pixel centres for painted themes, repeated pixels for pixel themes), then draws each koi and food sprite over it with straight alpha. A koi is snapped to a frame pixel (a multiple of `pixel_px` in a pixel theme) and the leftover fraction goes into its pose, as outside tmux.
4. `Hud::compose` draws the visible elements at their fade step into the same frame. Its text is terminal text written over the placeholder cells, with a background that is the average colour of the frame under each span. A span that goes away gets its placeholder cells back.
5. `Layers::send` sends the frame through the main ring with `a=T,U=1,i=<id>,p=1,c=<cols>,r=<rows>`, wrapped in a tmux passthrough (`ESC P tmux; ... ESC \`, every ESC inside doubled). `U=1` makes a virtual placement, and the fixed `p=1` replaces it each frame instead of adding another.
6. The pane holds one placeholder cell per cell: U+10EEEE with a combining mark for its row and one for its column (kitty's rowcolumn-diacritics table, 297 marks), in the image id as a 256-colour foreground. They are written at start, after a resize or a theme switch that rebuilds the layers, on focus in and on Ctrl-L. Other frames send only the image.

The image id is 16 plus the pid modulo 240, so it fits a 256-colour index and two ponds in panes of one window usually differ. The ring has 6 slots the size of the frame. On exit the restore deletes the image (`a=d,d=I`) through the passthrough and sends no bare Kitty command, since tmux would take that for the pane title.

## Config keys

Print the full commented file with `koi --print-default-config`. The default path is `$XDG_CONFIG_HOME/koi-pond/config.toml`, or `~/.config/koi-pond/config.toml`. `--config PATH` picks another file. Missing keys use the defaults. Unknown keys and values the game cannot use (a wrong type, an unknown name, negative seconds, a key that is not ASCII) are warnings that keep the default: they show on the top line for 10 s and print again on exit.

| Key | Default | What it does |
|---|---|---|
| `fps.focused` | 60 | Frame rate while focused or while something is happening. |
| `fps.unfocused_calm` | 8 | Frame rate while unfocused and calm. |
| `fps.input_secs` | 3.0 | A key or click counts as activity for this long. |
| `fps.ripple_secs` | 8.0 | Food landing, being eaten or sinking counts as activity for this long. |
| `fps.dart_speed` | 1.6 | A koi faster than this multiple of its cruise speed counts as activity. |
| `fps.send_when_unchanged` | false | Re-send everything every frame, even when nothing changed. |
| `render.backend` | "gpu" | "gpu" (wgpu on Vulkan) or "cpu". "gpu" falls back to "cpu" when no adapter is found. |
| `render.water_px` | 2 | Water image pixels per cell width. |
| `render.fish_px` | 0 | Koi image pixels per cell width. 0 means native screen pixels, the sharpest koi, capped at 10 on the CPU backend, where posing costs the square of this. 8 sends about two thirds of the bytes with a softer koi. |
| `render.water_fps` | 30 | Most water images sent per second. |
| `render.tmux` | "auto" | "auto" draws one frame shown through placeholder cells (see "tmux") when `$TMUX` is set, "on" always, "off" never. |
| `pond.koi` | 5 | Number of koi. |
| `pond.speed` | 1.0 | Swimming speed as a multiple of the calm default. |
| `pond.calmness` | 1.0 | Higher is lazier: slower turns, softer steering, longer glides. |
| `pond.seed` | 0 | 0 gives a new pond each start. Anything else repeats the same pond. |
| `hud.show` | "always" | "always" keeps the row on screen, "auto" hides it until a HUD key, hover or `Tab`, "hidden" draws nothing but keeps the keys. `Tab` switches between "always" and "auto" until the pond closes, and a reload keeps that choice unless `hud.show` changed. |
| `hud.hover` | false | Resting the pointer in the bottom 4 rows for 0.5 s shows the row (mouse motion reports, mode 1003). |
| `hud.align` | "center" | "center" or "left". |
| `hud.fade` | true | Fade in and out; false shows and hides at once. |
| `hud.peek_secs`, `hud.hold_secs` | 3.0, 4.0 | How long a peek shows, and how long the row stays after the last HUD input. |
| `theme.name` | "summer-garden" | The starting theme. `koi --list-themes` lists them; docs/STYLE.md describes them. |
| `audio.enabled` | true | Start the audio thread. |
| `audio.music_dir` | empty | Folder of mp3 and ogg files, with an optional tracks.json. Empty means `<repo>/assets/music`, the music that comes with the game. |
| `audio.volume`, `audio.ambient_volume` | 0.7, 0.6 | Music volume, and the ambient layer as a fraction of it. |
| `audio.chime` | true | Chime when food lands, one voice per food kind. |
| `audio.normalize` | true | Play every track at about the same loudness. Measured gains are cached in `$XDG_CACHE_HOME/koi-pond/loudness.json` (or `~/.cache`). |
| `input.mouse` | true | Click the pond to drop food. |
| `input.feed`, `quit`, `stats` | f, q, d | Keys, each one ASCII character. Ctrl-C also quits. |
| `input.hud`, `help`, `food` | Tab, ?, 1 to 5 | Hide or show the HUD, the help card, and the food kinds (pellets, flakes, petals, seeds, treat). Esc closes a tray, the card, or in "auto" the HUD. |
| `input.next_track`, `mute`, `volume_up`, `volume_down` | n, m, +, - | Audio keys. |
| `input.next_theme`, `prev_theme`, `later`, `earlier`, `reload` | t, T, l, L, r | Theme keys. |

"Something is happening" means any of: focused, pellets in the water, recent input, recent food ripples, a koi darting, a HUD element fading. The `d` line shows which one sets the current target.

## Tests

`cargo test` runs every crate's tests. `cargo test -p koi-sim` runs the simulation tests alone and builds without wgpu or rodio. The tests that compare the GPU path touch a GPU only when `VK_ICD_FILENAMES` names a Vulkan driver, and then fail if it finds no adapter. Use lavapipe: `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test`. Without it they check the CPU path only.

| Test | Crate | What it checks |
|---|---|---|
| `food_lands_in_open_water` | `koi-sim` | Random drops, and a click on the rim stones, land every piece in open water, clear of the stones. |
| `calm_limits_hold` | `koi-sim` | Ten minutes of pond time stay inside the limits for turn rate, speed, spacing and feeding. |
| `pellets_are_gulped_by_a_rushing_koi`, `flakes_bring_several_koi_calmly`, `petals_are_mouthed_once_and_fade`, `seeds_spiral_down_and_koi_follow`, `the_treat_is_circled_and_shared` | `koi-sim` | Each food kind's handful, cap, float and sink, and the koi's reaction to it. |
| `chimes_stay_in_key_and_in_character` | `koi-audio` | Every chime is on the yo scale, and each kind has its own register and loudness. |
| `sprites_fade_or_shrink` | `koi-render` | Food sprites fade (or, for the treat, shrink bite by bite) and keep their size. |
| `selout_light_edge_stays_toward_the_sun` | `koi-render` | A selout koi keeps its light edge toward the sun heading east and west, on both backends. |
| `petals_keep_their_places_across_themes` | `koi-render` | Petal k is in the same place in every theme. |
| `pixel_stones_are_hard_edged`, `ink_contrasts_with_every_theme` | `koi-render` | HUD stones in pixel themes have no soft alpha; HUD text reads on every theme's stones. |
| `layout`, `peek_times_out`, `expanded_holds_while_pointed_at`, `clicks_and_trays`, `scene_tray_switches_family`, `one_time_family`, `help_card`, `short_window_only_peeks`, `volume_peeks_on_change`, `draw_sends_only_changes`, `dwell_escape_and_shrinking` | `koi-pond` (hud.rs) | The HUD's layout, timers, hover reveal, clicks, trays, keys, Esc order, collapse in a short window, and that it sends only what changed. They run with `hud.show = "auto"`. |
| `always_keeps_the_row` | `koi-pond` (hud.rs) | With `hud.show = "always"` the row fades in at start and stays past Esc and the hold time, `Tab` switches to "auto" and back, and a short window only peeks. |
| `bad_values_warn_and_keep_the_default` | `koi-pond` (config.rs) | Unknown keys, wrong types, unknown names, negative seconds, non-ASCII keys and short arrays each warn with the file and key and keep the default, while good values beside them apply. |
| `round_trip_and_the_remembered_theme` | `koi-pond` (state.rs) | The state file reads back what was saved, and the remembered theme gives way once config.toml names another. |
| `subpixel_motion_is_even` | `koi-render` | A koi moved 0.1 sprite pixels per frame has its rendered centroid advance by about 0.1 every frame. It runs on the CPU path, and on the GPU path too when `VK_ICD_FILENAMES` is set. |
| `parses_mixed_input` | `koi-term` | Focus reports, clicks, releases, scroll, motion, a lone Esc, Alt chords, SS3 and other CSI sequences, and keys in one read come out as the right events. |
| `tmux_wrap_doubles_escapes` | `koi-term` | The passthrough doubles every ESC inside it and ends with a single `ESC \`. |
| `placeholder_cells_carry_row_column_and_id` | `koi-term` | Placeholder cells have the cursor move, the id as a 256-colour foreground, and the right row and column marks, including past the end of the mark table. |
| `split_reads_carry_over` | `koi-term` | Input split across two reads at any byte gives the same events as one read, once the cut-off sequence is carried into the next read. |
| `backends_draw_the_same_frame` | `koi-render` (tests/parity.rs) | One fixed frame (seed, splashes, 90 steps, koi shadows, a theme switch halfway) of eight themes, water and a posed koi half dived, matches between the CPU and GPU paths within 2/255 per channel. The themes cover every outline mode, mist, rain and fireflies, and the three pixel themes (palette lock, dither, dash glints, crisp koi), whose switch moves the water to a finer art grid with `regrid`. Without `VK_ICD_FILENAMES` only the CPU half runs. |
| `lock_table_snaps_to_swatches` | `koi-render` | Every palette lock table entry is a swatch, and each swatch snaps to itself. |
| `regrid_keeps_the_waves` | `koi-render` | Moving the water to a finer grid keeps a ripple's height and place. |
| `pixel_sprites_are_screen_sized_and_on_the_art_grid` | `koi-pond` (layers.rs) | In a pixel theme the water and every koi image are sent at exactly screen size, and each koi sits on a multiple of `pixel_px`. |
| `every_built_in_resolves_cleanly` | `koi-theme` | Every built-in theme resolves with no warnings, through `pixel.toml` where it applies. |
| `inheritance_rules` | `koi-theme` | Tables merge, file keys and derived slots are not inherited, derived slots follow the child's colors. |
| `broken_themes_are_errors_and_unknown_keys_warnings` | `koi-theme` | Bad colors, wrong types, bad enum values, cycles and missing parents are errors naming the file; unknown keys are warnings. |
| `names_and_user_overrides` | `koi-theme` | Display names resolve; a user file replaces a built-in, and a user root is merged over the built-in root and inherited by every theme. |
| `stepping_scenes_and_times` | `koi-theme` | `t` and `l` pick the right theme, wrap round, skip hidden themes and keep the time when they can. |

## Music files

The mp3 files are not in git. `scripts/fetch-music.sh` downloads every track in `assets/music/tracks.json` from its `download` URL (`--force` downloads them again, `--dry-run` only lists what it would fetch). Credits are in `assets/music/CREDITS.md`.

## Building without libasound2-dev

rodio links against ALSA, and its build finds it through pkg-config, which `libasound2-dev` provides. Without the package, a two-file shim over the runtime library works:

```sh
mkdir -p .alsa-shim
ln -s /usr/lib/x86_64-linux-gnu/libasound.so.2 .alsa-shim/libasound.so
printf 'libdir=${pcfiledir}\nName: alsa\nDescription: shim\nVersion: 1.2.0\nLibs: -L${libdir} -lasound\nCflags:\n' > .alsa-shim/alsa.pc
PKG_CONFIG_PATH=$PWD/.alsa-shim cargo build --release
```

Both `.alsa-shim` and a `.cargo/config.toml` that sets `PKG_CONFIG_PATH` are git-ignored.
