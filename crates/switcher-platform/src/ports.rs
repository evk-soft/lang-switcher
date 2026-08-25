//! Ports: the only surface adapters expose to the app shell. All handles are `Send`
//! because effects are dispatched from the core loop thread; implementations forward
//! calls to their owning threads (e.g. via channels + window messages) internally.

use crate::events::{BadgeImage, LangTag, LayoutId, Point, ResolvedAnchor};

#[derive(Debug, Clone, thiserror::Error)]
#[error("{detail}")]
pub struct PlatformError {
    /// Stable machine key authored by the adapter ("registry_write_denied",
    /// "hook_register_failed"). It lets the app build a `CapabilityReport` without
    /// knowing a single thing about the OS — the alternative would be parsing `detail`,
    /// which breaks on any wording change (ADR-0007).
    pub code: &'static str,
    pub detail: String,
}

impl PlatformError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Streams `PlatformEvent::LayoutChanged` into the channel supplied at construction.
pub trait LayoutMonitor: Send {
    /// One-shot read of the current layout (used at startup for the Initial event).
    fn current(&self) -> Result<(LayoutId, LangTag), PlatformError>;
}

/// Streams `PlatformEvent::PointerMoved` while armed. Disarmed by default: zero idle cost.
pub trait PointerTracker: Send {
    fn set_active(&self, active: bool);
    /// One-shot cursor position for anchor resolution.
    fn cursor_pos(&self) -> Option<Point>;
}

/// Best-effort caret position; always allowed to return `None`.
pub trait CaretLocator: Send {
    fn caret_point(&self) -> Option<Point>;
}

/// Click-through, topmost, non-activating badge window. **Owns all badge geometry**
/// (ADR-0005): the offset from the anchor point, the DPI scaling of that offset, the
/// monitor chosen for `ResolvedAnchor::Fixed`, and clamping into that monitor's work
/// area. The core never sees a pixel; the shell only picks how many of them to draw.
pub trait OverlayWindow: Send {
    /// `image.dpi` should equal `self.dpi_for(anchor)`. If it does not, the badge is still
    /// shown at the image's real pixel size — never clipped, never off screen — and
    /// `PlatformEvent::OverlayScaleChanged` is emitted so the shell can re-render.
    fn show(&self, image: &BadgeImage, anchor: ResolvedAnchor);

    /// Re-place the visible badge. Never re-rasterizes: on a DPI mismatch it moves now
    /// with the current pixel size and emits `OverlayScaleChanged`.
    fn move_to(&self, anchor: ResolvedAnchor);

    fn hide(&self);

    /// Effective DPI (96 = 100%) of the monitor this adapter *would* place `anchor` on,
    /// including the monitor it picks itself for `Fixed`. Answered **synchronously on the
    /// caller's thread** — no hop into the overlay thread, otherwise the core loop would
    /// deadlock against the overlay's message pump. Sole caller: the shell's rasterizer.
    fn dpi_for(&self, anchor: ResolvedAnchor) -> u32;
}

pub trait Autostart: Send {
    fn is_enabled(&self) -> Result<bool, PlatformError>;
    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError>;
}

/// Which cue to play on a layout switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCue {
    Ru,
    En,
    Neutral,
}

pub trait SoundPlayer: Send {
    fn play(&self, cue: SoundCue, volume: f32);
}

/// M1 caret stub: caret anchoring arrives in M2 (progressive enhancement, never promised).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullCaretLocator;

impl CaretLocator for NullCaretLocator {
    fn caret_point(&self) -> Option<Point> {
        None
    }
}
