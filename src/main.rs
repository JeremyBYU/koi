mod config;
mod hud;
mod layers;
mod state;

use config::{Backend, Config, Show, Tmux};
use koi_audio::{Audio, Event, Settings, Status};
use koi_render::{Gpu, Poser, Water};
use koi_sim::{DT, FoodKind, Pose, School};
use koi_term::{self as term, Input};
use koi_theme::{Catalog, ROOT, Rgb, Theme};
use layers::{Grid, Layers};
use state::State;
use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

/// The sharpest koi `fish_px = 0` gives on the CPU backend.
const CPU_FISH_PX: usize = 10;

const USAGE: &str = "usage: koi [--config PATH] [--theme NAME] [--list-themes] [--print-default-config] [--version] [-h | --help]";

const HELP: &str = "A koi pond for terminals with the Kitty graphics protocol.

Options:
  --config PATH             Read this config file instead of ~/.config/koi-pond/config.toml
  --theme NAME              Start in this theme
  --list-themes             List the themes
  --print-default-config    Print the full config with every key explained
  --version                 Print the version

Default keys (change them in the [input] section of the config):
  click, f      drop food where you click, or somewhere random
  1-5           pick the food: pellets, flakes, petals, seeds, treat
  t, T          next or previous scene
  l, L          later or earlier time of day
  n, m, +, -    next track, mute, volume
  Tab           hide or show the HUD
  ?             show the help card
  r             reload the config and themes
  d             show the frame rate
  q, Ctrl-C     quit";

