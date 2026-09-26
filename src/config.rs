use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The compiled-in defaults, and what --print-default-config prints. A user file is merged
/// over this, so every key here is optional in the user's file.
pub const DEFAULT: &str = r##"# koi-pond configuration
#
# Every key is optional. A missing key, or a missing file, uses the value shown here.
# Default location: $XDG_CONFIG_HOME/koi-pond/config.toml (or ~/.config/koi-pond/config.toml),
# on Windows %APPDATA%\koi-pond\config.toml.
# Another file: koi --config PATH

[fps]
# Frames per second while the window has focus, or while something is happening:
# pellets in the water, recent input, fresh ripples, or a koi darting.
focused = 60
# Frames per second while the window is unfocused and the pond is calm.
unfocused_calm = 8
# Seconds after the last key or click that still count as something happening.
input_secs = 3.0
# Seconds after food lands, is eaten or sinks that the ripples count as something happening.
ripple_secs = 8.0
# A koi faster than this multiple of its cruise speed counts as darting.
dart_speed = 1.6
# Re-send the picture even when nothing on screen changed. It costs terminal CPU for nothing.
send_when_unchanged = false

[render]
# How the pond reaches the terminal. "auto" asks the terminal and picks the best it has:
# "kitty" (Kitty graphics through shared memory: Ghostty, Kitty), "kitty-direct" (Kitty
# graphics sent inline, which also work over SSH), "sixel" (xterm, foot, mlterm, WezTerm),
# or "blocks" (coloured half-block characters, which any terminal shows). A name forces
# that one even if the terminal did not say it has it.
# koi --protocol NAME overrides this for one run.
protocol = "auto"
# "gpu" renders with wgpu on Vulkan. It falls back to "cpu" when no GPU adapter is found.
backend = "gpu"
# Water image pixels per cell width. The terminal scales the water up to the window.
water_px = 2
# Koi image pixels per cell width. 0 means the terminal's own pixels: the sharpest koi, at
# about 1.5 times the image bytes of 8. On the CPU backend 0 means at most 10, since posing
# there costs the square of this.
fish_px = 0
# Most water images sent per second. The koi still move at the full frame rate.
water_fps = 30
# Inside tmux, with Kitty graphics, the pond is drawn as one image per frame, shown through
# placeholder cells so it stays in its pane. tmux must have "set -g allow-passthrough on". fish_px sets the
# frame's size there too, so 8 is lighter on a HiDPI screen. "auto" does this when $TMUX is
# set, "on" always, "off" never.
tmux = "auto"

[pond]
koi = 5
# Swimming speed as a multiple of the calm default.
speed = 1.0
# Higher is lazier: slower turns, softer steering, longer glides. 0.5 is twice as brisk.
calmness = 1.0
# 0 picks a new pond every start. Any other number gives the same pond every time.
seed = 0

[theme]
# The starting theme. koi --list-themes lists them. After the first run the pond starts
# with the theme you last switched to, until this line names a different one.
# Your own themes go in $XDG_CONFIG_HOME/koi-pond/themes/<id>.toml (or ~/.config/..., on
# Windows %APPDATA%\koi-pond\themes). A
# three-line file is enough, and one with a built-in's id replaces it:
#   name = "My Garden"
#   [palette]
#   koi_red = "#EE6343"
# Theme files and this file are reloaded when they change. Changes to [render], [audio],
# hud.hover, input.mouse, pond.koi and pond.seed apply on the next start.
name = "summer-garden"

[hud]
# The row of stones at the bottom for music, food, scene, time of day and help.
# "always" keeps the row on screen. "auto" hides it, and a HUD key shows only the stone it
# changed. "hidden" draws nothing; the keys still work. Tab switches between "always" and
# "auto" until the pond closes.
show = "always"
# Resting the pointer in the bottom 4 rows for half a second shows the row.
hover = false
# "center" or "left".
align = "center"
# false shows and hides at once.
fade = true
# Seconds a single stone shows after something changes (a food key, a new track).
peek_secs = 3.0
# Seconds the row stays after the last HUD key or click, once the pointer is away.
hold_secs = 4.0

