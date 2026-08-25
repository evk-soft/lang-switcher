//! Drives the layered overlay along a scripted path so the things no unit test can see get
//! seen: click-through, no focus theft, staying on top, and one `OverlayScaleChanged` per
//! monitor crossing on a mixed-DPI desktop.
//!
//! Run it and watch the screen, not just the log:
//!
//! ```text
//! cargo run -p switcher-windows --example overlay_smoke
//! ```
//!
//! The expectations to tick off are in `docs/smoke/m1-windows.md`, section C.

use std::time::Duration;

use switcher_platform::events::{BadgeImage, PlatformEvent, Point, ResolvedAnchor};
use switcher_platform::ports::OverlayWindow;
use switcher_windows::dpi;
use switcher_windows::overlay::{Overlay, geometry};
use tracing_subscriber::EnvFilter;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Badge size in *logical* (96-dpi) pixels; the real image is this scaled to the monitor.
///
/// Scaling it here is what makes the ADR-0005 convergence claim observable: the badge must
/// look the same size to the eye on a 100% and a 150% monitor, which can only happen if a
/// scale change is noticed, reported, re-rendered and shown again — in one round.
const BADGE_W_96: i32 = 44;
const BADGE_H_96: i32 = 26;

/// Size of the fully transparent corner block, also logical. Its whole job is to fail
/// visibly if `ULW_ALPHA` is not doing what we think: without per-pixel alpha this corner is
/// a solid square like the rest of the badge.
const HOLE_96: i32 = 8;

const STEPS: u32 = 40;
const STEP_PAUSE: Duration = Duration::from_millis(150);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .init();

    // First line, before any window exists in this process. Examples are not covered by the
    // linker-embedded manifest (ADR-0010), so without this the whole run would happen on
    // DPI-virtualized coordinates and would prove nothing about real placement.
    let awareness = dpi::ensure_per_monitor_v2();
    tracing::info!(
        from_manifest = awareness.from_manifest,
        per_monitor_v2 = awareness.per_monitor_v2,
        "dpi awareness at startup (from_manifest=false is expected for an example)"
    );

    let (events_tx, events_rx) = crossbeam_channel::unbounded();
    let overlay = Overlay::new(events_tx).expect("the overlay window must open");

    let path = diagonal_across_the_virtual_desktop();
    tracing::info!(
        steps = path.len(),
        first = ?path.first(),
        last = ?path.last(),
        "walking the badge across the whole virtual desktop"
    );

    let first = *path.first().expect("the path is never empty");
    let mut anchor = ResolvedAnchor::Cursor(first);
    let mut badge = synthetic_badge(overlay.dpi_for(anchor));
    tracing::info!(
        render_dpi = badge.dpi,
        width = badge.width,
        height = badge.height,
        "badge rasterized for the starting monitor"
    );

    overlay.show(&badge, anchor);

    for (step, point) in path.iter().enumerate() {
        anchor = ResolvedAnchor::Cursor(*point);
        overlay.move_to(anchor);
        tracing::debug!(step, x = point.x, y = point.y, "moved");
        std::thread::sleep(STEP_PAUSE);

        // This is the shell's half of the ADR-0005 loop, and the reason it is in the
        // example at all: without it the badge keeps the previous monitor's pixel size and
        // the "same apparent size on both screens" expectation cannot be checked by eye.
        if let Some(dpi) = drain(&events_rx) {
            badge = synthetic_badge(dpi);
            tracing::info!(
                dpi,
                width = badge.width,
                height = badge.height,
                "re-rendered for the new scale; one more show must end the mismatch"
            );
            overlay.show(&badge, anchor);
        }
    }

    // `Fixed` should land in the bottom-right of the *active window's* monitor, not the
    // primary one (ADR-0005) — click another screen's window before this line to check it.
    tracing::info!("now placing the badge with the Fixed anchor");
    overlay.show(&badge, ResolvedAnchor::Fixed);
    std::thread::sleep(Duration::from_secs(3));
    if let Some(dpi) = drain(&events_rx) {
        badge = synthetic_badge(dpi);
        overlay.show(&badge, ResolvedAnchor::Fixed);
        std::thread::sleep(Duration::from_secs(2));
        drain(&events_rx);
    }

    overlay.hide();
    tracing::info!(
        "badge hidden; the process is now idle — check Task Manager for 0% CPU, then press \
         Enter to exit"
    );

    // A blocking read rather than a sleep, and not only for the reader's sake: the process
    // now waits on the OS with no timer and no wakeups of any kind, which is exactly the
    // state the 0% CPU check is about. A fixed countdown also made the check a race against
    // the clock — the first two runs on the author's machine were both cut short with Ctrl+C
    // because the wait was blind. With a redirected or empty stdin this reads EOF and exits
    // at once, so scripted runs do not hang.
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);

    drain(&events_rx);
    tracing::info!("done");
}

