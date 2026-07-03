//! What the badge shows for a given language: label, colors, sound cue.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use switcher_platform::events::LangTag;
use switcher_platform::ports::SoundCue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeStyle {
    Text,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const BADGE_FG: Rgb8 = Rgb8 {
    r: 0xFF,
    g: 0xFF,
    b: 0xFF,
};
const RU_BG: Rgb8 = Rgb8 {
    r: 0xD6,
    g: 0x45,
    b: 0x45,
};
const EN_BG: Rgb8 = Rgb8 {
    r: 0x3D,
    g: 0x6F,
    b: 0xD9,
};
const FALLBACK_BG: Rgb8 = Rgb8 {
    r: 0x66,
    g: 0x66,
    b: 0x66,
};

/// Parses "#RRGGBB" (case-insensitive). Anything else is `None`.
pub fn parse_hex_rgb(s: &str) -> Option<Rgb8> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgb8 {
        r: (n >> 16) as u8,
        g: (n >> 8) as u8,
        b: n as u8,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct BadgeContent {
    pub label: String,
    pub bg: Rgb8,
    pub fg: Rgb8,
    pub style: BadgeStyle,
}

impl BadgeContent {
    /// Config colors win over the built-in palette; unknown languages get
    /// a two-letter uppercase label on a neutral background.
    pub fn for_lang(lang: &LangTag, style: BadgeStyle, colors: &BTreeMap<String, String>) -> Self {
        let primary = lang.primary();
        let label = if primary.is_empty() {
            "??".to_owned()
        } else {
            primary
                .chars()
                .take(2)
                .collect::<String>()
                .to_ascii_uppercase()
        };
        let bg = colors
            .get(&primary)
            .and_then(|hex| parse_hex_rgb(hex))
            .unwrap_or(match primary.as_str() {
                "ru" => RU_BG,
                "en" => EN_BG,
                _ => FALLBACK_BG,
            });
        Self {
            label,
            bg,
            fg: BADGE_FG,
            style,
        }
    }
}

/// Distinct cues for the two languages the product is about; neutral for the rest.
pub fn cue_for(lang: &LangTag) -> SoundCue {
    match lang.primary().as_str() {
        "ru" => SoundCue::Ru,
        "en" => SoundCue::En,
        _ => SoundCue::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use switcher_platform::events::LangTag;
    use switcher_platform::ports::SoundCue;

    #[test]
    fn parse_hex_rgb_accepts_only_hash_rrggbb() {
        assert_eq!(
            parse_hex_rgb("#D64545"),
            Some(Rgb8 {
                r: 0xD6,
                g: 0x45,
                b: 0x45
            })
        );
        assert_eq!(
            parse_hex_rgb("#d64545"),
            Some(Rgb8 {
                r: 0xD6,
                g: 0x45,
                b: 0x45
            })
        );
        assert_eq!(parse_hex_rgb("D64545"), None);
        assert_eq!(parse_hex_rgb("#D6454"), None);
        assert_eq!(parse_hex_rgb("#D645451"), None);
        assert_eq!(parse_hex_rgb("#GGGGGG"), None);
        assert_eq!(parse_hex_rgb(""), None);
    }

    #[test]
    fn known_languages_get_labels_and_builtin_palette() {
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(ru.label, "RU");
        assert_eq!(
            ru.bg,
            Rgb8 {
                r: 0xD6,
                g: 0x45,
                b: 0x45
            }
        );
        assert_eq!(ru.fg, BADGE_FG);

        let en = BadgeContent::for_lang(&LangTag::new("en-US"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(en.label, "EN");
        assert_eq!(
            en.bg,
            Rgb8 {
                r: 0x3D,
                g: 0x6F,
                b: 0xD9
            }
        );
    }

    #[test]
    fn config_color_overrides_builtin_palette() {
        let mut colors = BTreeMap::new();
        colors.insert("ru".to_owned(), "#112233".to_owned());
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Color, &colors);
        assert_eq!(
            ru.bg,
            Rgb8 {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }
        );
        assert_eq!(ru.style, BadgeStyle::Color);
    }

    #[test]
    fn invalid_config_color_falls_back_to_builtin() {
        let mut colors = BTreeMap::new();
        colors.insert("ru".to_owned(), "not-a-color".to_owned());
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Text, &colors);
        assert_eq!(
            ru.bg,
            Rgb8 {
                r: 0xD6,
                g: 0x45,
                b: 0x45
            }
        );
    }

    #[test]
    fn unknown_language_gets_uppercased_two_letter_label_and_fallback_bg() {
        let de = BadgeContent::for_lang(&LangTag::new("de-DE"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(de.label, "DE");
        assert_eq!(
            de.bg,
            Rgb8 {
                r: 0x66,
                g: 0x66,
                b: 0x66
            }
        );

        let empty = BadgeContent::for_lang(&LangTag::new(""), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(empty.label, "??");
    }

    #[test]
    fn cue_follows_primary_language() {
        assert_eq!(cue_for(&LangTag::new("ru-RU")), SoundCue::Ru);
        assert_eq!(cue_for(&LangTag::new("en-GB")), SoundCue::En);
        assert_eq!(cue_for(&LangTag::new("de-DE")), SoundCue::Neutral);
    }
}
