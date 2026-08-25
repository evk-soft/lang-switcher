//! Turning an anchor into the badge's top-left corner: the whole arithmetic of ADR-0005,
//! with every OS fact arriving as an argument.
//!
//! Nothing here calls Windows. The `unsafe` half — asking which monitor a point sits on,
//! what its work area is and what its effective DPI is — lives in the parent module and
//! hands the answers over as [`MonitorFacts`]. That split is the reason the placement rules
//! are testable at all: the table in this module's tests *is* the specification, and it runs
//! without a display, a monitor layout, or a window.
#![deny(unsafe_code)]

use switcher_platform::events::{BadgeImage, Point, ResolvedAnchor};

/// The DPI that "100%" means. Every gap below is stated at this scale and multiplied up.
pub const DEFAULT_DPI: u32 = 96;

/// Gap between the mouse position and the badge at 100% (ADR-0005). A product number, not a
/// derived one: checked by eye in the smoke checklist, pinned here by tests.
pub const CURSOR_GAP_96: i32 = 12;

/// Gap between the caret and the badge at 100%. Smaller than the cursor gap because a caret
/// is a thin line, while a mouse pointer is a ~32 px arrow that would sit under the badge.
pub const CARET_GAP_96: i32 = 6;

/// Inset from the work-area corner for [`ResolvedAnchor::Fixed`] at 100%.
pub const FIXED_MARGIN_96: i32 = 16;

/// A monitor's work area in physical virtual-screen pixels — `MONITORINFO.rcWork`, never
/// `rcMonitor`: the latter includes the taskbar, and a badge behind the taskbar is exactly
/// the invisibility this product exists to cure.
///
/// `right` and `bottom` are exclusive, as a Win32 `RECT` is. A monitor to the left of or
/// above the primary one has negative coordinates, which is why nothing in this module
/// treats zero as a lower bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Everything the OS knows that placement needs, for the one monitor the badge will land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorFacts {
    pub work: WorkArea,
    /// Effective DPI of that monitor (96 = 100%). Zero is tolerated and read as
    /// [`DEFAULT_DPI`]: it can only come from an OS call that misbehaved, and collapsing
    /// every gap to zero pixels is a worse answer than assuming 100%.
    pub dpi: u32,
}

/// The badge's size in physical pixels.
///
/// `i32` because every Win32 coordinate is `i32`; the lossy step from the image's `u32` is
/// done once, in [`BadgeSize::from_image`], where the `u32` actually comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeSize {
    pub width: i32,
    pub height: i32,
}

impl BadgeSize {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    /// Saturating conversion: a nonsense image size must not panic inside the overlay
    /// thread. An absurd width is pinned by [`clamp`] anyway.
    pub fn from_image(image: &BadgeImage) -> Self {
        Self {
            width: i32::try_from(image.width).unwrap_or(i32::MAX),
            height: i32::try_from(image.height).unwrap_or(i32::MAX),
        }
    }
}

/// A zero DPI is read as 100%; see [`MonitorFacts::dpi`].
const fn effective_dpi(dpi: u32) -> u32 {
    if dpi == 0 { DEFAULT_DPI } else { dpi }
}

/// Scales a logical (96-dpi) length to physical pixels: `logical * dpi / 96`.
///
/// Truncating, not rounding, and a test pins that down: at 110 dpi a 12 px gap is 13
/// physical px, not 14. Which of the two is right hardly matters; drifting between builds
/// does, because a gap that silently changes by a pixel reads as a rendering bug.
///
/// Saturating, and not out of tidiness: `dpi` arrives from an OS call, and a wrapped
/// multiplication would turn a gap into a negative offset — a badge in the opposite corner
/// of the desktop rather than a badge 12 px away.
pub const fn scaled(logical_96: i32, dpi: u32) -> i32 {
    let product = logical_96 as i64 * effective_dpi(dpi) as i64 / DEFAULT_DPI as i64;
    if product > i32::MAX as i64 {
        i32::MAX
    } else if product < i32::MIN as i64 {
        i32::MIN
    } else {
        product as i32
    }
}

