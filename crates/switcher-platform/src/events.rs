/// Language tag like "ru-RU".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangTag(String);

impl LangTag {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn primary(&self) -> String {
        self.0
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}

/// Screen coordinate in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Opaque platform identity of a keyboard layout (the HKL value on Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutId(pub u64);

/// Which OS mechanism reported a layout change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSource {
    ShellHook,
    ForegroundChange,
    ForegroundPoll,
    Tsf,
    /// One-shot read at startup: updates the tray but never shows a badge or plays a sound.
    Initial,
}

/// Premultiplied-alpha RGBA image, row-major, top-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub rgba_premul: Vec<u8>,
}

/// Where the overlay places the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Top-left corner of the badge, physical pixels.
    At(Point),
    /// Fixed fallback anchor: bottom-right of the primary monitor with a margin.
    PrimaryBottomRight,
}

/// Flat events adapters push into the app channel. Data only — no handles, no callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    LayoutChanged {
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
    },
    PointerMoved {
        pos: Point,
    },
    /// DPI under the visible badge changed (monitor crossing or WM_DPICHANGED).
    OverlayScaleChanged {
        dpi: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_tag_primary_extracts_lowercase_primary_subtag() {
        assert_eq!(LangTag::new("ru-RU").primary(), "ru");
        assert_eq!(LangTag::new("EN_us").primary(), "en");
        assert_eq!(LangTag::new("de").primary(), "de");
        assert_eq!(LangTag::new("").primary(), "");
    }
}