/// Prints whatever the overlay reported and returns the scale it last asked for.
///
/// The only event this example can produce is `OverlayScaleChanged`, and exactly one per
/// monitor crossing is the expectation. More than one for the same crossing means the
/// adapter is repeating itself.
fn drain(events: &crossbeam_channel::Receiver<PlatformEvent>) -> Option<u32> {
    let mut wanted = None;
    while let Ok(event) = events.try_recv() {
        match event {
            PlatformEvent::OverlayScaleChanged { dpi } => {
                tracing::info!(dpi, "OverlayScaleChanged — re-rendering at this scale");
                wanted = Some(dpi);
            }
            other => tracing::warn!(?other, "unexpected event from the overlay"),
        }
    }
    wanted
}

/// A solid, fully opaque badge with one transparent corner, sized for `dpi`.
///
/// Bytes are `[B, G, R, A]` — BGRA, not RGBA (ADR-0006). At `a = 255` premultiplication is
/// the identity, which is why the body is opaque: it keeps this example about the overlay
/// rather than about the rasterizer that task 16 will write.
fn synthetic_badge(dpi: u32) -> BadgeImage {
    // The same `scaled` the overlay uses for its gaps, so the badge and the gap around it
    // grow together and a wrong scale is visible as a mismatch rather than a slight blur.
    let width = geometry::scaled(BADGE_W_96, dpi).max(1);
    let height = geometry::scaled(BADGE_H_96, dpi).max(1);
    let hole = geometry::scaled(HOLE_96, dpi).max(1);

    let mut bgra_premul = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            if x < hole && y < hole {
                // Fully transparent: with premultiplied alpha the colour channels must be
                // zero too, or the blend would tint the pixels behind the badge.
                bgra_premul.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                bgra_premul.extend_from_slice(&[0x20, 0xC8, 0x40, 0xFF]);
            }
        }
    }

    BadgeImage {
        width: width as u32,
        height: height as u32,
        bgra_premul,
        dpi,
    }
}

/// Points along the diagonal of the whole virtual desktop, so a multi-monitor setup is
/// actually crossed rather than assumed.
fn diagonal_across_the_virtual_desktop() -> Vec<Point> {
    // SAFETY: `GetSystemMetrics` reads a system-wide value for the index given and has no
    // preconditions. The four indices below describe the bounding box of every monitor;
    // they are physical pixels only because `ensure_per_monitor_v2` already ran.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    tracing::info!(left, top, width, height, "virtual desktop bounds");

    // `SM_CXVIRTUALSCREEN` is a width, so the last pixel is at `left + width - 1`. Walking
    // to `left + width` would put the final steps outside the desktop, where clamping pins
    // them all to the same corner and the badge visibly sticks at the end of the run —
    // making the "does it follow smoothly" observation harder for no reason.
    let span_x = width.saturating_sub(1).max(0);
    let span_y = height.saturating_sub(1).max(0);

    let last = i32::try_from(STEPS.saturating_sub(1)).unwrap_or(1).max(1);
    (0..i32::try_from(STEPS).unwrap_or(2))
        .map(|step| Point {
            x: left + span_x * step / last,
            y: top + span_y * step / last,
        })
        .collect()
}