/// Where the badge's top-left corner goes: anchor offset, scaled by DPI, clamped into the
/// work area. The single entry point the overlay uses — callers never have to remember to
/// clamp afterwards.
///
/// The [`ResolvedAnchor::Caret`] branch is unreachable in M1 (the caret locator is a null
/// implementation), and is deliberately implemented as "same as the cursor, with the smaller
/// gap". Whether a caret badge belongs above the line rather than below it is a product
/// question for M2, to be settled together with `Effect::SetCaretTracking` — not guessed at
/// here.
pub fn place(anchor: ResolvedAnchor, size: BadgeSize, facts: MonitorFacts) -> Point {
    let top_left = match anchor {
        ResolvedAnchor::Cursor(p) => offset_by(p, scaled(CURSOR_GAP_96, facts.dpi)),
        ResolvedAnchor::Caret(p) => offset_by(p, scaled(CARET_GAP_96, facts.dpi)),
        ResolvedAnchor::Fixed => {
            let margin = scaled(FIXED_MARGIN_96, facts.dpi);
            Point {
                x: facts
                    .work
                    .right
                    .saturating_sub(margin)
                    .saturating_sub(size.width),
                y: facts
                    .work
                    .bottom
                    .saturating_sub(margin)
                    .saturating_sub(size.height),
            }
        }
    };
    clamp(top_left, size, facts.work)
}

/// Down and to the right of the anchor point, by the same gap on both axes.
fn offset_by(anchor: Point, gap: i32) -> Point {
    Point {
        x: anchor.x.saturating_add(gap),
        y: anchor.y.saturating_add(gap),
    }
}