fn main() -> ExitCode {
    let (mut config_path, mut theme_arg) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "--theme" => match args.next() {
                Some(value) if arg == "--config" => config_path = Some(PathBuf::from(value)),
                Some(value) => theme_arg = Some(value),
                None => {
                    eprintln!("koi: `{arg}` needs a value\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--list-themes" => {
                let (catalog, warnings) = Catalog::load(config::dir().map(|d| d.join("themes")).as_deref());
                for s in catalog.summaries().iter().filter(|s| !s.hidden) {
                    println!("{:<20} {:<20} {:<24} {}", s.id, s.name, format!("{} · {}", s.family, s.time.name()), s.description);
                }
                for w in warnings {
                    eprintln!("koi: {w}");
                }
                return ExitCode::SUCCESS;
            }
            "--print-default-config" => {
                print!("{}", config::DEFAULT);
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!("{USAGE}\n\n{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("koi {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("koi: unknown argument `{arg}`\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let (cfg, mut warnings) = match config::load(config_path.as_deref()) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("koi: {e}");
            return ExitCode::from(2);
        }
    };
    // Cheap failures come before the GPU and the audio start.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("koi: stdin and stdout must be a terminal");
        return ExitCode::from(2);
    }
    if let Some(name) = &theme_arg
        && let Err(e) = Catalog::load(config::dir().map(|d| d.join("themes")).as_deref()).0.resolve(name)
    {
        eprintln!("koi: {e}");
        return ExitCode::from(2);
    }
    let in_tmux = std::env::var_os("TMUX").is_some();
    // The image id doubles as the 256-colour index of its placeholder cells. Taking it from
    // the pid keeps two ponds in panes of one window apart, most of the time.
    let tmux = match cfg.render.tmux {
        Tmux::Auto => in_tmux,
        Tmux::On => true,
        Tmux::Off => false,
    }
    .then(|| u8::try_from(16 + std::process::id() % 240).expect("under 256"));
    if tmux.is_some() && in_tmux {
        let mut query = std::process::Command::new("tmux");
        query.args(["display-message", "-p"]);
        if let Ok(pane) = std::env::var("TMUX_PANE") {
            query.args(["-t", &pane]);
        }
        if query.arg("#{allow-passthrough}").output().is_ok_and(|o| o.stdout.trim_ascii() == b"off") {
            eprintln!("koi: tmux passthrough is off, so the pond would stay blank. Run `tmux set -g allow-passthrough on`, and add that line to your tmux config (~/.tmux.conf or ~/.config/tmux/tmux.conf) to keep it.");
            return ExitCode::from(2);
        }
    }
    if tmux.is_none()
        && let Err(e) = term::probe_images(Duration::from_secs(2))
    {
        eprintln!("koi: {e}");
        return ExitCode::from(2);
    }
    let gpu = match cfg.render.backend {
        Backend::Gpu => match Gpu::new() {
            Ok(gpu) => Some(gpu),
            Err(e) => {
                warnings.push(format!("{e}; rendering on the CPU"));
                None
            }
        },
        Backend::Cpu => None,
    };

    term::remove_stale_shm();
    let state = State::load();
    let audio = cfg.audio.enabled.then(|| {
        Audio::start(Settings {
            music_dir: cfg.audio.music_dir.clone(),
            volume: state.volume.unwrap_or(cfg.audio.volume),
            muted: state.muted,
            ambient_volume: cfg.audio.ambient_volume,
            chime: cfg.audio.chime,
            normalize: cfg.audio.normalize,
        })
    });
    let result = run(cfg, config_path.as_deref(), theme_arg, state, gpu.as_ref(), audio.as_ref(), tmux, &mut warnings);
    if let Some(audio) = audio {
        audio.shutdown();
    }
    for w in &warnings {
        eprintln!("koi: {w}");
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("koi: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Scene {
    water: Water,
    school: School,
    poser: Poser,
    layers: Layers,
    /// The koi before the last simulation step. Frames blend from these to the current koi.
    before: Vec<Pose>,
}

/// Builds the pond for the current window size and appends the startup images to `out`.
/// With `keep`, the koi and the waves of that scene carry over to `theme`'s grid, on the
/// same window: a switch between a painted theme and a pixel theme, or between two pixel
/// sizes, needs new water, koi sprites and layers, but the same pond. `tmux` is the image
/// id of tmux mode.
fn build(cfg: &Config, theme: &Theme, gpu: Option<&Gpu>, seed: u64, out: &mut Vec<u8>, keep: Option<Scene>, tmux: Option<u8>) -> io::Result<Scene> {
    let water_px = cfg.render.water_px.max(1);
    // Posing on the CPU costs the square of `fish_px`, so a HiDPI window's own pixels would
    // not hold the frame rate there.
    let fish_px_for = |cell_w: usize| match (cfg.render.fish_px, gpu) {
        (0, Some(_)) => cell_w,
        (0, None) => cell_w.min(CPU_FISH_PX),
        (px, _) => px,
    };
    let grid = match &keep {
        Some(scene) => scene.layers.grid.clone(),
        None => {
            let (cols, rows, xpixel, ypixel) = term::winsize();
            let (cols, rows) = (cols.max(1), rows.max(1));
            let (cell_w, cell_h) = if xpixel >= cols && ypixel >= rows { (xpixel / cols, ypixel / rows) } else { (10, 20) };
            // In tmux the whole window is one frame at the koi's resolution, `fish_px` to a
            // cell width, which the terminal scales to its cells.
            let (cell_w, cell_h) = match tmux {
                Some(_) => (fish_px_for(cell_w), ((cell_h * fish_px_for(cell_w) + cell_w / 2) / cell_w).max(1)),
                None => (cell_w, cell_h),
            };
            Grid { cols, rows, cell_w, cell_h, water_w: cols * water_px, water_h: ((rows * water_px * cell_h + cell_w / 2) / cell_w).max(3) }
        }
    };
    // The koi swim on the simulation grid. A painted theme renders the water on it too; a
    // pixel theme renders it on the art grid, `pixel_px` screen pixels to an art pixel.
    let pixel = theme.style.pixel_px as usize;
    let (w, h) = if pixel > 0 { ((grid.cols * grid.cell_w).div_ceil(pixel), (grid.rows * grid.cell_h).div_ceil(pixel)) } else { (grid.water_w, grid.water_h) };
    let per_sim = [w as f32 / grid.water_w as f32, h as f32 / grid.water_h as f32];
    let ratio = water_px as f32 / 8.0 * per_sim[0];
    let (water, school, before) = match keep {
        Some(scene) => {
            let mut water = scene.water;
            water.regrid(w, h, per_sim, ratio, theme);
            (water, scene.school, scene.before)
        }
        None => {
            let mut school = School::new(grid.water_w, grid.water_h, cfg.pond.koi, seed);
            school.speed = cfg.pond.speed;
            school.calmness = cfg.pond.calmness;
            let before = school.fish.iter().map(|f| f.pose()).collect();
            (Water::new(gpu, w, h, per_sim, ratio, theme, seed), school, before)
        }
    };
    let fish_px = fish_px_for(grid.cell_w);
    let scale = if pixel > 0 { per_sim[0] } else { (grid.cols * grid.cell_w) as f32 / grid.water_w as f32 * fish_px as f32 / grid.cell_w as f32 };
    let poser = Poser::new(gpu, &school, theme, scale);
    let layers = Layers::new(out, grid, &school, &poser, &theme.palette, fish_px, pixel, (w, h), tmux)?;
    Ok(Scene { water, school, poser, layers, before })
}

fn percentile(samples: &VecDeque<f32>, p: f32) -> f32 {
    let mut sorted: Vec<f32> = samples.iter().copied().collect();
    sorted.sort_by(f32::total_cmp);
    sorted.get(((sorted.len().max(1) - 1) as f32 * p).round() as usize).copied().unwrap_or(0.0)
}

/// A message on the top line that replaces the others until `until`.
struct Toast {
    text: String,
    until: Instant,
    error: bool,
}

impl Toast {
    fn new(text: String, error: bool) -> Toast {
        Toast { text, until: Instant::now() + Duration::from_secs_f32(if error { 6.0 } else { 2.5 }), error }
    }
}

/// Modification times of `paths`, for hot reload. A missing file counts too, so creating
/// one is a change.
fn stamps(paths: &[PathBuf]) -> Vec<Option<SystemTime>> {
    paths.iter().map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok()).collect()
}

/// `tmux` is the image id of tmux mode.
#[allow(clippy::too_many_arguments)]
fn run(mut cfg: Config, config_path: Option<&Path>, theme_arg: Option<String>, mut state: State, gpu: Option<&Gpu>, audio: Option<&Audio>, tmux: Option<u8>, warnings: &mut Vec<String>) -> io::Result<()> {
    let themes_dir = config::dir().map(|d| d.join("themes"));
    let config_file = config_path.map(Path::to_path_buf).or_else(|| config::dir().map(|d| d.join("config.toml")));
    let (mut catalog, found) = Catalog::load(themes_dir.as_deref());
    warnings.extend(found);
    let wanted = theme_arg.as_deref().or(state.theme_for(&cfg.theme.name)).unwrap_or(&cfg.theme.name);
    let mut theme = match catalog.resolve(wanted) {
        Ok(theme) => theme,
        Err(e) => {
            warnings.push(format!("{e} (using Summer Garden)"));
            // A user file can replace even the root, so fall back to the built-in one.
            Catalog::load(None).0.resolve(ROOT).expect("the built-in root theme resolves")
        }
    };
    warnings.extend(theme.warnings.iter().cloned());
    let watch = |theme: &Theme| -> Vec<PathBuf> { theme.files.iter().chain(&themes_dir).chain(&config_file).cloned().collect() };
    let mut watched = watch(&theme);
    let mut seen = stamps(&watched);
    let mut polled = Instant::now();
    let mut toast: Option<Toast> = None;

    let _terminal = term::Terminal::enter(cfg.input.mouse, tmux)?;
    if cfg.input.mouse && cfg.hud.hover {
        term::report_motion();
    }
    let mut stdout = io::stdout().lock();
    let seed = match cfg.pond.seed {
        0 => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64),
        seed => seed,
    };
    // Read before the build so a resize during it is caught by the next frame.
    let mut size = term::winsize();
    let mut out = Vec::new();
    let mut scene = build(&cfg, &theme, gpu, seed, &mut out, None, tmux)?;
    stdout.write_all(&out)?;
    stdout.flush()?;
    let mut hud = hud::Hud::new(&cfg, &scene.layers.grid, &theme, &catalog, state.volume.unwrap_or(cfg.audio.volume), state.muted, Instant::now());

    let started = Instant::now();
    let (mut sim_time, mut last_present, mut last_input) = (started, started, started);
    let mut last_water: Option<Instant> = None;
    let mut last_food_change: Option<Instant> = None;
    let mut focused = true;
    let mut show_stats = false;
    // The start of an escape sequence the last read cut off.
    let mut pending = Vec::new();
    // Why no music plays, for the stats line.
    let mut no_music = String::new();
    let water_interval = Duration::from_secs_f64(1.0 / f64::from(cfg.render.water_fps.max(1)));

    let mut build_ms: VecDeque<f32> = VecDeque::new();
    let (mut frames, mut sent, mut sent_bytes) = (0u32, 0u32, 0usize);
    let (mut fps, mut sent_fps, mut bytes_per_frame, mut shm_rate, mut pose_rate) = (0.0, 0.0, 0, 0.0, 0.0);
    let (mut stats_since, mut shm_mark, mut pose_mark) = (started, 0, 0);

    loop {
        let now = Instant::now();
        let food_in_water = scene.school.food.iter().any(|p| p.kind == FoodKind::Pellets);
        let recent_input = now - last_input < Duration::from_secs_f64(cfg.fps.input_secs);
        let ripples = last_food_change.is_some_and(|t| now - t < Duration::from_secs_f64(cfg.fps.ripple_secs));
        let darting = scene.school.max_speed_ratio() > cfg.fps.dart_speed;
        let hud_fading = hud.settings.show != Show::Hidden && hud.fading(now);
        let reason = [(focused, "focused"), (food_in_water, "food"), (recent_input, "input"), (ripples, "ripples"), (darting, "darting"), (hud_fading, "hud")].iter().find(|r| r.0).map_or("calm", |r| r.1);
        let target = if reason == "calm" { cfg.fps.unfocused_calm } else { cfg.fps.focused }.max(1);
        let interval = Duration::from_secs_f64(1.0 / f64::from(target));
        let due = last_present + interval;

        pending.extend(term::read_input(due.saturating_duration_since(now)));
        if term::QUIT.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = Instant::now();
        let mut switch_to: Option<String> = None;
        let mut reload = false;
        let (events, used) = term::parse_input(&pending);
        pending.drain(..used);
        for event in events {
            // A hidden HUD still answers its keys, but the pointer only ever meets the pond.
            let reply = if hud.settings.show != Show::Hidden || matches!(event, Input::Key(_) | Input::Escape) { hud.input(&event, now) } else { hud::Reply::Pass };
            match reply {
                hud::Reply::Took => {
                    last_input = now;
                    continue;
                }
                hud::Reply::Act(action) => {
                    last_input = now;
                    let sound = match action {
                        hud::Action::NextTrack => Some(Event::NextTrack),
                        hud::Action::Volume(true) => Some(Event::VolumeUp),
                        hud::Action::Volume(false) => Some(Event::VolumeDown),
                        hud::Action::Mute => Some(Event::ToggleMute),
                        hud::Action::Scene(forward) => {
                            switch_to = catalog.next_scene(&theme.summary, forward);
                            None
                        }
                        hud::Action::Time(later) => {
                            switch_to = catalog.next_time(&theme.summary, later);
                            None
                        }
                        hud::Action::Theme(id) => {
                            switch_to = Some(id);
                            None
                        }
                    };
                    if let (Some(audio), Some(event)) = (audio, sound) {
                        audio.send(event);
                    }
                    continue;
                }
                hud::Reply::Pass => {}
            }
            match event {
                Input::Focus(gained) => {
                    focused = gained;
                    // tmux may have redrawn the pane while away.
                    if gained && tmux.is_some() {
                        scene.layers.redraw();
                        hud.forget_text();
                    }
                }
                Input::Click { col, row } => {
                    last_input = now;
                    let food = hud.food();
                    let (cols, rows) = (scene.layers.grid.cols, scene.layers.grid.rows);
                    let (w, h) = (scene.layers.grid.water_w as f32, scene.layers.grid.water_h as f32);
                    let landed = scene.school.drop_food(food, (col as f32 - 0.5) * w / cols as f32, (row as f32 - 0.5) * h / rows as f32);
                    if landed > 0
                        && let Some(audio) = audio
                    {
                        audio.send(Event::Chime(food, (col as f32 - 0.5) / cols as f32 * 2.0 - 1.0));
                    }
                }
                Input::Key(3) => return Ok(()),
                // Ctrl-L redraws, as in a shell.
                Input::Key(12) if tmux.is_some() => {
                    scene.layers.redraw();
                    hud.forget_text();
                }
                Input::Key(byte) => {
                    last_input = now;
                    let key = char::from(byte);
                    if key == cfg.input.quit {
                        return Ok(());
                    } else if key == cfg.input.feed {
                        let food = hud.food();
                        let (x, _, landed) = scene.school.drop_food_random(food);
                        if landed > 0
                            && let Some(audio) = audio
                        {
                            audio.send(Event::Chime(food, x / scene.layers.grid.water_w as f32 * 2.0 - 1.0));
                        }
                    } else if key == cfg.input.stats {
                        show_stats = !show_stats;
                    } else if key == cfg.input.reload {
                        reload = true;
                    }
                }
                Input::Scroll { .. } | Input::Move { .. } | Input::Escape | Input::Other => {}
            }
        }

        if now - polled >= Duration::from_secs(1) {
            polled = now;
            let fresh = stamps(&watched);
            if fresh != seen {
                seen = fresh;
                reload = true;
            }
        }
        let mut problems = Vec::new();
        if reload {
            match config::load(config_path) {
                Ok((fresh, found)) => {
                    problems.extend(found);
                    if fresh.theme.name != cfg.theme.name {
                        switch_to = Some(fresh.theme.name.clone());
                    }
                    (scene.school.speed, scene.school.calmness) = (fresh.pond.speed, fresh.pond.calmness);
                    // Tab's choice between always and auto lasts until the config changes it.
                    let show = if fresh.hud.show == cfg.hud.show { hud.settings.show } else { fresh.hud.show };
                    cfg = fresh;
                    (hud.settings, hud.keys) = (cfg.hud.clone(), cfg.input.clone());
                    hud.settings.show = show;
                }
                Err(e) => problems.push(e),
            }
            let (fresh, found) = Catalog::load(themes_dir.as_deref());
            catalog = fresh;
            problems.extend(found);
            switch_to = switch_to.or_else(|| Some(theme.summary.id.clone()));
        }
        if let Some(id) = switch_to {
            match catalog.resolve(&id) {
                Ok(next) => {
                    // Same pond, new light: the koi, the waves and the layout carry on. A new
                    // pixel size needs a new grid for the water and the koi sprites.
                    out.clear();
                    if next.style.pixel_px == theme.style.pixel_px {
                        scene.water.set_theme(&next);
                        scene.poser.recolor(&scene.school, &next);
                        scene.layers.recolor(&mut out, &next.palette)?;
                    } else {
                        scene = build(&cfg, &next, gpu, seed, &mut out, Some(scene), tmux)?;
                        // New layers write the placeholder cells again, over the HUD's text.
                        hud.forget_text();
                        (shm_mark, pose_mark) = (0, 0);
                    }
                    stdout.write_all(&out)?;
                    last_water = None;
                    problems.extend(next.warnings.iter().cloned());
                    if next.summary.id != theme.summary.id {
                        state.theme = Some(next.summary.id.clone());
                        state.config_theme = Some(cfg.theme.name.clone());
                        if let Err(e) = state.save() {
                            problems.push(e);
                        }
                    }
                    theme = next;
                    watched = watch(&theme);
                    seen = stamps(&watched);
                    hud.set_theme(&theme, &catalog, now);
                    toast = match problems.first() {
                        Some(problem) => Some(Toast::new(problem.clone(), true)),
                        None if reload => Some(Toast::new(format!("reloaded {}", theme.summary.name), false)),
                        None => None,
                    };
                }
                Err(e) => toast = Some(Toast::new(format!("{e} (kept {})", theme.summary.name), true)),
            }
        }
        if now < due {
            continue;
        }

        // A resize keeps the seed, so the pond keeps its layout.
        let new_size = term::winsize();
        if new_size != size {
            size = new_size;
            out.clear();
            // In tmux the new frame replaces the image, and a bare Kitty command would set the
            // pane title.
            if tmux.is_none() {
                out.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\");
            }
            out.extend_from_slice(b"\x1b[2J");
            scene = build(&cfg, &theme, gpu, seed, &mut out, None, tmux)?;
            hud.resize(&scene.layers.grid, &theme);
            stdout.write_all(&out)?;
            last_water = None;
            (shm_mark, pose_mark) = (0, 0);
        }

        // The frame shows the pond at its scheduled time, not at the moment the loop woke, so
        // a late wakeup does not show as uneven spacing. A frame more than an interval late
        // starts a new schedule.
        let frame_at = if now - due < interval { due } else { now };
        let frame_start = Instant::now();
        let food_before = scene.school.food.len();
        let mut steps = 0;
        while sim_time + Duration::from_secs_f32(DT) <= frame_at && steps < 60 {
            scene.before = scene.school.fish.iter().map(|f| f.pose()).collect();
            scene.school.step();
            for splash in scene.school.splashes.drain(..) {
                scene.water.splash(splash);
            }
            scene.water.step();
            sim_time += Duration::from_secs_f32(DT);
            steps += 1;
        }
        if steps == 60 {
            sim_time = frame_at;
        }
        for splash in scene.school.splashes.drain(..) {
            scene.water.splash(splash);
        }
        if scene.school.food.len() != food_before {
            last_food_change = Some(now);
        }

        while let Some(status) = audio.and_then(|a| a.status.try_recv().ok()) {
            match status {
                Status::NowPlaying { title } => hud.track(&title, now),
                Status::Volume { level, muted } => {
                    hud.volume(level, muted, now);
                    (state.volume, state.muted) = (Some(level), muted);
                    if let Err(e) = state.save() {
                        toast = Some(Toast::new(e, true));
                    }
                }
                Status::NoMusic(why) => {
                    hud.no_music();
                    no_music = why;
                }
                Status::Error(why) => toast = Some(Toast::new(why, true)),
            }
        }

        let palette = &theme.palette;
        let shown = toast.as_ref().filter(|t| now < t.until);
        let text = if show_stats {
            let backend = gpu.map_or("cpu".to_string(), |g| format!("gpu {}", g.adapter));
            let music = if no_music.is_empty() { String::new() } else { format!(" | {no_music}") };
            // The most useful fields first, since a narrow window cuts the line.
            Some((
                format!(
                    " {} | {backend}{music} | {fps:.1} fps, {sent_fps:.1} sent | target {target} ({reason}) | {} | build p50 {:.2}ms p99 {:.2}ms | {bytes_per_frame} B/frame | shm {shm_rate:.1} MB/s | poses {pose_rate:.0}/s | water {}x{}",
                    theme.summary.id,
                    if focused { "focused" } else { "unfocused" },
                    percentile(&build_ms, 0.5),
                    percentile(&build_ms, 0.99),
                    scene.water.w,
                    scene.water.h,
                ),
                palette.deep,
                palette.ui_text,
            ))
        } else if let Some(t) = shown {
            Some((format!(" {}", t.text), palette.shadow, if t.error { palette.ui_accent } else { palette.ui_text }))
        } else {
            warnings.first().filter(|_| now - started < Duration::from_secs(10)).map(|first| (format!(" {first} (details on exit)"), palette.deep, palette.ui_text))
        };
        // One line only: a wrapped second line would not be cleared by the next update.
        let overlay = text.map(|(t, [br, bg, bb], [fr, fg, fb]): (String, Rgb, Rgb)| {
            format!("\x1b[48;2;{br};{bg};{bb}m\x1b[38;2;{fr};{fg};{fb}m{} \x1b[0m", t.chars().take(scene.layers.grid.cols.saturating_sub(1)).collect::<String>())
        });

        let blend = ((frame_at - sim_time).as_secs_f32() / DT).min(1.0);
        let poses: Vec<Pose> = scene.before.iter().zip(&scene.school.fish).map(|(before, f)| before.lerp(&f.pose(), blend)).collect();
        let (w, h) = (scene.water.w, scene.water.h);
        let water = if last_water.is_none_or(|t| now - t >= water_interval) {
            last_water = Some(now);
            let shadows = scene.school.shadows();
            Some(scene.water.render(&shadows))
        } else {
            None
        };
        out.clear();
        out.extend_from_slice(b"\x1b[?2026h");
        let header = out.len();
        match tmux {
            Some(id) => {
                if let Some(frame) = scene.layers.compose(&mut out, &scene.school, &poses, &mut scene.poser, water.map(|rgba| (rgba, w, h)))
                    && hud.settings.show != Show::Hidden
                {
                    hud.compose(frame, &mut out, id, now);
                }
                scene.layers.send(&mut out, overlay.as_deref())?;
            }
            None => {
                scene.layers.encode(&mut out, &scene.school, &poses, &mut scene.poser, water.map(|rgba| (rgba, w, h)), overlay.as_deref(), cfg.fps.send_when_unchanged)?;
                if hud.settings.show != Show::Hidden {
                    hud.draw(&mut out, &mut scene.layers.ring, now)?;
                }
            }
        }
        if out.len() > header {
            out.extend_from_slice(b"\x1b[?2026l");
            stdout.write_all(&out)?;
            stdout.flush()?;
            sent += 1;
            sent_bytes += out.len();
        }

        build_ms.push_back(frame_start.elapsed().as_secs_f32() * 1000.0);
        if build_ms.len() > 120 {
            build_ms.pop_front();
        }
        frames += 1;
        let elapsed = now - stats_since;
        if elapsed >= Duration::from_secs(1) {
            let seconds = elapsed.as_secs_f32();
            fps = frames as f32 / seconds;
            sent_fps = sent as f32 / seconds;
            bytes_per_frame = sent_bytes / sent.max(1) as usize;
            shm_rate = (scene.layers.ring.bytes - shm_mark) as f32 / seconds / 1e6;
            pose_rate = (scene.layers.renders - pose_mark) as f32 / seconds;
            (shm_mark, pose_mark) = (scene.layers.ring.bytes, scene.layers.renders);
            (frames, sent, sent_bytes, stats_since) = (0, 0, 0, now);
        }
        last_present = frame_at;
    }
}
