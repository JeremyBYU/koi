# Architecture

`koi` is a Rust binary built from a Cargo workspace of six crates. It runs in any terminal, directly or inside tmux, and draws best with the Kitty graphics protocol, as Ghostty has (see "Terminals"). Build it with `cargo build --release` and run `target/release/koi`. Plain `cargo build`, `cargo test` and `cargo clippy` at the root cover every crate.

## Crates

| Crate | Path | What it contains | Depends on |
|---|---|---|---|
| `koi-sim` | `crates/koi-sim` | Koi steering, moods, feeding and food, petting and its bubbles, the fixed step `DT`, and the `Splash` and `Shadow` records the water reads. No GPU, terminal or audio code. | nothing |
| `koi-theme` | `crates/koi-theme` | Theme files: the built-in themes (compiled in from `themes/`), user themes, `extends` resolution, derived palette slots, and stepping through families and times. `Palette`, `Light`, `Style`, `Scene`. | serde, toml |
| `koi-render` | `crates/koi-render` | The headless Vulkan device (`Gpu`), the water (`Water`, `water.wgsl`) and the koi sprites (`Poser`, `koi.wgsl`), each with a GPU and a CPU path, painted from a `Theme`. The pond layout comes from `Layout::new(w, h, seed)`, which never sees the theme. Koi outlines, and the fade of a diving koi, are drawn in the pose pass. Pixel themes: hard-edged koi, dither, dash glints and the palette lock table. Weather: rain, mist, fireflies. A petted koi's tail flutter and head shimmer. Food and bubble sprites (`food_sprite`, `bubble_sprite`) and HUD stones and icons (`hud::Look`). | `koi-sim`, `koi-theme`, wgpu |
| `koi-term` | `crates/koi-term` | Raw mode, alternate screen, focus and mouse reporting of presses, releases and drags (`report_motion` adds pointer motion for the HUD's hover), restore on exit and on panic, signals, `winsize`, the terminal probe and `Caps`, `read_input`, `parse_input` (keys, clicks, releases, drags, scroll, motion, a lone Esc, Kitty graphics answers), the shm ring and its direct (inline, zlib) mode, removal of shm left by killed runs, and for tmux the passthrough wrapper and Unicode placeholder cells. | libc, base64, miniz_oxide |
| `koi-audio` | `crates/koi-audio` | Music with crossfades and per-track loudness normalization, the generated ambient layer, the food chimes, one per food kind on the yo scale, and the petting bloop, on their own threads. | `koi-sim`, rodio, symphonia |
| `koi-pond` | root, `src/` | The `koi` binary: `main.rs` (arguments, backend choice, the frame loop, input handling, theme switching and hot reload, the error toast, the adaptive frame rate, the `d` stats line), `hud.rs` (the HUD: its state machine and timers, hit tests, and its images, ids 60 to 67 at z=-2), `config.rs` (the TOML config), `state.rs` (theme, volume and mute, remembered between runs) and `layers.rs` (the tiers and `choose`, Kitty images and placements, the pixel themes' scaling up and snapping, and the single frame of tmux, sixel and blocks with its sinks: placeholder cells, the sixel encoder and the half-block writer). | all of the above, serde, toml |

The water height field is in `koi-render`, not `koi-sim`, because on the GPU path it lives in GPU buffers and steps in `water.wgsl`. `koi-sim` only produces the splashes that disturb it.

Shared dependency versions are in `[workspace.dependencies]` in the root `Cargo.toml`.

`cargo doc --workspace --no-deps` builds the API docs. Every library crate has `#![warn(missing_docs)]`.

### Public API

- `koi-sim`: `School::new(w, h, count, seed)`, `step`, `drop_food(kind, x, y)`, `drop_food_random(kind)`, `pet(x, y, reach, secs)`, `move_hand`, `let_go`, `FoodKind`, `shadows`, `max_speed_ratio`, and the public `fish`, `food`, `bubbles`, `splashes`, `speed` and `calmness`. `Koi::pose` returns a `Pose`, and `Pose::lerp` blends two steps. `noise2` is the value noise shared by koi wandering and koi patterns.
- `koi-theme`: `Catalog::load`, `resolve`, `summaries`, `next_scene`, `next_time`; `Theme` with `Summary`, `Palette`, `Light`, `Style`, `Scene`, `warnings` and `files`; `Time`, `ROOT`, `Rgb`.
- `koi-render`: `Gpu::new`, `Water::new`, `set_theme`, `regrid`, `splash`, `step`, `render`, `Poser::new`, `recolor`, `largest`, `bounds`, `pose`, `food_sprite`, and the `hud` painters (`Look`, `Mark`, `Swatch`).
- `koi-term`: `Terminal::enter`, `report_motion`, `tmux_wrap`, `placeholders`, `QUIT`, `winsize`, `probe`, `Caps::parse` and `Sixel`, `read_input`, `parse_input` returning `Input` events (`Focus`, `Click`, `Release`, `Drag`, `Scroll`, `Move`, `Escape`, `Key`, `Graphics`, `Other`), `remove_stale_shm`, and `ShmRing::new`, `direct` and `transmit`.
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
                 │
                 └──> one frame ──> placeholder cells (tmux), sixel images or half blocks
```

Input goes to the HUD first (`Hud::input`). What it does not take (a press on the water or on a koi, a drag and release, `f`, `p`, `q`, `d`, `r`) the main loop handles. The `Status` messages from `koi-audio` drive the HUD: a new track peeks the music pill, a volume change peeks it and is saved to the state file, and `NoMusic` hides the pill (as `audio.enabled = false` does) and closes up the row. An audio error shows on the top line; why no music plays shows on the `d` line.

### Themes

At start `main.rs` loads a `Catalog` (the built-in themes, then `~/.config/koi-pond/themes/*.toml` over them) and resolves the starting theme: `--theme`, else the theme remembered in `$XDG_STATE_HOME/koi-pond/state.toml` if config.toml still names the theme it named when that was saved, else `theme.name`. A theme that fails to resolve falls back to the built-in `summer-garden` with a warning. Debug builds read the built-in themes from `themes/` on disk, so edits show without a rebuild.

A switch (`t`, `T`, `l`, `L`, a HUD tray pick, `r`, or a changed file) repaints in place: `Water::set_theme` repaints the statics with the same seed and keeps the waves, `Poser::recolor` repaints the koi bodies, and `Layers::recolor` re-sends the food images. A switch that changes `style.pixel_px` (painted to pixel, or one pixel size to another) needs a new grid, so `build` runs with the current scene: `Water::regrid` moves the water to the new grid and resamples the waves onto it, and the poser and layers are made again, while the school carries over. The simulation is untouched, so every koi keeps its place. A switch to another theme is saved to the state file, and `Hud::set_theme` restyles the HUD and peeks the scene or time stone. Once a second the loop compares the modification times of the current theme's files, the user theme folder and config.toml; a change reloads config.toml and the catalog, then re-resolves. A reload applies `[fps]`, `[input]` (but `input.mouse`), `[theme]`, `[hud]` (but `hud.hover`), `pond.speed` and `pond.calmness`. `[render]`, `[audio]`, `hud.hover`, `input.mouse`, `pond.koi` and `pond.seed` apply on the next start.

The simulation works in water pixels: a grid of `water_px` pixels per cell width, with the height set by the real cell aspect. Koi, food and splashes all use it. `layers.rs` scales it to screen pixels. A painted theme renders the water on the same grid. A pixel theme renders it on its art grid, the window's screen pixels divided by `pixel_px` (rounded up), and `Water` scales the splashes and koi shadows it is given by `per_sim`, its pixels per simulation unit.

## One frame

1. Work out the target rate (see `[fps]` below), then wait for input until the next frame is due. Input wakes the loop early, so a click takes effect at once and raises the rate. The frame time is the scheduled deadline, not the moment the loop woke. A frame more than one interval late starts a new schedule from now.
2. Handle input: the HUD first, then focus in and out (`CSI I`, `CSI O`), presses, drags and releases on the water, keys.
3. Switch theme if a key asked for it or a watched file changed (see "Themes").
4. Rebuild everything if the window size changed. The seed stays the same, so the pond keeps its layout. The koi start again from their seeded places.
5. Step the simulation in fixed 1/60 s steps up to the frame time: `School::step`, hand its splashes to the water, `Water::step`. The simulation runs at 60 Hz whatever the frame rate. Koi are drawn at a blend of the last two steps, at the frame time's fraction of a step.
6. If a water frame is due (`water_fps`), render the water image.
7. `Layers::encode` appends a fresh image for every koi, plus the water image if it differs from the last one sent, and the food and top line if they changed. `Hud::draw` appends whatever part of the HUD changed: an element's image when its look or fade step changed, a placement to show it again, a delete to hide it, and its text. A koi placement is rounded to a whole screen pixel. The leftover fraction goes into the pose as a fractional origin, so the koi slides inside its image by less than a pixel and moves evenly at any speed. In a pixel theme the placement is rounded to a multiple of `pixel_px` instead, the koi is posed one sprite pixel per art pixel, and the koi and the water are scaled up by repeating pixels to exactly the screen size, since Ghostty would blur any other size. Food sprites are drawn on the art grid and placed on it too.
8. If anything was appended, write it in one piece inside a synchronized update (mode 2026). Otherwise write nothing. A write that takes longer than the frame interval halves the most frames a second the loop aims for, down to 8. It climbs back by a quarter every 2 s.

Steps 7 and 8 are for the `kitty` and `kitty-direct` tiers outside tmux. The other tiers draw one frame instead: see "tmux" and "Terminals".

Images go through `ShmRing`: one ring of shm files for the koi, food and HUD, sized for the largest of them (the help card on HiDPI), and 6 for the water (a full-window image in a pixel theme), mapped and pre-faulted once. A theme switch sends all 48 food and bubble images in one write and the next frame sends every koi and up to 8 HUD images, so the main ring holds all of those plus 16 spare: 77 files with the default 5 koi. Each image is copied into the next file, hard-linked under a fresh name and sent with `t=s`. Ghostty unlinks that name after reading it. On exit, SIGINT, SIGTERM, SIGHUP, a write error or a panic, the ring removes its files and any names Ghostty has not read. A run killed with SIGKILL leaves its files behind, and the next start removes them.

## tmux

tmux keeps its own copy of each pane's cells and does not pass Kitty graphics commands to the terminal, so the layers above do not work there. With `render.tmux = "auto"` and `$TMUX` set (or `"on"`), koi draws differently:

1. Before entering raw mode, `main.rs` runs `tmux display-message -p -t $TMUX_PANE '#{allow-passthrough}'`. If that prints `off`, the warning on the top line gives the config line to add. The probe's Kitty queries then get no answer, so auto picks blocks. If tmux cannot be run, koi goes on.
2. `build` makes the grid's cells the frame's cells: `fish_px` pixels wide (as `fish_px = 0` resolves on the backend in use) and as tall as keeps the cell's shape. Everything downstream treats those as screen pixels, so the koi, food, HUD stones and a pixel theme's art grid are sized as usual. The terminal scales the frame to its real cells.
3. `Layers::compose` scales the latest water image to the frame (linear between pixel centres for painted themes, repeated pixels for pixel themes), then draws each koi and food sprite over it with straight alpha. A koi is snapped to a frame pixel (a multiple of `pixel_px` in a pixel theme) and the leftover fraction goes into its pose, as outside tmux.
4. `Hud::compose` draws the visible elements at their fade step into the same frame. Its text is terminal text written over the placeholder cells, with a background that is the average colour of the frame under each span. `Layers::send` gets the cells of every span (`Hud::cells`) and the top line's, and gives a cell that text left its placeholder back.
5. `Layers::send` sends the frame through the main ring with `a=T,U=1,i=<id>,p=1,c=<cols>,r=<rows>`, wrapped in a tmux passthrough (`ESC P tmux; ... ESC \`, every ESC inside doubled). `U=1` makes a virtual placement, and the fixed `p=1` replaces it each frame instead of adding another.
6. The pane holds one placeholder cell per cell: U+10EEEE with a combining mark for its row and one for its column (kitty's rowcolumn-diacritics table, 297 marks), in the image id as a 256-colour foreground. They are written at start, after a resize or a theme switch that rebuilds the layers, on focus in and on Ctrl-L. Other frames send only the image.

This path is used in tmux only for the Kitty tiers, with shared memory or inline (`kitty-direct`). The image id is 16 plus the pid modulo 240, so it fits a 256-colour index and two ponds in panes of one window usually differ. The ring has 6 slots the size of the frame. On exit the restore deletes the image (`a=d,d=I`) through the passthrough and sends no bare Kitty command, since tmux would take that for the pane title.

## Terminals

Before the GPU and the audio start, `koi_term::probe` writes one query and reads the answers for up to 2 s: a Kitty image read from shared memory (`a=q,t=s`, id 31, sent only if a shm object can be made), a Kitty image sent inline (`a=q,t=d`, id 32), XTSMGRAPHICS for the sixel colour registers (`CSI ? 1;1;0 S`) and the largest image (`CSI ? 2;4;0 S`), the cell size (`CSI 16 t`), XTVERSION (`CSI > 0 q`), and last the device attributes (`CSI c`). Every terminal answers the device attributes, and after the other queries, so their answer ends the wait. In tmux the Kitty queries go through the passthrough. tmux answers the device attributes itself, before the terminal outside answers the Kitty queries, so the wait goes on until both Kitty answers are in or 300 ms have passed.

`Caps::parse` reads the answers into `Caps`: Kitty through shared memory, Kitty inline, sixel (attribute 4 in the device attributes) with its colour registers (256 when the terminal does not say) and largest image, the cell size, 24-bit colour and the terminal's name. 24-bit colour is `COLORTERM=truecolor` or `24bit`, `WT_SESSION` (Windows Terminal), an xterm, or any Kitty answer. `layers::choose` then picks the tier:

| Tier | Chosen by auto when | Frames per second | How it draws |
|---|---|---|---|
| `kitty` | the shared-memory query answers OK, and the terminal is not WezTerm | the config's | Layered Kitty images through the shm ring, as in "One frame". In tmux, the one frame through placeholder cells. |
| `kitty-direct` | the inline query answers OK but the shared-memory one does not (over SSH), and the terminal is not WezTerm | 30 at most | The same commands, with each image zlib-compressed (level 1), in base64 chunks of at most 4096 bytes, `m=1` on all but the last. |
| `sixel` | the device attributes have 4, the terminal has at least 64 colour registers, its largest image is at least as wide as the window at start, and koi is not in tmux | 20 at most, 30 in a pixel theme | One frame at the terminal's own pixels. Each row of cells sends one image from its first changed cell to its last, split where text is. The bottom row is coloured spaces, since a sixel image on the last line scrolls the screen in xterm. |
| `blocks` | none of the above, including a terminal that answers nothing | the config's | One frame of two pixels per cell, water drawn at `water_px = 1`, and each cell whose colours moved more than 2 per channel is written as U+2580 with its top pixel in the foreground and its bottom pixel in the background, or a space when both are the same. |

WezTerm answers the Kitty queries, but it falls behind reading the shm files, garbles koi placed inside a cell and runs out of memory within a minute, so auto gives it sixel. `render.protocol` or `--protocol` forces a tier. One the terminal did not confirm is tried anyway, with a warning on the top line.

Sixel details: the palette is fixed per theme. A pixel theme uses its swatches and slot colours. A painted theme uses a median cut of the first frame after a switch, up to the terminal's registers or 256. A 32x32x32 table maps each colour to its nearest entry, with no dither. Each image defines only the registers it uses, keeps pixels it does not set (P2=1), so its height need not be a multiple of 6, and run-length encodes runs of 4 or more. Text is never drawn over: a cell that text leaves is sent again.

Blocks without 24-bit colour use the 256-colour palette's cube and grey ramp.

The Kitty tiers ask for the terminal's answer on 1 of every 60 images (no `q`, so an OK comes back too); the rest are sent with `q=2`. Three errors in a row (`Input::Graphics`) drop that capability from `Caps`, and the loop chooses again and rebuilds the scene with a toast. So does a sixel or `kitty-direct` terminal that stays at the frame rate floor of 8, with every write slower than the frame interval, for 5 s. A resize chooses again too. The restore on exit deletes the Kitty images only when the tier at start was a Kitty one, so no other terminal sees a Kitty command.

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
| `render.protocol` | "auto" | "auto" asks the terminal (see "Terminals"), or "kitty", "kitty-direct", "sixel" or "blocks" to force one. `--protocol NAME` overrides it for one run. |
| `render.backend` | "gpu" | "gpu" (wgpu on Vulkan, or Metal on macOS) or "cpu". "gpu" falls back to "cpu" when no adapter is found. `--backend gpu|cpu` overrides it for one run. |
| `render.water_px` | 2 | Water image pixels per cell width. |
| `render.fish_px` | 0 | Koi image pixels per cell width. 0 means native screen pixels, the sharpest koi, capped at 10 on the CPU backend, where posing costs the square of this. 8 sends about two thirds of the bytes with a softer koi. |
| `render.water_fps` | 30 | Most water images sent per second. |
| `render.tmux` | "auto" | With a Kitty tier, "auto" draws one frame shown through placeholder cells (see "tmux") when `$TMUX` is set, "on" always, "off" never. |
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
| `audio.music_dir` | empty | Folder of mp3 and ogg files, with an optional tracks.json. Empty looks in `$XDG_DATA_HOME/koi-pond/music` and beside the binary (see Music files), and plays the three built-in tracks if none has music. |
| `audio.volume`, `audio.ambient_volume` | 0.7, 0.6 | Music volume, and the ambient layer as a fraction of it. |
| `audio.chime` | true | Chime when food lands, one voice per food kind. |
| `audio.normalize` | true | Play every track at about the same loudness. Measured gains are cached in `$XDG_CACHE_HOME/koi-pond/loudness.json` (or `~/.cache`). |
| `input.mouse` | true | Click the pond to drop food. |
| `input.pet_click` | true | A press on a koi pets it instead of feeding: holding keeps the hand in the water up to 6 s of nuzzling, a drag leads the koi, and a release or losing focus lets go. false makes every click feed. |
| `input.pet` | p | Put a hand in the middle of the pond. The nearest koi not feeding, favouring one petted lately, comes and nuzzles it for 3 s. |
| `input.feed`, `quit`, `stats` | f, q, d | Keys, each one ASCII character. Ctrl-C also quits. |
| `input.hud`, `help`, `food` | Tab, ?, 1 to 5 | Hide or show the HUD, the help card, and the food kinds (pellets, flakes, petals, seeds, treat). Esc closes a tray, the card, or in "auto" the HUD. |
| `input.next_track`, `mute`, `volume_up`, `volume_down` | n, m, +, - | Audio keys. |
| `input.next_theme`, `prev_theme`, `later`, `earlier`, `reload` | t, T, l, L, r | Theme keys. |

"Something is happening" means any of: focused, pellets in the water, recent input, recent food ripples, a koi darting, a koi pleased at being petted, a HUD element fading. The `d` line shows which one sets the current target.

## Tests

`cargo test` runs every crate's tests. `cargo test -p koi-sim` runs the simulation tests alone and builds without wgpu or rodio. The tests that compare the GPU path touch a GPU only when `VK_ICD_FILENAMES` names a Vulkan driver, and then fail if it finds no adapter. Use lavapipe: `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test`. Without it they check the CPU path only.

| Test | Crate | What it checks |
|---|---|---|
| `food_lands_in_open_water` | `koi-sim` | Random drops, and a click on the rim stones, land every piece in open water, clear of the stones. |
| `calm_limits_hold` | `koi-sim` | Ten minutes of pond time stay inside the limits for turn rate, speed, spacing and feeding. |
| `pellets_are_gulped_by_a_rushing_koi`, `flakes_bring_several_koi_calmly`, `petals_are_mouthed_once_and_fade`, `seeds_spiral_down_and_koi_follow`, `the_treat_is_circled_and_shared` | `koi-sim` | Each food kind's handful, cap, float and sink, and the koi's reaction to it. |
| `a_pressed_koi_nuzzles_and_settles` | `koi-sim` | A press on a koi's spine pets it and a press a BL away does not. Pressed mid-body, it comes round to nuzzle the hand, grows pleased and blows bubbles; let go, it lingers, calms within 4 s and cruises again, never faster than `CRUISE_CAP`. |
| `p_brings_the_nearest_koi` | `koi-sim` | `p` picks the nearest koi that is not eating, which reaches the hand within 10 s, moves on after its nuzzle, and no koi passes `CRUISE_CAP`. |
| `chimes_stay_in_key_and_in_character` | `koi-audio` | Every chime is on the yo scale, and each kind has its own register and loudness. The petting bell is in key and quiet, with a low bloop, and a second pet soon after is silent. |
| `sprites_fade_or_shrink` | `koi-render` | Food sprites fade (or, for the treat, shrink bite by bite) and keep their size. |
| `selout_light_edge_stays_toward_the_sun` | `koi-render` | A selout koi keeps its light edge toward the sun heading east and west, on both backends. |
| `petals_keep_their_places_across_themes` | `koi-render` | Petal k is in the same place in every theme. |
| `pixel_stones_are_hard_edged`, `ink_contrasts_with_every_theme` | `koi-render` | HUD stones in pixel themes have no soft alpha; HUD text reads on every theme's stones. |
| `layout`, `peek_times_out`, `expanded_holds_while_pointed_at`, `clicks_and_trays`, `scene_tray_switches_family`, `one_time_family`, `help_card`, `short_window_only_peeks`, `volume_peeks_on_change`, `draw_sends_only_changes`, `dwell_escape_and_shrinking` | `koi-pond` (hud.rs) | The HUD's layout, timers, hover reveal, clicks, trays, keys, Esc order, collapse in a short window, and that it sends only what changed. They run with `hud.show = "auto"`. |
| `always_keeps_the_row` | `koi-pond` (hud.rs) | With `hud.show = "always"` the row fades in at start and stays past Esc and the hold time, `Tab` switches to "auto" and back, and a short window only peeks. |
| `bad_values_warn_and_keep_the_default` | `koi-pond` (config.rs) | Unknown keys, wrong types, unknown names, negative seconds, non-ASCII keys and short arrays each warn with the file and key and keep the default, while good values beside them apply. |
| `round_trip_and_the_remembered_theme` | `koi-pond` (state.rs) | The state file reads back what was saved, and the remembered theme gives way once config.toml names another. |
| `subpixel_motion_is_even` | `koi-render` | A koi moved 0.1 sprite pixels per frame has its rendered centroid advance by about 0.1 every frame. It runs on the CPU path, and on the GPU path too when `VK_ICD_FILENAMES` is set. |
| `parses_mixed_input` | `koi-term` | Focus reports, clicks, releases, drags, scroll, motion, a lone Esc, Alt chords (Alt-`_` too), SS3 and other CSI sequences, Kitty graphics answers, a late XTVERSION answer, and keys in one read come out as the right events. |
| `caps_from_recorded_answers` | `koi-term` | Answers recorded from Ghostty, WezTerm, xterm, foot, mlterm, tmux in Ghostty, GNOME Terminal (VTE) and a Kitty terminal over SSH, and silence, read into the right `Caps`. |
| `direct_ring_chunks_and_inflates` | `koi-term` | A direct ring's chunks are at most 4096 base64 bytes with the keys on the first and `m=1` on all but the last, and they inflate back to the image. |
| `tmux_wrap_doubles_escapes` | `koi-term` | The passthrough doubles every ESC inside it and ends with a single `ESC \`. |
| `placeholder_cells_carry_row_column_and_id` | `koi-term` | Placeholder cells have the cursor move, the id as a 256-colour foreground, and the right row and column marks, including past the end of the mark table. |
| `split_reads_carry_over` | `koi-term` | Input split across two reads at any byte gives the same events as one read, once the cut-off sequence is carried into the next read. |
| `backends_draw_the_same_frame` | `koi-render` (tests/parity.rs) | One fixed frame (seed, splashes, 90 steps, koi shadows, a theme switch halfway) of eight themes, water and a posed koi half dived and pleased (joy's head shimmer), matches between the CPU and GPU paths within 2/255 per channel. The themes cover every outline mode, mist, rain and fireflies, and the three pixel themes (palette lock, dither, dash glints, crisp koi), whose switch moves the water to a finer art grid with `regrid`. Without `VK_ICD_FILENAMES` only the CPU half runs. |
| `lock_table_snaps_to_swatches` | `koi-render` | Every palette lock table entry is a swatch, and each swatch snaps to itself. |
| `regrid_keeps_the_waves` | `koi-render` | Moving the water to a finer grid keeps a ripple's height and place. |
| `pixel_sprites_are_screen_sized_and_on_the_art_grid` | `koi-pond` (layers.rs) | In a pixel theme the water and every koi image are sent at exactly screen size, and each koi sits on a multiple of `pixel_px`. |
| `choose_the_best_tier` | `koi-pond` (layers.rs) | Auto picks kitty, kitty-direct, sixel or blocks from `Caps`, WezTerm gets sixel, tmux never gets sixel, too few colours or too small a largest image give blocks, and a forced tier is kept with a warning. |
| `sixel_round_trips` | `koi-pond` (layers.rs) | Sixel images of heights that are and are not multiples of 6 decode (with icy_sixel) to the palette colours within 3, rows below the image stay unset, and runs of 3 are written out while runs of 4 are run-length encoded. |
| `sixel_spans_update_the_last_frame` | `koi-pond` (layers.rs) | The images `send` writes, pasted where they go over the previous frame, give the new frame. A row splits at text, text is never drawn over, the cells text leaves are drawn again, the bottom row gets no image, and an unchanged frame sends nothing. |
| `blocks_send_only_changed_cells` | `koi-pond` (layers.rs) | Half-block bytes for a first frame, nothing for an unchanged one or a change within the tolerance, one cell for a change, none under text, and the 256-colour cube and grey ramp. |
| `every_built_in_resolves_cleanly` | `koi-theme` | Every built-in theme resolves with no warnings, through `pixel.toml` where it applies. |
| `inheritance_rules` | `koi-theme` | Tables merge, file keys and derived slots are not inherited, derived slots follow the child's colors. |
| `broken_themes_are_errors_and_unknown_keys_warnings` | `koi-theme` | Bad colors, wrong types, bad enum values, cycles and missing parents are errors naming the file; unknown keys are warnings. |
| `names_and_user_overrides` | `koi-theme` | Display names resolve; a user file replaces a built-in, and a user root is merged over the built-in root and inherited by every theme. |
| `stepping_scenes_and_times` | `koi-theme` | `t` and `l` pick the right theme, wrap round, skip hidden themes and keep the time when they can. |

## Music files

Three tracks are built into the binary as Ogg Vorbis (`assets/music/builtin/`), and they play when the music folder has none. The folder is `audio.music_dir`, or else the first that exists of `$XDG_DATA_HOME/koi-pond/music`, `music/` beside the binary, `../share/koi-pond/music` beside it, and the repository's `assets/music` for a binary in `target/release`.

The full set of mp3 files is not in git. `scripts/fetch-music.sh` downloads every track in `assets/music/tracks.json` from its `download` URL into `$XDG_DATA_HOME/koi-pond/music`, with the track list and credits (`--dest DIR` downloads elsewhere, `--force` downloads them again, `--dry-run` only lists what it would fetch). Credits are in `assets/music/CREDITS.md`.

## Building without libasound2-dev

rodio links against ALSA, and its build finds it through pkg-config, which `libasound2-dev` provides. Without the package, a two-file shim over the runtime library works:

```sh
mkdir -p .alsa-shim
ln -s /usr/lib/x86_64-linux-gnu/libasound.so.2 .alsa-shim/libasound.so
printf 'libdir=${pcfiledir}\nName: alsa\nDescription: shim\nVersion: 1.2.0\nLibs: -L${libdir} -lasound\nCflags:\n' > .alsa-shim/alsa.pc
PKG_CONFIG_PATH=$PWD/.alsa-shim cargo build --release
```

Both `.alsa-shim` and a `.cargo/config.toml` that sets `PKG_CONFIG_PATH` are git-ignored.
