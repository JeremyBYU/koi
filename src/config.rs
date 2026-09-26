use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The compiled-in defaults, and what --print-default-config prints. A user file is merged
/// over this, so every key here is optional in the user's file.
pub const DEFAULT: &str = r##"# koi-pond configuration
#
# Every key is optional. A missing key, or a missing file, uses the value shown here.
# Default location: $XDG_CONFIG_HOME/koi-pond/config.toml (or ~/.config/koi-pond/config.toml).
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
# Inside tmux the pond is drawn as one image per frame, shown through placeholder cells so
# it stays in its pane. tmux must have "set -g allow-passthrough on". fish_px sets the
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
# Your own themes go in $XDG_CONFIG_HOME/koi-pond/themes/<id>.toml (or ~/.config/...). A
# three-line file is enough, and one with a built-in's id replaces it:
#   name = "My Garden"
#   [palette]
#   koi_red = "#EE6343"
# Theme files and this file are reloaded when they change. Changes to [render], [audio],
# pond.koi and pond.seed apply on the next start.
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
# Empty plays the music that comes with the game.
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
feed = "f"
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

#[derive(Deserialize)]
pub struct Render {
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
    pub feed: char,
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

/// `$XDG_CONFIG_HOME/koi-pond`, or `~/.config/koi-pond`.
pub fn dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
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
        None => dir().map(|d| d.join("config.toml")).filter(|p| p.exists()),
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
                // hud.enabled became hud.show.
                let (key, value) = match (section.as_str(), key.as_str(), value) {
                    ("hud", "enabled", toml::Value::Boolean(on)) => {
                        let show = if on { "always" } else { "hidden" };
                        warnings.push(format!("{}: `hud.enabled` is now `hud.show`; using show = \"{show}\"", path.display()));
                        ("show".to_string(), toml::Value::from(show))
                    }
                    (_, _, value) => (key, value),
                };
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
        config.audio.music_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/music");
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
        // Keys are matched a byte at a time, and the terminal sends any other character as
        // several bytes.
        let i = &self.input;
        let keys = [i.feed, i.quit, i.stats, i.next_track, i.mute, i.volume_up, i.volume_down, i.next_theme, i.prev_theme, i.later, i.earlier, i.reload, i.hud, i.help];
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
        let text = "bogus = 1\nhud = 3\n[fps]\nfocused = \"fast\"\nripple_secs = 5\ninput_secs = -1\nfrom = 2\n[render]\nbackend = \"vulkan\"\n[pond]\nkoi = -1\n[input]\nfeed = \"ff\"\nquit = \"é\"\nfood = [\"1\", \"2\"]\nstats = \"x\"\n";
        std::fs::write(&path, text).expect("write temp config");
        let loaded = load(Some(&path));
        std::fs::remove_file(&path).expect("remove temp config");
        let (config, warnings) = loaded.expect("a bad value is not an error");

        assert_eq!((config.fps.focused, config.fps.ripple_secs, config.fps.input_secs), (60, 5.0, 3.0));
        assert_eq!((config.render.backend == Backend::Gpu, config.pond.koi), (true, 5));
        assert_eq!((config.input.feed, config.input.quit, config.input.food, config.input.stats), ('f', 'q', ['1', '2', '3', '4', '5'], 'x'));
        assert!(!config.audio.music_dir.as_os_str().is_empty());
        let keys: Vec<&str> = warnings.iter().map(|w| w.strip_prefix(&format!("{}: `", path.display())).and_then(|w| w.split('`').next()).expect("names the file and key")).collect();
        assert_eq!(keys, ["bogus", "fps.focused", "fps.from", "fps.input_secs", "hud", "input.feed", "input.food", "input.quit", "pond.koi", "render.backend"]);
    }

    /// The old `hud.enabled = false` still hides the HUD, with a warning naming the new key.
    #[test]
    fn hud_enabled_becomes_show() {
        let path = std::env::temp_dir().join(format!("koi-config-enabled-{}.toml", std::process::id()));
        std::fs::write(&path, "[hud]\nenabled = false\n").expect("write temp config");
        let loaded = load(Some(&path));
        std::fs::remove_file(&path).expect("remove temp config");
        let (config, warnings) = loaded.expect("an old key is not an error");
        assert!(config.hud.show == Show::Hidden);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("`hud.enabled` is now `hud.show`"), "{}", warnings[0]);
    }
}
