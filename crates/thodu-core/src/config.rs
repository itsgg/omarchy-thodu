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

const SETTINGS: &str = "labels_only, status_display, scroll_step and scroll_shift_step";

/// The settings in `text`. An unknown key, a wrong type or a value out of
/// range is an error that names the key and what it takes, so a typo is
/// reported instead of silently ignored.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| match e.span() {
        Some(span) => format!(
            "line {}: {}",
            text[..span.start.min(text.len())].matches('\n').count() + 1,
            e.message()
        ),
        None => e.message().to_string(),
    })?;
    let mut config = Config::default();
    for (key, value) in &table {
        match key.as_str() {
            "labels_only" => {
                config.labels_only = value.as_bool().ok_or_else(|| {
                    format!(
                        "labels_only must be true or false, not a {}",
                        value.type_str()
                    )
                })?;
            }
            "status_display" => {
                config.status_display = match value.as_str() {
                    Some("auto") => StatusDisplay::Auto,
                    Some("always") => StatusDisplay::Always,
                    Some("off") => StatusDisplay::Off,
                    Some(other) => {
                        return Err(format!(
                            "status_display must be \"auto\", \"always\" or \"off\", not {other:?}"
                        ));
                    }
                    None => {
                        return Err(format!(
                            "status_display must be \"auto\", \"always\" or \"off\", not a {}",
                            value.type_str()
                        ));
                    }
                };
            }
            name @ ("scroll_step" | "scroll_shift_step") => {
                let Some(number) = value.as_integer() else {
                    return Err(format!(
                        "{name} must be a whole number from 1 to 100, not a {}",
                        value.type_str()
                    ));
                };
                let step = u16::try_from(number)
                    .ok()
                    .filter(|n| (1..=100).contains(n))
                    .ok_or_else(|| format!("{name} must be between 1 and 100, not {number}"))?;
                if name == "scroll_step" {
                    config.scroll_step = step;
                } else {
                    config.scroll_shift_step = step;
                }
            }
            other => {
                return Err(format!(
                    "{other} is not a setting; the settings are {SETTINGS}"
                ));
            }
        }
    }
    Ok(config)
}

/// The settings in the file at `path`, and why the defaults are used
/// instead when the file exists but cannot be read, is not UTF-8 or does
/// not parse. A missing file is the defaults, with nothing to say.
pub fn load(path: &std::path::Path) -> (Config, Option<String>) {
    let problem = |why: String| {
        (
            Config::default(),
            Some(format!("{why}; using the defaults until it is fixed")),
        )
    };
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => match parse(&text) {
                Ok(config) => (config, None),
                Err(e) => problem(format!("{}: {e}", path.display())),
            },
            Err(_) => problem(format!("{} is not UTF-8 text", path.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => problem(format!("cannot read {}: {e}", path.display())),
    }
}
