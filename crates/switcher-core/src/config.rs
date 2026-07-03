//! Config model: schema v1. Parsing is tolerant (missing fields -> defaults, unknown
//! fields ignored); sanitize() clamps bad values and reports what it fixed. Schema
//! migrations will live in from_toml_str as `if cfg.version < N` blocks.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::content::{BadgeStyle, parse_hex_rgb};

pub const CONFIG_VERSION: u32 = 1;
pub const MIN_SHOW_MS: u64 = 200;
pub const MAX_SHOW_MS: u64 = 10_000;
pub const DEFAULT_SHOW_MS: u64 = 1_500;
const DEFAULT_VOLUME: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeMode {
    Transient,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorPref {
    Auto,
    Cursor,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BadgeConfig {
    pub mode: BadgeMode,
    pub anchor: AnchorPref,
    pub style: BadgeStyle,
    pub show_ms: u64,
    /// Primary language subtag -> "#RRGGBB" badge background override.
    pub colors: BTreeMap<String, String>,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        Self {
            mode: BadgeMode::Transient,
            anchor: AnchorPref::Auto,
            style: BadgeStyle::Text,
            show_ms: DEFAULT_SHOW_MS,
            colors: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: DEFAULT_VOLUME,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub badge: BadgeConfig,
    pub sound: SoundConfig,
    pub autostart: bool,
    pub log_level: String,
    pub ui_language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            badge: BadgeConfig::default(),
            sound: SoundConfig::default(),
            autostart: false,
            log_level: "info".to_owned(),
            ui_language: "ru".to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    Parse(String),
    #[error("config version {found} is newer than supported {supported}")]
    TooNew { found: u32, supported: u32 },
}

impl Config {
    /// Parses TOML, migrates old schema versions, clamps invalid values.
    /// Returns the config plus a human-readable warning per fixed value.
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<String>), ConfigError> {
        let mut cfg: Config =
            toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if cfg.version > CONFIG_VERSION {
            return Err(ConfigError::TooNew {
                found: cfg.version,
                supported: CONFIG_VERSION,
            });
        }
        cfg.version = CONFIG_VERSION;
        let warnings = cfg.sanitize();
        Ok((cfg, warnings))
    }

    pub fn to_toml_string(&self) -> String {
        toml::to_string_pretty(self).expect("config model always serializes")
    }

    fn sanitize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !(MIN_SHOW_MS..=MAX_SHOW_MS).contains(&self.badge.show_ms) {
            warnings.push(format!(
                "badge.show_ms {} out of range {MIN_SHOW_MS}..={MAX_SHOW_MS}, clamped",
                self.badge.show_ms
            ));
            self.badge.show_ms = self.badge.show_ms.clamp(MIN_SHOW_MS, MAX_SHOW_MS);
        }
        if !self.sound.volume.is_finite() {
            warnings.push("sound.volume is not a number, reset to default".to_owned());
            self.sound.volume = DEFAULT_VOLUME;
        } else if !(0.0..=1.0).contains(&self.sound.volume) {
            warnings.push(format!(
                "sound.volume {} out of range 0..=1, clamped",
                self.sound.volume
            ));
            self.sound.volume = self.sound.volume.clamp(0.0, 1.0);
        }
        let invalid: Vec<String> = self
            .badge
            .colors
            .iter()
            .filter(|(_, hex)| parse_hex_rgb(hex).is_none())
            .map(|(lang, _)| lang.clone())
            .collect();
        for lang in invalid {
            self.badge.colors.remove(&lang);
            warnings.push(format!(
                "badge.colors.{lang}: invalid #RRGGBB value removed"
            ));
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let cfg = Config::default();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.badge.mode, BadgeMode::Transient);
        assert_eq!(cfg.badge.anchor, AnchorPref::Auto);
        assert_eq!(cfg.badge.style, crate::content::BadgeStyle::Text);
        assert_eq!(cfg.badge.show_ms, DEFAULT_SHOW_MS);
        assert!(cfg.badge.colors.is_empty());
        assert!(cfg.sound.enabled);
        assert_eq!(cfg.sound.volume, 0.4);
        assert!(!cfg.autostart);
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.ui_language, "ru");
    }

    #[test]
    fn empty_and_partial_toml_fill_defaults() {
        let (cfg, warnings) = Config::from_toml_str("").unwrap();
        assert_eq!(cfg, Config::default());
        assert!(warnings.is_empty());

        let (cfg, warnings) = Config::from_toml_str("[badge]\nmode = \"follow\"\n").unwrap();
        assert_eq!(cfg.badge.mode, BadgeMode::Follow);
        assert_eq!(cfg.badge.show_ms, DEFAULT_SHOW_MS);
        assert!(warnings.is_empty());
    }

    #[test]
    fn roundtrip_preserves_config() {
        let mut cfg = Config::default();
        cfg.badge.mode = BadgeMode::Follow;
        cfg.badge.anchor = AnchorPref::Cursor;
        cfg.badge.show_ms = 900;
        cfg.badge
            .colors
            .insert("ru".to_owned(), "#112233".to_owned());
        cfg.sound.volume = 0.75;
        cfg.autostart = true;
        let (parsed, warnings) = Config::from_toml_str(&cfg.to_toml_string()).unwrap();
        assert_eq!(parsed, cfg);
        assert!(warnings.is_empty());
    }

    #[test]
    fn newer_schema_version_is_rejected() {
        let err = Config::from_toml_str("version = 99\n").unwrap_err();
        assert!(matches!(
            err,
            ConfigError::TooNew {
                found: 99,
                supported: CONFIG_VERSION
            }
        ));
    }

    #[test]
    fn malformed_toml_is_a_parse_error() {
        assert!(matches!(
            Config::from_toml_str("badge = {"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn out_of_range_values_are_clamped_with_warnings() {
        let text = "[badge]\nshow_ms = 50\n[sound]\nvolume = 3.5\n";
        let (cfg, warnings) = Config::from_toml_str(text).unwrap();
        assert_eq!(cfg.badge.show_ms, MIN_SHOW_MS);
        assert_eq!(cfg.sound.volume, 1.0);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn non_finite_volume_resets_to_default() {
        let (cfg, warnings) = Config::from_toml_str("[sound]\nvolume = nan\n").unwrap();
        assert_eq!(cfg.sound.volume, 0.4);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn invalid_badge_colors_are_removed_with_warning() {
        let text = "[badge.colors]\nru = \"#112233\"\nen = \"red\"\n";
        let (cfg, warnings) = Config::from_toml_str(text).unwrap();
        assert_eq!(
            cfg.badge.colors.get("ru").map(String::as_str),
            Some("#112233")
        );
        assert!(!cfg.badge.colors.contains_key("en"));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let (cfg, _) = Config::from_toml_str("future_field = 42\n").unwrap();
        assert_eq!(cfg, Config::default());
    }
}