[audio]
enabled = true
# Every mp3 and ogg in this folder plays in shuffled order. tracks.json there adds titles.
# Empty looks in $XDG_DATA_HOME/koi-pond/music (or ~/.local/share/koi-pond/music, on Windows
# %LOCALAPPDATA%\koi-pond\music), where scripts/fetch-music.sh puts the game's music, then in
# a music folder beside the binary.
music_dir = ""
# Music volume, 0.0 to 1.0.
volume = 0.7
# Generated ambient layer (drone, pad, water, drops), as a fraction of the music volume.
ambient_volume = 0.6
# A chime when food lands, one voice per food kind.
chime = true
# Play every track at about the same loudness.
normalize = true

[input]
# Click the pond to drop food.
mouse = true
# Pressing on a koi pets it instead of feeding: hold to keep your hand in the water, drag to
# lead the koi. false makes every click drop food.
pet_click = true
feed = "f"
# A hand in the middle of the pond, for the nearest koi to come and nuzzle.
pet = "p"
quit = "q"
stats = "d"
next_track = "n"
mute = "m"
volume_up = "+"
volume_down = "-"
# Next and previous scene (a family of themes), later and earlier time of day in it, and
# reload the theme and this file.
next_theme = "t"
prev_theme = "T"
later = "l"
earlier = "L"
reload = "r"
# Show or hide the HUD, the help card, and the five foods: pellets, flakes, petals, seeds
# and a treat.
hud = "\t"
help = "?"
food = ["1", "2", "3", "4", "5"]
"##;

#[derive(Deserialize)]
pub struct Config {
    pub fps: Fps,
    pub render: Render,
    pub pond: Pond,
    pub theme: ThemeConfig,
    pub hud: Hud,
    pub audio: Audio,
    pub input: Input,
}

#[derive(Deserialize)]
pub struct Fps {
    pub focused: u32,
    pub unfocused_calm: u32,
    pub input_secs: f64,
    pub ripple_secs: f64,
    pub dart_speed: f32,
    pub send_when_unchanged: bool,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Gpu,
    Cpu,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Auto,
    Kitty,
    KittyDirect,
    Sixel,
    Blocks,
}

