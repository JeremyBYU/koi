use crate::config::{self, Place};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the pond remembers between runs, in `$XDG_STATE_HOME/koi-pond/state.toml` (or
/// `~/.local/state/koi-pond/state.toml`, on Windows `%LOCALAPPDATA%\koi-pond\state.toml`). It
/// is not configuration: the game rewrites it.
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct State {
    /// The theme on screen when it was saved.
    pub theme: Option<String>,
    /// config.toml's `theme.name` when it was saved. If config.toml names another theme
    /// now, the user changed it since, and it wins over `theme`.
    pub config_theme: Option<String>,
    /// The music volume, 0 to 1, as last set with the volume keys. None until then, so
    /// `audio.volume` applies.
    pub volume: Option<f32>,
    pub muted: bool,
}

fn path() -> Option<PathBuf> {
    Some(config::dir(Place::State)?.join("state.toml"))
}

impl State {
    /// The saved state. A missing or unreadable file is an empty state: nothing remembered.
    pub fn load() -> State {
        path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|text| toml::from_str(&text).ok()).unwrap_or_default()
    }

    /// The error is a message for the status line. The file is replaced whole, so a crash
    /// while saving leaves the old one.
    pub fn save(&self) -> Result<(), String> {
        let path = path().ok_or("no home folder to save state in")?;
        let text = toml::to_string(self).map_err(|e| format!("state: {e}"))?;
        let temp = path.with_extension("toml.tmp");
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&temp, text))
            .and_then(|()| std::fs::rename(&temp, &path))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The theme to start with, if config.toml still names `configured`, the theme it named
    /// when this was saved. A theme named there since wins.
    pub fn theme_for(&self, configured: &str) -> Option<&str> {
        self.theme.as_deref().filter(|_| self.config_theme.as_deref() == Some(configured))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saved state reads back the same, and its theme holds until config.toml names
    /// another one.
    #[test]
    fn round_trip_and_the_remembered_theme() {
        let saved = State { theme: Some("moonlit-pond".to_string()), config_theme: Some("summer-garden".to_string()), volume: Some(0.4), muted: true };
        let state: State = toml::from_str(&toml::to_string(&saved).expect("serializes")).expect("parses");
        assert_eq!(
            (state.theme.as_deref(), state.config_theme.as_deref(), state.volume, state.muted),
            (Some("moonlit-pond"), Some("summer-garden"), Some(0.4), true)
        );
        assert_eq!(state.theme_for("summer-garden"), Some("moonlit-pond"));
        assert_eq!(state.theme_for("evening-garden"), None);
    }
}
