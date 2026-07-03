//! Ports: the only surface adapters expose to the app shell. All handles are `Send`
//! because effects are dispatched from the core loop thread; implementations forward
//! calls to their owning threads (e.g. via channels + window messages) internally.

use crate::events::{BadgeImage, LangTag, LayoutId, Placement, Point};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PlatformError(pub String);

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

/// Click-through, topmost, non-activating badge window.
pub trait OverlayWindow: Send {
    fn show(&self, image: &BadgeImage, placement: Placement);
    fn move_to(&self, pos: Point);
    fn hide(&self);
    /// Effective DPI at a screen point (96 = 100%).
    fn dpi_at(&self, pos: Point) -> u32;
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
