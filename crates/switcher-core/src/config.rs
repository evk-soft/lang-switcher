//! Config model: schema v1. Parsing is tolerant (missing fields -> defaults, unknown
//! fields ignored); sanitize() clamps bad values and reports what it fixed. Schema
//! migrations will live in from_toml_str as `if cfg.version < N` blocks.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde::{Deserialize, Serialize};
use switcher_platform::events::LangTag;

use crate::content::{BadgeStyle, parse_hex_rgb};

pub const CONFIG_VERSION: u32 = 1;
pub const MIN_SHOW_MS: u64 = 200;
pub const MAX_SHOW_MS: u64 = 10_000;
pub const DEFAULT_SHOW_MS: u64 = 1_500;
const DEFAULT_VOLUME: f32 = 0.4;
const DEFAULT_LOG_LEVEL: &str = "info";
/// `ui_language` value meaning "follow the Windows display language" (ADR-0022).
pub const UI_LANGUAGE_AUTO: &str = "auto";
/// The levels `tracing`'s `LevelFilter` parses, case-insensitively. Hardcoded on purpose:
/// `switcher-core` must not take a dependency on `tracing` just to validate a string.
const LOG_LEVELS: [&str; 6] = ["error", "warn", "info", "debug", "trace", "off"];

/// Canonicalizes a `ui_language` value: `UI_LANGUAGE_AUTO`, or a Unicode language
/// identifier in canonical case (`EN` -> `en`, `zh-hans` -> `zh-Hans`, `ru_RU` -> `ru-RU`).
///
/// Deliberately does NOT check that a catalog exists for the tag (ADR-0022): the set of
/// shipped translations lives in the shell, and a valid tag must survive a downgrade to a
/// build that carries fewer of them.
pub(crate) fn normalize_ui_language(value: &str, warnings: &mut Vec<String>) -> String {
    // `auto` is four ASCII letters, which BCP 47 accepts as a (reserved) language subtag,
    // so it has to be recognized before parsing rather than after.
    if value.eq_ignore_ascii_case(UI_LANGUAGE_AUTO) {
        return UI_LANGUAGE_AUTO.to_owned();
    }
    match value.parse::<unic_langid::LanguageIdentifier>() {
        Ok(langid) => langid.to_string(),
        Err(error) => {
            warnings.push(format!(
                "ui_language {value:?} is not a language tag ({error}), reset to {UI_LANGUAGE_AUTO:?}"
            ));
            UI_LANGUAGE_AUTO.to_owned()
        }
    }
}