#[derive(Deserialize)]
pub struct Render {
    pub protocol: Protocol,
    pub backend: Backend,
    pub water_px: usize,
    pub fish_px: usize,
    pub water_fps: u32,
    pub tmux: Tmux,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tmux {
    Auto,
    On,
    Off,
}

#[derive(Deserialize)]
pub struct Pond {
    pub koi: usize,
    pub speed: f32,
    pub calmness: f32,
    pub seed: u64,
}

#[derive(Deserialize)]
pub struct ThemeConfig {
    pub name: String,
}

#[derive(Deserialize, Clone)]
pub struct Hud {
    pub show: Show,
    pub hover: bool,
    pub align: Align,
    pub fade: bool,
    pub peek_secs: f32,
    pub hold_secs: f32,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    Always,
    Auto,
    Hidden,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Center,
    Left,
}

#[derive(Deserialize)]
pub struct Audio {
    pub enabled: bool,
    pub music_dir: PathBuf,
    pub volume: f32,
    pub ambient_volume: f32,
    pub chime: bool,
    pub normalize: bool,
}

#[derive(Deserialize, Clone)]
pub struct Input {
    pub mouse: bool,
    pub pet_click: bool,
    pub feed: char,
    pub pet: char,
    pub quit: char,
    pub stats: char,
    pub next_track: char,
    pub mute: char,
    pub volume_up: char,
    pub volume_down: char,
    pub next_theme: char,
    pub prev_theme: char,
    pub later: char,
    pub earlier: char,
    pub reload: char,
    pub hud: char,
    pub help: char,
    pub food: [char; 5],
}

/// A kind of file koi-pond keeps, each kind in its own folder.
#[derive(Clone, Copy)]
pub enum Place {
    /// config.toml and the user's themes.
    Config,
    /// The music.
    Data,
    /// The state file.
    State,
    /// The loudness cache.
    Cache,
}

/// The folder for `place` (see `place_dir`), from this process's environment.
pub fn dir(place: Place) -> Option<PathBuf> {
    place_dir(place, cfg!(windows), |name| std::env::var_os(name))
}

/// The folder for `place`, reading variables with `var`. On Unix, the XDG base directory
/// (`$XDG_CONFIG_HOME/koi-pond`, or `~/.config/koi-pond` when it is unset or empty, and so on).
/// On `windows`, `%APPDATA%\koi-pond` for the config, which roams with the user, and
/// `%LOCALAPPDATA%\koi-pond` for the rest. None when the variable it needs is missing.
fn place_dir(place: Place, windows: bool, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let base = if windows {
        PathBuf::from(var(if matches!(place, Place::Config) { "APPDATA" } else { "LOCALAPPDATA" }).filter(|dir| !dir.is_empty())?)
    } else {
        let (xdg, home) = match place {
            Place::Config => ("XDG_CONFIG_HOME", ".config"),
            Place::Data => ("XDG_DATA_HOME", ".local/share"),
            Place::State => ("XDG_STATE_HOME", ".local/state"),
            Place::Cache => ("XDG_CACHE_HOME", ".cache"),
        };
        match var(xdg) {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => PathBuf::from(var("HOME")?).join(home),
        }
    };
    Some(base.join("koi-pond"))
}

/// Loads `explicit`, or the default path if that exists, over the defaults. A key that is
/// unknown or has a value the game cannot use comes back as a warning, and the default
/// stays. Only a file that cannot be read or parsed is an error.
pub fn load(explicit: Option<&Path>) -> Result<(Config, Vec<String>), String> {
    let mut table: toml::Table = DEFAULT.parse().map_err(|e| format!("built-in default config: {e}"))?;
    let mut warnings = Vec::new();
    let path = match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => dir(Place::Config).map(|d| d.join("config.toml")).filter(|p| p.exists()),
    };
    if let Some(path) = &path {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let user: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
        for (section, values) in user {
            let toml::Value::Table(values) = values else {
                let problem = if table.contains_key(&section) { format!("expects a table, got {}", values.type_str()) } else { "unknown key".to_string() };
                warnings.push(format!("{}: `{section}` {problem}; ignored", path.display()));
                continue;
            };
            for (key, value) in values {
                let Some(default) = table.get(&section).and_then(|s| s.get(&key)).cloned() else {
                    warnings.push(format!("{}: `{section}.{key}` unknown key; ignored", path.display()));
                    continue;
                };
                // Each value is tried in a whole config, so serde and `check` judge it with
                // its real type and every other value already accepted.
                let mut trial = table.clone();
                trial[&section][&key] = value;
                match Config::deserialize(trial.clone()).map_err(|e| e.message().to_string()).and_then(|config| config.check()) {
                    Ok(()) => table = trial,
                    Err(e) => warnings.push(format!("{}: `{section}.{key}` {e}; using the default {default}", path.display())),
                }
            }
        }
    }
    let mut config = Config::deserialize(table).map_err(|e| format!("config: {e}"))?;
    if config.audio.music_dir.as_os_str().is_empty() {
        let data = dir(Place::Data).map(|d| d.join("music"));
        let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        // Beside the binary as unpacked from a release archive, under an install prefix, and
        // the repository's own assets/music for a binary in target/release.
        let beside = exe_dir.iter().flat_map(|d| [d.join("music"), d.join("../share/koi-pond/music"), d.join("../../assets/music")]);
        let candidates: Vec<PathBuf> = data.iter().cloned().chain(beside).collect();
        // With none of them there, the stats line names the first place to put music.
        config.audio.music_dir = candidates.iter().find(|d| d.is_dir()).or(candidates.first()).cloned().unwrap_or_default();
    }
    Ok((config, warnings))
}

