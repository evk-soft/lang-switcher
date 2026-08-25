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
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
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

    // Phase 1: parked. Everything that needs a *stationary* target is checked here, and this
    // phase exists because the first run by someone other than the author established that
    // the walk below is far too fast to click: 40 steps of 150 ms across 5120 px is roughly
    // 850 px/s, and the badge is 44 px wide. A check nobody can physically perform is not a
    // check.
    let parked = centre_of_the_primary_monitor();
    let mut anchor = ResolvedAnchor::Cursor(parked);
    let mut badge = synthetic_badge(overlay.dpi_for(anchor));
    overlay.show(&badge, anchor);

    tracing::info!(
        x = parked.x,
        y = parked.y,
        render_dpi = badge.dpi,
        width = badge.width,
        height = badge.height,
        "the badge is parked and will not move until you say so"
    );
    tracing::info!("check now, on the badge itself:");
    tracing::info!("  1. click straight through it — the click must land in the window behind");
    tracing::info!(
        "  2. focus must not move: the active title bar stays active, the caret keeps blinking"
    );
    tracing::info!("  3. the top-left corner block must be see-through, not filled");
    tracing::info!(
        "  4. the body must be BLUE with a RED stripe down its right third — swapped colours mean the BGRA byte order of ADR-0006 is wrong"
    );
    tracing::info!("  5. it must sit above a maximized, and above a full-screen, window");
    tracing::info!("  6. Task Manager: 0% CPU while it just sits there");
    tracing::info!(
        "  7. change the display scale in Settings — the badge must re-place itself and then change size"
    );
    tracing::info!("press Enter when you are done, to start the walk across the desktop");
    wait_for_enter();

    // A scale change during the parked phase leaves a pending re-render request.
    if let Some(dpi) = drain(&events_rx) {
        badge = synthetic_badge(dpi);
        tracing::info!(
            dpi,
            width = badge.width,
            height = badge.height,
            "re-rendered"
        );
        overlay.show(&badge, anchor);
    }

    // Phase 2: the walk, which is about monitor crossing and nothing else.
    let path = diagonal_across_the_virtual_desktop();
    tracing::info!(
        steps = path.len(),
        first = ?path.first(),
        last = ?path.last(),
        "walking the badge across the whole virtual desktop"
    );

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

    // Phase 3: the `Fixed` anchor. The policy under test is "the bottom-right of the *active
    // window's* monitor, never the primary one" (ADR-0005), and the two answers only differ
    // when the active window is somewhere other than the primary monitor.
    //
    // Hence the odd-looking instruction to move the terminal rather than to click some other
    // window: pressing Enter necessarily focuses the terminal, so any step that ended with
    // "click a window over there, then press Enter" would hand the foreground back to the
    // console and test nothing at all.
    let (primary_w, primary_h) = primary_monitor_size();
    tracing::info!(
        primary_width = primary_w,
        primary_height = primary_h,
        "the primary monitor always starts at (0,0); anything at a negative x or y is not it"
    );
    tracing::info!(
        "drag THIS TERMINAL onto a monitor that is not the primary one, then press Enter"
    );
    wait_for_enter();

    let chosen_dpi = overlay.dpi_for(ResolvedAnchor::Fixed);
    anchor = ResolvedAnchor::Fixed;
    overlay.show(&badge, anchor);
    tracing::info!(
        chosen_monitor_dpi = chosen_dpi,
        "the badge must now be in the bottom-right of THIS terminal's monitor. If that \
         monitor is scaled differently from the primary one, the dpi above is the giveaway: \
         it is the scale of the monitor the adapter picked"
    );
    tracing::info!("press Enter to hide the badge");
    wait_for_enter();

    if let Some(dpi) = drain(&events_rx) {
        badge = synthetic_badge(dpi);
        overlay.show(&badge, anchor);
        tracing::info!(
            dpi,
            "re-rendered for the Fixed anchor's monitor; press Enter"
        );
        wait_for_enter();
    }

    overlay.hide();
    tracing::info!(
        "badge hidden; the process is now idle — check Task Manager for 0% CPU, then press \
         Enter to exit"
    );
    wait_for_enter();

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

/// A blue badge with a red stripe, one transparent corner, sized for `dpi`.
///
/// Bytes are `[B, G, R, A]` — BGRA, not RGBA (ADR-0006). At `a = 255` premultiplication is
/// the identity, which is why the body is opaque: it keeps this example about the overlay
/// rather than about the rasterizer that task 16 will write.
///
/// **The two colours are chosen so that a channel swap is visible.** The first version of
/// this example used a green body, and green is the middle byte in both BGRA and RGBA — so it
/// looked identical either way and could not have caught a wrong byte order at all. Blue and
/// red sit at opposite ends, so if the body renders red and the stripe blue, the order is
/// reversed somewhere.
fn synthetic_badge(dpi: u32) -> BadgeImage {
    // The same `scaled` the overlay uses for its gaps, so the badge and the gap around it
    // grow together and a wrong scale is visible as a mismatch rather than a slight blur.
    let width = geometry::scaled(BADGE_W_96, dpi).max(1);
    let height = geometry::scaled(BADGE_H_96, dpi).max(1);
    let hole = geometry::scaled(HOLE_96, dpi).max(1);

    let stripe_from = width - width / 3;

    let mut bgra_premul = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            if x < hole && y < hole {
                // Fully transparent: with premultiplied alpha the colour channels must be
                // zero too, or the blend would tint the pixels behind the badge.
                bgra_premul.extend_from_slice(&[0, 0, 0, 0]);
            } else if x >= stripe_from {
                bgra_premul.extend_from_slice(&[0x00, 0x00, 0xFF, 0xFF]); // red
            } else {
                bgra_premul.extend_from_slice(&[0xFF, 0x00, 0x00, 0xFF]); // blue
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

/// Blocks until the reader presses Enter, or returns at once on a redirected or empty stdin
/// so scripted runs do not hang.
///
/// A blocking read rather than a sleep, and not only for the reader's sake: the process waits
/// on the OS with no timer and no wakeups of any kind, which is exactly the state the "0% CPU"
/// checks are about.
fn wait_for_enter() {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

/// Middle of the **primary** monitor, which is a comfortable place to click at.
///
/// Deliberately not the middle of the virtual desktop: on a two-monitor setup that lands on
/// the seam between them, which is the worst spot for every check in the parked phase.
fn centre_of_the_primary_monitor() -> Point {
    let (width, height) = primary_monitor_size();
    Point {
        x: width / 2,
        y: height / 2,
    }
}

/// Size of the primary monitor, whose top-left corner is always the virtual-screen origin.
fn primary_monitor_size() -> (i32, i32) {
    // SAFETY: `GetSystemMetrics` reads a system-wide value for the index given and has no
    // preconditions. These two indices describe the primary monitor, in physical pixels only
    // because `ensure_per_monitor_v2` already ran.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
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
