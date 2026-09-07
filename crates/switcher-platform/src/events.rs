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

/// Screen coordinate in physical pixels. Physical only because the process is manifested
/// Per-Monitor-V2 (ADR-0010); without that, DPI virtualization silently rewrites every
/// coordinate in this file.
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
    /// One-shot startup read: updates the tray, restores a Follow badge, and never plays
    /// a sound or shows a transient badge.
    Initial,
}

/// A badge rasterized for one specific DPI, ready for `UpdateLayeredWindow`.
///
/// Byte order is **BGRA with premultiplied alpha** (ADR-0006): index 0 = blue, 1 = green,
/// 2 = red, 3 = alpha, and every colour channel is already multiplied by alpha, so
/// `b <= a && g <= a && r <= a` holds for every pixel. That is exactly what a 32bpp
/// `BI_RGB` DIB expects on little-endian Windows together with `AC_SRC_ALPHA` — do NOT
/// demultiply. Rows are top-down with stride `width * 4` and no padding, so an adapter
/// copies the whole buffer into a `biHeight = -height` DIB in one `copy_from_slice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub bgra_premul: Vec<u8>,
    /// The DPI this image was rasterized for (96 = 100%). The overlay adapter compares it
    /// with the DPI of the monitor it actually places the badge on; a mismatch is the sole
    /// trigger for `PlatformEvent::OverlayScaleChanged`.
    pub dpi: u32,
}

/// What the badge is anchored to. Turning this into a top-left pixel position is entirely
/// the overlay adapter's job (ADR-0005): it owns the anchor->corner offset, the DPI scaling
/// of that offset, the monitor choice for `Fixed`, and clamping into the work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Caret(Point),
    Cursor(Point),
    /// No usable anchor, or the user asked for a fixed corner: the adapter picks both the
    /// monitor (the active window's, else the primary) and the corner itself.
    Fixed,
}

/// A user-visible capability that can be lost at runtime without killing the app (ADR-0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    LayoutShellHook,
    LayoutForegroundHook,
    LayoutTsf,
    Pointer,
    Caret,
    Overlay,
    Sound,
    Autostart,
}

impl Capability {
    pub const ALL: [Capability; 8] = [
        Self::LayoutShellHook,
        Self::LayoutForegroundHook,
        Self::LayoutTsf,
        Self::Pointer,
        Self::Caret,
        Self::Overlay,
        Self::Sound,
        Self::Autostart,
    ];

    /// Stable key for log fields and tests. Never localized, never parsed for OS specifics.
    pub const fn key(self) -> &'static str {
        match self {
            Self::LayoutShellHook => "layout.shell_hook",
            Self::LayoutForegroundHook => "layout.foreground_hook",
            Self::LayoutTsf => "layout.tsf",
            Self::Pointer => "pointer",
            Self::Caret => "caret",
            Self::Overlay => "overlay",
            Self::Sound => "sound",
            Self::Autostart => "autostart",
        }
    }

    /// The three redundant layout sources: the tray aggregates them into one line.
    pub const fn is_layout_source(self) -> bool {
        matches!(
            self,
            Self::LayoutShellHook | Self::LayoutForegroundHook | Self::LayoutTsf
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    Ok,
    /// Works, but with a caveat worth telling the user about.
    Degraded,
    /// Unavailable for the rest of this process' lifetime.
    Off,
}

/// One capability's health. Adapter-authored, app-consumed. The core never sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub capability: Capability,
    pub state: CapabilityState,
    /// Stable machine key, e.g. "restart_budget_exhausted", "registry_write_denied".
    pub code: &'static str,
    /// One line for the tray. English; the app localizes by `code`, never by parsing this.
    pub detail: String,
}

/// Flat events adapters push into the app channel. Data only — no handles, no callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    /// Notification plus the source's observed snapshot. The runtime re-reads
    /// `LayoutMonitor::current()` before feeding the core: queued observations can be
    /// stale. `source` remains trigger provenance (ADR-0011).
    LayoutChanged {
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
    },
    PointerMoved {
        pos: Point,
    },
    /// The visible badge's image was rendered for a different DPI than the monitor it now
    /// sits on (monitor crossing, or `WM_DPICHANGED` while it sat still). Consumed by
    /// `switcher-app` ONLY: re-rasterize at `dpi`, then call `OverlayWindow::show` again
    /// with the last known anchor. Deliberately NOT mapped to any core event — the core
    /// never learns about DPI (ADR-0005).
    OverlayScaleChanged {
        dpi: u32,
    },
    /// An adapter lost or regained a capability. Consumed by the app shell ONLY: the core
    /// is never told, because it has no decision to make from it (ADR-0007).
    CapabilityChanged(CapabilityReport),
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

    /// `Capability::key()` ends up in log fields and in tests, so a duplicate would
    /// silently merge two capabilities into one line.
    #[test]
    fn capability_keys_are_unique_and_stable() {
        let mut keys: Vec<&str> = Capability::ALL.iter().map(|c| c.key()).collect();
        assert_eq!(keys.len(), Capability::ALL.len());
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(
            keys.len(),
            Capability::ALL.len(),
            "duplicate capability key"
        );
        assert_eq!(Capability::LayoutTsf.key(), "layout.tsf");
        assert_eq!(Capability::Autostart.key(), "autostart");
    }

    #[test]
    fn only_the_three_layout_sources_are_layout_sources() {
        let sources: Vec<Capability> = Capability::ALL
            .into_iter()
            .filter(|c| c.is_layout_source())
            .collect();
        assert_eq!(
            sources,
            vec![
                Capability::LayoutShellHook,
                Capability::LayoutForegroundHook,
                Capability::LayoutTsf,
            ]
        );
    }
}