/// Pushes the badge back inside `work` if it hangs off an edge.
///
/// `min` first, then `max`, and the order is the entire behaviour for a badge bigger than
/// the work area: `min` shoves it past `left`, and `max` then pins it exactly to `left`, so
/// an oversized badge overflows off the right/bottom edge — where its first characters stay
/// readable — instead of off the left/top, where the text would begin off-screen.
pub fn clamp(top_left: Point, size: BadgeSize, work: WorkArea) -> Point {
    Point {
        x: top_left
            .x
            .min(work.right.saturating_sub(size.width))
            .max(work.left),
        y: top_left
            .y
            .min(work.bottom.saturating_sub(size.height))
            .max(work.top),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920x1080 monitor with a 40 px taskbar along the bottom.
    const WORK: WorkArea = WorkArea {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    const fn facts(dpi: u32) -> MonitorFacts {
        MonitorFacts { work: WORK, dpi }
    }

    struct Case {
        name: &'static str,
        anchor: ResolvedAnchor,
        size: BadgeSize,
        facts: MonitorFacts,
        expected: Point,
    }

    #[test]
    fn placement_table() {
        let cases = [
            Case {
                name: "cursor at 100%: the gap is 12 physical px",
                anchor: ResolvedAnchor::Cursor(Point { x: 100, y: 100 }),
                size: BadgeSize::new(40, 24),
                facts: facts(96),
                expected: Point { x: 112, y: 112 },
            },
            Case {
                name: "cursor at 200%: the same logical gap is 24 physical px",
                anchor: ResolvedAnchor::Cursor(Point { x: 100, y: 100 }),
                size: BadgeSize::new(40, 24),
                facts: facts(192),
                expected: Point { x: 124, y: 124 },
            },
            Case {
                name: "cursor near the bottom-right corner is pulled back inside rcWork",
                anchor: ResolvedAnchor::Cursor(Point { x: 1900, y: 1030 }),
                size: BadgeSize::new(40, 24),
                facts: facts(96),
                expected: Point { x: 1880, y: 1016 },
            },
            Case {
                name: "caret uses the smaller gap",
                anchor: ResolvedAnchor::Caret(Point { x: 100, y: 100 }),
                size: BadgeSize::new(40, 24),
                facts: facts(96),
                expected: Point { x: 106, y: 106 },
            },
            Case {
                name: "fixed at 100%: bottom-right of rcWork, inset by 16",
                anchor: ResolvedAnchor::Fixed,
                size: BadgeSize::new(40, 24),
                facts: facts(96),
                expected: Point { x: 1864, y: 1000 },
            },
            Case {
                name: "fixed at 200%: both the inset and the badge doubled",
                anchor: ResolvedAnchor::Fixed,
                size: BadgeSize::new(80, 48),
                facts: facts(192),
                expected: Point { x: 1808, y: 960 },
            },
        ];

        for case in cases {
            assert_eq!(
                place(case.anchor, case.size, case.facts),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn scaled_truncates_fractional_scales() {
        assert_eq!(scaled(12, 96), 12, "100% must be the identity");
        assert_eq!(
            scaled(12, 110),
            13,
            "12 * 110 / 96 = 13.75 truncates to 13; rounding to 14 would be a silent \
             one-pixel change in every gap"
        );
        assert_eq!(scaled(12, 192), 24);
        assert_eq!(scaled(16, 144), 24, "150% of the fixed margin");
    }

    #[test]
    fn a_zero_dpi_is_read_as_one_hundred_percent() {
        assert_eq!(
            place(
                ResolvedAnchor::Cursor(Point { x: 100, y: 100 }),
                BadgeSize::new(40, 24),
                MonitorFacts { work: WORK, dpi: 0 },
            ),
            Point { x: 112, y: 112 },
            "a monitor that reported 0 dpi must place the badge as if at 100%, not with \
             every gap collapsed to zero"
        );
    }

    /// The work area of a monitor left of and above the primary one is negative on both
    /// axes; `max(work.left)` must not quietly become `max(0)`.
    #[test]
    fn clamping_respects_a_monitor_left_of_the_primary_one() {
        let work = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let placed = clamp(Point { x: -2000, y: -50 }, BadgeSize::new(40, 24), work);
        assert_eq!(placed, Point { x: -1920, y: 0 });
        assert!(placed.x >= work.left && placed.y >= work.top);
    }

    #[test]
    fn a_badge_wider_than_the_work_area_is_pinned_to_its_left_edge() {
        let placed = clamp(Point { x: 500, y: 500 }, BadgeSize::new(2000, 24), WORK);
        assert_eq!(
            placed.x, WORK.left,
            "min-then-max must pin an oversized badge to the left edge, letting it overflow \
             to the right where its first characters remain readable"
        );
        assert_eq!(
            placed.y, 500,
            "the vertical axis fits and must not be moved"
        );
    }

    #[test]
    fn a_badge_taller_than_the_work_area_is_pinned_to_its_top_edge() {
        let placed = clamp(Point { x: 500, y: 500 }, BadgeSize::new(40, 2000), WORK);
        assert_eq!(placed.y, WORK.top);
        assert_eq!(placed.x, 500);
    }

    /// `place` must never hand back a position that still needs clamping: callers rely on it
    /// being the single entry point.
    #[test]
    fn place_never_returns_an_unclamped_position() {
        for dpi in [96, 120, 144, 192] {
            for anchor in [
                ResolvedAnchor::Fixed,
                ResolvedAnchor::Cursor(Point { x: 1919, y: 1039 }),
                ResolvedAnchor::Cursor(Point { x: -5000, y: -5000 }),
                ResolvedAnchor::Caret(Point { x: 1919, y: 1039 }),
            ] {
                let size = BadgeSize::new(44, 26);
                let p = place(anchor, size, facts(dpi));
                assert!(
                    p.x >= WORK.left && p.x + size.width <= WORK.right,
                    "x out of rcWork for {anchor:?} at dpi {dpi}: {p:?}"
                );
                assert!(
                    p.y >= WORK.top && p.y + size.height <= WORK.bottom,
                    "y out of rcWork for {anchor:?} at dpi {dpi}: {p:?}"
                );
            }
        }
    }

    /// Saturation, not a panic: the size arrives from an image whose dimensions the overlay
    /// did not choose, and the DPI from an OS call.
    #[test]
    fn extreme_inputs_saturate_instead_of_overflowing() {
        let huge = BadgeSize::new(i32::MAX, i32::MAX);
        let _ = clamp(Point { x: 0, y: 0 }, huge, WORK);

        let far = WorkArea {
            left: i32::MIN,
            top: i32::MIN,
            right: i32::MAX,
            bottom: i32::MAX,
        };
        let _ = clamp(
            Point {
                x: i32::MAX,
                y: i32::MIN,
            },
            huge,
            far,
        );

        assert_eq!(scaled(i32::MAX, 192), i32::MAX, "must saturate, not wrap");
        assert_eq!(
            place(
                ResolvedAnchor::Cursor(Point {
                    x: i32::MAX,
                    y: i32::MAX
                }),
                BadgeSize::new(40, 24),
                MonitorFacts {
                    work: far,
                    dpi: 192
                },
            ),
            Point {
                x: i32::MAX.saturating_sub(40),
                y: i32::MAX.saturating_sub(24)
            },
            "a cursor at the far edge of the coordinate space must land at that edge, not \
             wrap into the opposite corner"
        );
    }

    #[test]
    fn badge_size_from_image_saturates_absurd_dimensions() {
        let image = BadgeImage {
            width: u32::MAX,
            height: 26,
            bgra_premul: Vec::new(),
            dpi: 96,
        };
        let size = BadgeSize::from_image(&image);
        assert_eq!(size.width, i32::MAX);
        assert_eq!(size.height, 26);
    }
}