impl Config {
    /// What serde cannot tell from the types.
    fn check(&self) -> Result<(), String> {
        // `Duration` panics on a negative, NaN or huge number of seconds.
        for secs in [self.fps.input_secs, self.fps.ripple_secs, f64::from(self.hud.peek_secs), f64::from(self.hud.hold_secs)] {
            if !(0.0..=86_400.0).contains(&secs) {
                return Err(format!("expects seconds from 0 to 86400, got {secs}"));
            }
        }
        // Each koi and each water pixel costs shared memory, which is RAM.
        if self.pond.koi > 50 {
            return Err(format!("expects at most 50 koi, got {}", self.pond.koi));
        }
        if self.render.water_px > 16 || self.render.fish_px > 64 {
            return Err(format!("expects water_px up to 16 and fish_px up to 64, got {} and {}", self.render.water_px, self.render.fish_px));
        }
        for level in [self.audio.volume, self.audio.ambient_volume] {
            if !(0.0..=1.0).contains(&level) {
                return Err(format!("expects a level from 0.0 to 1.0, got {level}"));
            }
        }
        // Keys are matched a byte at a time, and the terminal sends any other character as
        // several bytes.
        let i = &self.input;
        let keys = [i.feed, i.pet, i.quit, i.stats, i.next_track, i.mute, i.volume_up, i.volume_down, i.next_theme, i.prev_theme, i.later, i.earlier, i.reload, i.hud, i.help];
        if let Some(key) = keys.iter().chain(&i.food).find(|k| !k.is_ascii()) {
            return Err(format!("expects an ASCII key, got {key:?}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every problem in a user file is a warning naming the file and key, and leaves that
    /// key's default. Good values beside them still apply.
    #[test]
    fn bad_values_warn_and_keep_the_default() {
        let path = std::env::temp_dir().join(format!("koi-config-test-{}.toml", std::process::id()));
        let text = "bogus = 1\nhud = 3\n[fps]\nfocused = \"fast\"\nripple_secs = 5\ninput_secs = -1\nfrom = 2\n[render]\nbackend = \"vulkan\"\nprotocol = \"iterm\"\n[pond]\nkoi = -1\n[audio]\nambient_volume = 3\n[input]\nfeed = \"ff\"\nquit = \"é\"\nfood = [\"1\", \"2\"]\nstats = \"x\"\n";
        std::fs::write(&path, text).expect("write temp config");
        let loaded = load(Some(&path));
        std::fs::remove_file(&path).expect("remove temp config");
        let (config, warnings) = loaded.expect("a bad value is not an error");

        assert_eq!((config.fps.focused, config.fps.ripple_secs, config.fps.input_secs), (60, 5.0, 3.0));
        assert_eq!((config.render.backend == Backend::Gpu, config.pond.koi), (true, 5));
        assert_eq!((config.input.feed, config.input.quit, config.input.food, config.input.stats), ('f', 'q', ['1', '2', '3', '4', '5'], 'x'));
        assert!(!config.audio.music_dir.as_os_str().is_empty());
        let keys: Vec<&str> = warnings.iter().map(|w| w.strip_prefix(&format!("{}: `", path.display())).and_then(|w| w.split('`').next()).expect("names the file and key")).collect();
        assert_eq!(keys, ["audio.ambient_volume", "bogus", "fps.focused", "fps.from", "fps.input_secs", "hud", "input.feed", "input.food", "input.quit", "pond.koi", "render.backend", "render.protocol"]);
    }

    /// XDG folders with a HOME fallback on Unix, and APPDATA for the config and LOCALAPPDATA
    /// for the rest on Windows, where HOME is ignored.
    #[test]
    fn places_per_platform() {
        let env = |vars: &'static [(&'static str, &'static str)]| move |name: &str| vars.iter().find(|v| v.0 == name).map(|v| OsString::from(v.1));
        let unix = |place, vars| place_dir(place, false, env(vars));
        let home = &[("HOME", "/home/k"), ("XDG_STATE_HOME", ""), ("APPDATA", "C:/Roaming")];
        assert_eq!(unix(Place::Config, home), Some(PathBuf::from("/home/k/.config/koi-pond")));
        assert_eq!(unix(Place::Data, home), Some(PathBuf::from("/home/k/.local/share/koi-pond")));
        assert_eq!(unix(Place::State, home), Some(PathBuf::from("/home/k/.local/state/koi-pond")), "an empty XDG variable is unset");
        assert_eq!(unix(Place::Cache, &[("HOME", "/home/k"), ("XDG_CACHE_HOME", "/tmp/c")]), Some(PathBuf::from("/tmp/c/koi-pond")));
        assert_eq!(unix(Place::Config, &[]), None);

        let windows = |place, vars| place_dir(place, true, env(vars));
        let profile = &[("APPDATA", "C:/Users/k/AppData/Roaming"), ("LOCALAPPDATA", "C:/Users/k/AppData/Local"), ("HOME", "/home/k"), ("XDG_CONFIG_HOME", "/x")];
        assert_eq!(windows(Place::Config, profile), Some(PathBuf::from("C:/Users/k/AppData/Roaming/koi-pond")));
        for place in [Place::Data, Place::State, Place::Cache] {
            assert_eq!(windows(place, profile), Some(PathBuf::from("C:/Users/k/AppData/Local/koi-pond")));
        }
        assert_eq!(windows(Place::Config, &[("HOME", "/home/k"), ("APPDATA", "")]), None);
    }
}
