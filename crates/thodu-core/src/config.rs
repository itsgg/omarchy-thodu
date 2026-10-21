//! Thodu's settings, from ~/.config/thodu/config.toml (SPEC section 6).

use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// REQ-SEARCH-02: letters type labels and / starts a search.
    pub labels_only: bool,
}

/// The settings in `text`; an unknown key or a wrong type is an error, so a
/// typo is reported instead of silently ignored.
pub fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| e.message().to_string())
}
