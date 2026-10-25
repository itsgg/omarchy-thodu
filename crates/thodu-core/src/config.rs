//! Thodu's settings, from ~/.config/thodu/config.toml (SPEC section 6).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusDisplay {
    #[default]
    Auto,
    Always,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// REQ-SEARCH-02: letters type labels and / starts a search.
    pub labels_only: bool,
    /// REQ-SESSION-08: how long routine source detail remains visible.
    pub status_display: StatusDisplay,
    /// REQ-ACT-05: normal wheel detents for one scroll key action.
    pub scroll_step: u16,
    /// REQ-ACT-05: Shift wheel detents for one scroll key action.
    pub scroll_shift_step: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            labels_only: true,
            status_display: StatusDisplay::Auto,
            scroll_step: 1,
            scroll_shift_step: 5,
        }
    }
}

/// The settings in `text`; an unknown key or a wrong type is an error, so a
/// typo is reported instead of silently ignored.
pub fn parse(text: &str) -> Result<Config, String> {
    let config: Config = toml::from_str(text).map_err(|e| e.message().to_string())?;
    for (name, value) in [
        ("scroll_step", config.scroll_step),
        ("scroll_shift_step", config.scroll_shift_step),
    ] {
        if !(1..=100).contains(&value) {
            return Err(format!("{name} must be between 1 and 100"));
        }
    }
    Ok(config)
}