/// Lower-cases `value` and accepts it only if it is in `allowed`; otherwise falls back to
/// `default` and says so. Without this, a typo in the config file travels all the way into
/// the logging setup and silently changes which levels are recorded.
fn normalize_enum(
    value: &str,
    allowed: &[&str],
    default: &str,
    field: &str,
    warnings: &mut Vec<String>,
) -> String {
    let lower = value.to_ascii_lowercase();
    if allowed.contains(&lower.as_str()) {
        return lower;
    }
    warnings.push(format!(
        "{field} {value:?} is not one of {allowed:?}, reset to {default:?}"
    ));
    default.to_owned()
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub fallback_enabled: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            fallback_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub badge: BadgeConfig,
    pub sound: SoundConfig,
    pub layout: LayoutConfig,
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
            layout: LayoutConfig::default(),
            autostart: false,
            log_level: DEFAULT_LOG_LEVEL.to_owned(),
            ui_language: UI_LANGUAGE_AUTO.to_owned(),
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
        // Match the content lookup exactly: case and region aliases must resolve to the
        // primary subtag. An already canonical key wins any collision; other aliases
        // are processed in BTreeMap order so the result is deterministic.
        let aliases: Vec<(String, String)> = self
            .badge
            .colors
            .keys()
            .filter_map(|lang| {
                let primary = LangTag::new(lang.as_str()).primary();
                (primary != *lang).then(|| (lang.clone(), primary))
            })
            .collect();
        for (lang, primary) in aliases {
            let Some(hex) = self.badge.colors.remove(&lang) else {
                continue;
            };
            match self.badge.colors.entry(primary) {
                Entry::Occupied(taken) => warnings.push(format!(
                    "badge.colors.{lang}: duplicates {}, dropped in favour of it",
                    taken.key()
                )),
                Entry::Vacant(slot) => {
                    warnings.push(format!(
                        "badge.colors.{lang}: keys are matched by primary language, renamed to {}",
                        slot.key()
                    ));
                    slot.insert(hex);
                }
            }
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
        let log_level = normalize_enum(
            &self.log_level,
            &LOG_LEVELS,
            DEFAULT_LOG_LEVEL,
            "log_level",
            &mut warnings,
        );
        self.log_level = log_level;
        self.ui_language = normalize_ui_language(&self.ui_language, &mut warnings);
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
        assert!(cfg.layout.fallback_enabled);
        assert!(!cfg.autostart);
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.ui_language, "auto");
    }

    #[test]
    fn old_toml_without_layout_section_enables_fallback() {
        let (cfg, warnings) =
            Config::from_toml_str("version = 1\n[badge]\nshow_ms = 900\n").unwrap();
        assert!(cfg.layout.fallback_enabled);
        assert!(warnings.is_empty());
    }

    #[test]
    fn explicit_false_disables_layout_fallback() {
        let (cfg, warnings) =
            Config::from_toml_str("[layout]\nfallback_enabled = false\n").unwrap();
        assert!(!cfg.layout.fallback_enabled);
        assert!(warnings.is_empty());
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
        cfg.layout.fallback_enabled = false;
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

    /// An upper-case key used to pass validation and then never match, because lookups go
    /// through `LangTag::primary()`, which lower-cases.
    #[test]
    fn badge_color_keys_are_normalized_to_lower_case() {
        let (cfg, warnings) = Config::from_toml_str("[badge.colors]\nRU = \"#112233\"\n").unwrap();
        assert_eq!(
            cfg.badge.colors.get("ru").map(String::as_str),
            Some("#112233")
        );
        assert!(!cfg.badge.colors.contains_key("RU"));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn duplicate_badge_color_keys_keep_the_lower_case_one() {
        let text = "[badge.colors]\nru = \"#112233\"\nRU = \"#445566\"\n";
        let (cfg, warnings) = Config::from_toml_str(text).unwrap();
        assert_eq!(
            cfg.badge.colors.get("ru").map(String::as_str),
            Some("#112233"),
            "the canonical lower-case key wins, whatever the iteration order"
        );
        assert_eq!(cfg.badge.colors.len(), 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn unknown_log_level_resets_to_default_with_warning() {
        let (cfg, warnings) = Config::from_toml_str("log_level = \"verbose\"\n").unwrap();
        assert_eq!(cfg.log_level, "info");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn log_level_is_accepted_case_insensitively_and_normalized() {
        let (cfg, warnings) = Config::from_toml_str("log_level = \"WARN\"\n").unwrap();
        assert_eq!(cfg.log_level, "warn");
        assert!(warnings.is_empty());
    }

    /// ADR-0022: the core stores and canonicalizes the tag, it does not decide which
    /// catalogs exist. A valid tag with no catalog must survive so that downgrading to a
    /// build with fewer translations cannot silently rewrite the user's choice.
    #[test]
    fn valid_ui_language_tags_are_canonicalized_and_kept() {
        for (written, expected) in [
            ("auto", "auto"),
            ("AUTO", "auto"),
            ("ru", "ru"),
            ("EN", "en"),
            ("fr", "fr"),
            ("zh-hans", "zh-Hans"),
            ("ru_RU", "ru-RU"),
            ("pt-br", "pt-BR"),
            // No catalog ships for this one; the shell falls back to English at runtime.
            ("ja", "ja"),
        ] {
            let (cfg, warnings) =
                Config::from_toml_str(&format!("ui_language = {written:?}\n")).unwrap();
            assert_eq!(cfg.ui_language, expected, "input {written:?}");
            assert!(
                warnings.is_empty(),
                "input {written:?} warned: {warnings:?}"
            );
        }
    }

    #[test]
    fn malformed_ui_language_resets_to_auto_with_warning() {
        for written in ["", "not a language!", "toolongsubtag", "-", "en-"] {
            let (cfg, warnings) =
                Config::from_toml_str(&format!("ui_language = {written:?}\n")).unwrap();
            assert_eq!(cfg.ui_language, "auto", "input {written:?}");
            assert_eq!(warnings.len(), 1, "input {written:?}");
        }
    }

    /// Configs written by 0.1.0-alpha and earlier carry an explicit "ru" or "en"; both are
    /// already canonical, so an upgrade must not warn or change them.
    #[test]
    fn previously_saved_explicit_languages_survive_untouched() {
        for saved in ["ru", "en"] {
            let (cfg, warnings) =
                Config::from_toml_str(&format!("version = 1\nui_language = {saved:?}\n")).unwrap();
            assert_eq!(cfg.ui_language, saved);
            assert!(warnings.is_empty());
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let (cfg, _) = Config::from_toml_str("future_field = 42\n").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn full_language_color_keys_apply_to_badge_content() {
        let (cfg, warnings) =
            Config::from_toml_str("[badge.colors]\nru-RU = \"#123456\"\nEN_us = \"#654321\"\n")
                .unwrap();
        assert_eq!(warnings.len(), 2);
        for (tag, expected) in [("ru-RU", "#123456"), ("en-US", "#654321")] {
            let content = crate::content::BadgeContent::for_lang(
                &switcher_platform::events::LangTag::new(tag),
                cfg.badge.style,
                &cfg.badge.colors,
            );
            assert_eq!(Some(content.bg), parse_hex_rgb(expected));
        }
    }

    #[test]
    fn primary_color_key_wins_over_region_alias() {
        let (cfg, warnings) =
            Config::from_toml_str("[badge.colors]\nru = \"#123456\"\nru-RU = \"#654321\"\n")
                .unwrap();
        assert_eq!(cfg.badge.colors.len(), 1);
        assert_eq!(cfg.badge.colors["ru"], "#123456");
        assert_eq!(warnings.len(), 1);
    }
}
