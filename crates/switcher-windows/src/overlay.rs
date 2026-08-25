//! The badge window: layered, click-through, top-most, and owned by its own thread.
//!
//! Three responsibilities, split so only the middle one needs `unsafe`:
//!
//! 1. [`geometry`] — pure arithmetic, no OS calls, exhaustively table-tested.
//! 2. this module's private half — asking Windows which monitor a point is on, what its work
//!    area is, and what its effective DPI is, then blitting a premultiplied BGRA image into
//!    a layered window.
//! 3. [`Overlay`] — the `Send` handle the shell holds. It contains **no `HWND`**: every
//!    request becomes a command on a channel plus a nudge to the overlay thread. That is
//!    what makes the type honestly `Send` rather than `Send` by assertion, and it is the
//!    project rule that an OS window is only ever touched by the thread that created it.
//!
//! The one exception is [`OverlayWindow::dpi_for`], which answers synchronously on the
//! caller's thread. It has to: the shell asks for the DPI *before* rasterizing, so a hop
//! into the overlay thread would deadlock against the overlay's own message handling. That
//! is sound because none of the four calls it makes take our window's `HWND` — verified by
//! `dpi_for_answers_from_a_foreign_thread` below, since the docs do not say it in words.

pub mod geometry;

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::c_void;

use crossbeam_channel::{Receiver, Sender};
use switcher_platform::events::{
    BadgeImage, Capability, CapabilityReport, CapabilityState, PlatformEvent, ResolvedAnchor,
};
use switcher_platform::ports::{OverlayWindow, PlatformError};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetMonitorInfoW,
    HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MonitorFromPoint, MonitorFromWindow, SelectObject,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, ShowWindow,
    ULW_ALPHA, UpdateLayeredWindow, WM_DISPLAYCHANGE, WM_DPICHANGED, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::PCWSTR;

use crate::supervise::{callback_panic_outcome, guard_callback};
use crate::win_util::{
    PumpHandler, PumpThread, PumpVerdict, PumpWaker, WM_PUMP_WAKE, current_thread_id,
    register_class, spawn_pump, wide,
};
use geometry::{BadgeSize, DEFAULT_DPI, MonitorFacts, WorkArea, place};

/// Must be unique per window procedure in this process — see [`register_class`].
const CLASS_NAME: &str = "LangSwitcherOverlayWindow";

thread_local! {
    /// Set by [`overlay_wndproc`] when Windows reports a scale or display-topology change,
    /// read and cleared by the overlay thread before it trusts its DPI cache again.
    ///
    /// A thread-local rather than state reached through `create_param` and
    /// `GWLP_USERDATA`, and the first smoke run turned that from a preference into a
    /// requirement: `WM_DPICHANGED` arrives **re-entrantly, inside our own `SetWindowPos`
    /// call**, so the window procedure runs while [`OverlayThread::move_to`] is holding
    /// `&mut self`. A wndproc reaching the same state through a raw pointer would therefore
    /// alias a live mutable borrow — undefined behaviour on the ordinary path, not an edge
    /// case. A `Cell<bool>` touched by one thread has no such problem, and window messages
    /// are always dispatched on the thread that owns the window, which is the thread that
    /// owns the cache.
    static MONITOR_FACTS_STALE: Cell<bool> = const { Cell::new(false) };
}

/// Cache of effective DPI per monitor, keyed by the `HMONITOR` value.
///
/// Lives on the overlay thread only. `HMONITOR` is a pointer-sized handle that is stable
/// while the display configuration is, which is exactly why the cache is dropped whenever
/// Windows says that configuration changed.
type DpiCache = HashMap<isize, u32>;

/// What the shell asked for, on its way to the overlay thread.
#[derive(Debug)]
enum OverlayCommand {
    Show {
        image: BadgeImage,
        anchor: ResolvedAnchor,
    },
    MoveTo {
        anchor: ResolvedAnchor,
    },
    Hide,
}

/// The `Send` handle the shell holds. Deliberately holds no `HWND`.
#[derive(Debug)]
pub struct Overlay {
    commands: Sender<OverlayCommand>,
    pump: PumpThread,
}

impl Overlay {
    /// Raises the overlay thread and creates the badge window on it.
    ///
    /// `events` carries `OverlayScaleChanged` (a DPI mismatch was noticed while placing the
    /// badge) and, if a callback on the overlay thread panics, one final
    /// `CapabilityChanged(Overlay, Off)`.
    ///
    /// The overlay is **not** supervised, unlike the hook threads (ADR-0007): its state is
    /// the visible badge, and restarting the thread would leave that state to be rebuilt by
    /// replaying effects nobody kept.
    pub fn new(events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        let (commands, requests) = crossbeam_channel::unbounded();

        let pump = spawn_pump("overlay", move || {
            let window = OverlayHwnd::new()?;
            Ok(OverlayThread {
                // Declared before `window` on purpose: fields drop in declaration order,
                // and the GDI objects must be released before the window they draw into.
                surface: None,
                window,
                requests,
                events,
                dpi_cache: DpiCache::new(),
                visible: false,
                shown_dpi: DEFAULT_DPI,
                reported_mismatch: None,
                last_anchor: None,
            })
        })?;

        Ok(Self { commands, pump })
    }

    /// Queues a command and wakes the overlay thread so it drains the queue.
    fn request(&self, command: OverlayCommand) {
        // A dead overlay thread is worth one line in the log, not a panic: the badge is a
        // convenience, and the app has to keep switching layouts without it.
        if self.commands.send(command).is_err() {
            tracing::warn!(
                target: "switcher_windows::overlay",
                "the overlay thread is gone; dropping the request"
            );
            return;
        }
        if let Err(e) = self.pump.post(WM_PUMP_WAKE, WPARAM(0), LPARAM(0)) {
            tracing::warn!(
                target: "switcher_windows::overlay",
                code = e.code, detail = %e.detail,
                "could not wake the overlay thread"
            );
        }
    }
}

impl OverlayWindow for Overlay {
    fn show(&self, image: &BadgeImage, anchor: ResolvedAnchor) {
        // One clone per `show`, not per frame: the shell caches rasterized badges by
        // (content, dpi) (ADR-0006), and `move_to` carries no pixels at all.
        self.request(OverlayCommand::Show {
            image: image.clone(),
            anchor,
        });
    }

    fn move_to(&self, anchor: ResolvedAnchor) {
        self.request(OverlayCommand::MoveTo { anchor });
    }

    fn hide(&self) {
        self.request(OverlayCommand::Hide);
    }

    fn dpi_for(&self, anchor: ResolvedAnchor) -> u32 {
        // A throwaway cache: a fresh map always misses, so this is the uncached path
        // spelled without a second code path to keep in step with the cached one.
        monitor_facts(anchor, &mut DpiCache::new()).dpi
    }
}

/// The badge window, owned by the thread that created it.
///
/// Not `Send`, and that is load-bearing: `DestroyWindow` must run on the creating thread.
#[derive(Debug)]
struct OverlayHwnd(HWND);

impl OverlayHwnd {
    fn new() -> Result<Self, PlatformError> {
        // Enforced where the invariant actually lives, not left to whoever wrote `main`.
        // Every coordinate this module computes is a physical pixel only while the process
        // is Per-Monitor-V2 (ADR-0005), and `GetDpiForMonitor` reports 96 for every display
        // to a DPI-unaware caller — so a badge window created before awareness is settled is
        // a badge placed by DPI-virtualized arithmetic. The call is memoized, and if some
        // other window already beat us to it, it says so loudly instead of silently.
        let awareness = crate::dpi::ensure_per_monitor_v2();
        if !awareness.per_monitor_v2 {
            tracing::warn!(
                target: "switcher_windows::overlay",
                "creating the badge window without Per-Monitor-V2 awareness; placement will \
                 be computed from virtualized coordinates"
            );
        }

        let class = wide(CLASS_NAME);
        let module = register_class(CLASS_NAME, Some(overlay_wndproc))?;

        // SAFETY: the class name buffer (`class`) is alive across the call and
        // NUL-terminated by `wide`, and the class was just registered against `module`.
        // The parent is `None` on purpose: `HWND_MESSAGE` would make this a message-only
        // window, which never renders — the opposite of what a badge is for. Size is zero
        // here because `UpdateLayeredWindow` sets position, size and content together.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module.into()),
                None,
            )
        }
        .map_err(|e| PlatformError::new("overlay_create_failed", e.message()))?;

        Ok(Self(hwnd))
    }
}

impl Drop for OverlayHwnd {
    fn drop(&mut self) {
        // SAFETY: runs on the creating thread, which `OverlayHwnd: !Send` guarantees, and
        // the handle came from a successful `CreateWindowExW`. `Drop` runs once, so the
        // window is destroyed once.
        if let Err(e) = unsafe { DestroyWindow(self.0) } {
            tracing::warn!(
                target: "switcher_windows::overlay",
                error = %e,
                "DestroyWindow failed for the overlay window"
            );
        }
    }
}

/// A memory DC with a top-down 32bpp DIB section selected into it: the bitmap
/// `UpdateLayeredWindow` blits from.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    /// Whatever was selected into `dc` before our bitmap. Restoring it is not politeness:
    /// `DeleteObject` is documented to fail while the object is selected into a DC, so
    /// deleting the bitmap first would leak the whole DIB section.
    previous: HGDIOBJ,
    /// The DIB section's pixels, owned by `bitmap` and valid until it is deleted.
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Surface {
    fn new(width: i32, height: i32) -> Result<Self, PlatformError> {
        // SAFETY: `None` asks for a memory DC compatible with the screen, which is what a
        // layered window blits from. Returns a null handle on failure rather than erroring.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err(PlatformError::new(
                "overlay_dc_failed",
                "CreateCompatibleDC returned a null DC",
            ));
        }

        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height means top-down rows — the order `BadgeImage` uses. With a
                // positive height the badge would be blitted upside down.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                // `biCompression` is a plain `u32` while `BI_RGB` is a newtype, hence `.0`.
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: `info` is a fully initialized header describing a 32bpp uncompressed DIB,
        // alive across the call; `bits` is a live pointer the call writes the section's
        // address into. No file mapping is used, so the section handle is `None` and the
        // offset is 0.
        let bitmap =
            unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) };

        let bitmap = match bitmap {
            Ok(bitmap) if !bits.is_null() => bitmap,
            outcome => {
                let detail = match &outcome {
                    Err(e) => format!("CreateDIBSection failed: {e}"),
                    Ok(_) => "CreateDIBSection succeeded but returned no pixel pointer".to_owned(),
                };
                if let Ok(unusable) = outcome {
                    // A bitmap without its pixel pointer is useless but still a GDI object:
                    // returning here without deleting it would leak one per failure.
                    //
                    // SAFETY: it was just created and has not been selected into any DC.
                    let _ = unsafe { DeleteObject(unusable.into()) };
                }
                // SAFETY: `dc` came from a successful `CreateCompatibleDC` and nothing of
                // ours is selected into it, so deleting it here is the complete cleanup.
                let _ = unsafe { DeleteDC(dc) };
                return Err(PlatformError::new("overlay_dib_failed", detail));
            }
        };

        // SAFETY: both handles are valid and freshly created; selecting a bitmap into a
        // memory DC is the documented way to make it the DC's drawing surface.
        let previous = unsafe { SelectObject(dc, bitmap.into()) };
        if previous.is_invalid() {
            // A fresh memory DC always has a 1x1 monochrome default bitmap selected, so a
            // null return means the call failed rather than "there was nothing selected".
            // Failing here beats returning a DC that cannot be blitted from, which would
            // instead show up as an `UpdateLayeredWindow` error on every single show.
            //
            // SAFETY: the bitmap was created but never became this DC's surface, so it is
            // selected nowhere and deleting it is complete.
            let _ = unsafe { DeleteObject(bitmap.into()) };
            // SAFETY: nothing of ours is selected into `dc`, so this releases the last of
            // the two resources this function had acquired.
            let _ = unsafe { DeleteDC(dc) };
            return Err(PlatformError::new(
                "overlay_select_failed",
                "SelectObject refused the DIB section",
            ));
        }

        Ok(Self {
            dc,
            bitmap,
            previous,
            bits: bits.cast::<u8>(),
            width,
            height,
        })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // The order below is the whole point of this `Drop`, and it is not cosmetic:
        // restore the previous object, then delete the bitmap, then the DC. Deleting a
        // bitmap that is still selected into a DC fails, and a failed delete leaks
        // `width * height * 4` bytes plus a GDI object on every resize.
        if !self.previous.is_invalid() {
            // SAFETY: `dc` is still alive here, and `previous` is the handle this DC held
            // before we selected our bitmap into it.
            let _ = unsafe { SelectObject(self.dc, self.previous) };
        }
        // SAFETY: `bitmap` came from `CreateDIBSection` and is no longer selected into any
        // DC. `Drop` runs once, so it is deleted once.
        if !unsafe { DeleteObject(self.bitmap.into()) }.as_bool() {
            tracing::warn!(
                target: "switcher_windows::overlay",
                "DeleteObject(bitmap) failed; the DIB section has leaked"
            );
        }
        // SAFETY: `dc` came from `CreateCompatibleDC` and nothing of ours is selected into
        // it any more.
        if !unsafe { DeleteDC(self.dc) }.as_bool() {
            tracing::warn!(
                target: "switcher_windows::overlay",
                "DeleteDC failed; the memory DC has leaked"
            );
        }
    }
}

/// State that lives on the overlay thread. Everything here is touched by that thread only.
struct OverlayThread {
    surface: Option<Surface>,
    window: OverlayHwnd,
    requests: Receiver<OverlayCommand>,
    events: Sender<PlatformEvent>,
    dpi_cache: DpiCache,
    visible: bool,
    /// DPI of the image currently on screen, so `move_to` can notice a monitor crossing
    /// without being handed the image again.
    shown_dpi: u32,
    /// The `(image dpi, monitor dpi)` pair the last `OverlayScaleChanged` was about.
    ///
    /// Without it the event repeats on **every** move while the mismatch lasts, which the
    /// first smoke run showed plainly: one crossing produced thirteen events, one per step
    /// of the pointer path, because the mismatch persists until the shell re-renders. The
    /// shell is already required to ignore a scale it has just rendered (ADR-0005), so the
    /// repeats could only ever be discarded — this keeps them off the channel instead.
    reported_mismatch: Option<(u32, u32)>,
    /// Where the visible badge was last placed, so a display change can re-place it without
    /// the shell being involved. See [`OverlayThread::replace_after_display_change`].
    last_anchor: Option<ResolvedAnchor>,
}

impl PumpHandler for OverlayThread {
    fn on_thread_message(&mut self, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> PumpVerdict {
        if msg != WM_PUMP_WAKE {
            return PumpVerdict::Continue;
        }
        // Drained in order, without collapsing a run of `MoveTo` into its last element:
        // redundant moves are suppressed at the source, where the pointer events arrive
        // (task 11), which avoids sending them at all rather than discarding them here.
        while let Ok(request) = self.requests.try_recv() {
            match request {
                OverlayCommand::Show { image, anchor } => self.show(&image, anchor),
                OverlayCommand::MoveTo { anchor } => self.move_to(anchor),
                OverlayCommand::Hide => self.hide(),
            }
        }
        // Handled here rather than in the window procedure, because this is the only place
        // that may touch `self`: the procedure runs re-entrantly from inside our own
        // `SetWindowPos` (see `MONITOR_FACTS_STALE`), so it can only post a wake.
        if MONITOR_FACTS_STALE.get() {
            self.replace_after_display_change();
        }
        PumpVerdict::Continue
    }
}

impl OverlayThread {
    fn show(&mut self, image: &BadgeImage, anchor: ResolvedAnchor) {
        let Some(bytes) = dib_bytes(image) else {
            tracing::error!(
                target: "switcher_windows::overlay",
                width = image.width, height = image.height, len = image.bgra_premul.len(),
                "badge image is not self-consistent; refusing to blit it"
            );
            return;
        };

        let size = BadgeSize::from_image(image);
        let facts = self.facts(anchor);
        let position = place(anchor, size, facts);

        if let Err(e) = self.ensure_surface(size) {
            tracing::error!(
                target: "switcher_windows::overlay",
                code = e.code, detail = %e.detail,
                "could not prepare the badge surface"
            );
            // Otherwise a GDI failure during a resize leaves the previous badge frozen on
            // screen: it is still visible, but `move_to` now finds no surface and refuses to
            // move it. A badge stuck at a stale position is worse than no badge.
            self.hide();
            return;
        }
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        if surface.width != size.width || surface.height != size.height {
            // Unreachable through `ensure_surface`, and checked anyway: this equality is the
            // precondition of the copy below, and a precondition that is asserted where it
            // is used cannot be invalidated by a future edit somewhere else.
            tracing::error!(
                target: "switcher_windows::overlay",
                surface_w = surface.width, surface_h = surface.height,
                image_w = size.width, image_h = size.height,
                "surface does not match the image; refusing to blit it"
            );
            return;
        }

        // SAFETY: `bytes` is exactly `width * height * 4` (checked by `dib_bytes`, which also
        // rejects dimensions that do not fit `i32`), and the surface was just verified to
        // have that same width and height — so the destination holds exactly that many
        // bytes: a 32bpp DIB has no row padding, its stride being `width * 4`, already a
        // multiple of four. Source and destination are distinct allocations — one a `Vec`,
        // one a GDI DIB section — so they cannot overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(image.bgra_premul.as_ptr(), surface.bits, bytes);
        }

        let destination = POINT {
            x: position.x,
            y: position.y,
        };
        let extent = SIZE {
            cx: size.width,
            cy: size.height,
        };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        // SAFETY: every pointer argument refers to a live local that outlives the call. The
        // destination DC is `None`, which the API documents as "use the default palette" —
        // so no screen DC has to be acquired and released here. `hdcSrc` is our memory DC
        // with the DIB selected into it, and `ULW_ALPHA` with `AC_SRC_ALPHA` is what makes
        // Windows read the per-pixel alpha of premultiplied BGRA (ADR-0006).
        let updated = unsafe {
            UpdateLayeredWindow(
                self.window.0,
                None,
                Some(&destination),
                Some(&extent),
                Some(surface.dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        if let Err(e) = updated {
            tracing::error!(
                target: "switcher_windows::overlay",
                error = %e, ?position, width = size.width, height = size.height,
                "UpdateLayeredWindow failed; the badge was not shown"
            );
            return;
        }

        if !self.visible {
            // SAFETY: shows the window without activating it, so focus stays where the user
            // put it. The `BOOL` result reports whether the window was already visible, not
            // success, so there is nothing to check.
            let _ = unsafe { ShowWindow(self.window.0, SW_SHOWNOACTIVATE) };
            self.visible = true;
        }

        self.shown_dpi = image.dpi;
        self.last_anchor = Some(anchor);
        self.report_scale_mismatch(image.dpi, facts.dpi);
    }

    fn move_to(&mut self, anchor: ResolvedAnchor) {
        let Some(size) = self
            .surface
            .as_ref()
            .map(|s| BadgeSize::new(s.width, s.height))
        else {
            tracing::debug!(
                target: "switcher_windows::overlay",
                "move_to before the first show; nothing to move"
            );
            return;
        };

        let facts = self.facts(anchor);
        let position = place(anchor, size, facts);

        // SAFETY: moves our own window without touching size, z-order or activation.
        // `SWP_ASYNCWINDOWPOS` is deliberately absent (ADR-0005): it would let another
        // thread's request be applied out of order with respect to show/hide.
        let moved = unsafe {
            SetWindowPos(
                self.window.0,
                None,
                position.x,
                position.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        if let Err(e) = moved {
            tracing::warn!(
                target: "switcher_windows::overlay",
                error = %e, ?position,
                "SetWindowPos failed; the badge stayed where it was"
            );
            return;
        }

        self.last_anchor = Some(anchor);
        self.report_scale_mismatch(self.shown_dpi, facts.dpi);
    }

    /// Re-places the standing badge after Windows reported a scale or topology change.
    ///
    /// Without this the two messages we listen for would be inert. The staleness flag is
    /// consumed only by [`OverlayThread::facts`], which runs only when the shell sends a
    /// command — and a badge can legitimately stand still indefinitely: with
    /// `AnchorPref::Fixed` the core turns pointer tracking **off** (`tracking` is true only
    /// for a `Cursor` anchor), and `BadgeMode::Follow` cancels the hide timer. So the user
    /// changes the scale in Settings, `rcWork` moves, and nothing asks us to do anything.
    ///
    /// That is precisely the case `WM_DPICHANGED` is kept for (ADR-0005 and
    /// `docs/architecture/overview.md`): the comparison trigger cannot cover it, because
    /// nothing invokes the comparison. Re-placing here fixes the position from the new
    /// `rcWork` and lets `report_scale_mismatch` ask the shell for a correctly sized image.
    ///
    /// `lParam`'s suggested rectangle is deliberately ignored: it is meant for windows the
    /// system lays out, and this one positions itself from `rcWork` and the anchor.
    fn replace_after_display_change(&mut self) {
        if !self.visible {
            // Nothing on screen to correct. The flag is left standing on purpose, so the
            // next `facts` still drops the DPI cache it was set for.
            return;
        }
        let Some(anchor) = self.last_anchor else {
            return;
        };
        tracing::debug!(
            target: "switcher_windows::overlay",
            ?anchor,
            "re-placing the standing badge after a display change"
        );
        // Consumes the flag, drops the cache, re-places, and reports any scale mismatch.
        self.move_to(anchor);
    }

    fn hide(&mut self) {
        if !self.visible {
            return;
        }
        // SAFETY: hides our own window; the `BOOL` reports the previous visibility.
        let _ = unsafe { ShowWindow(self.window.0, SW_HIDE) };
        self.visible = false;
        // Hiding ends the current mismatch episode. Without this, a badge shown again later
        // at the same pair of scales would be silently denied its `OverlayScaleChanged`,
        // because the deduplication above would still be remembering the old episode.
        self.reported_mismatch = None;
    }

    /// Facts about the monitor `anchor` lands on, honouring the staleness flag the wndproc
    /// sets. Every OS call here happens while handling a message that already arrived, so
    /// nothing about this is polling (ADR-0003).
    fn facts(&mut self, anchor: ResolvedAnchor) -> MonitorFacts {
        if MONITOR_FACTS_STALE.replace(false) {
            tracing::debug!(
                target: "switcher_windows::overlay",
                entries = self.dpi_cache.len(),
                "display scale or topology changed; dropping the monitor DPI cache"
            );
            self.dpi_cache.clear();
        }
        monitor_facts(anchor, &mut self.dpi_cache)
    }

    fn ensure_surface(&mut self, size: BadgeSize) -> Result<(), PlatformError> {
        if self
            .surface
            .as_ref()
            .is_some_and(|s| s.width == size.width && s.height == size.height)
        {
            return Ok(());
        }
        // Dropped before the replacement is built, so a resize never holds two DIB sections
        // at once.
        self.surface = None;
        self.surface = Some(Surface::new(size.width, size.height)?);
        Ok(())
    }

    /// The sole trigger for `OverlayScaleChanged` (ADR-0005): the image was rendered for one
    /// scale and the monitor it landed on wants another. Not derived from `WM_DPICHANGED`,
    /// whose delivery to a `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` window we move ourselves is
    /// not documented.
    fn report_scale_mismatch(&mut self, image_dpi: u32, monitor_dpi: u32) {
        if image_dpi == monitor_dpi {
            // Back in agreement: forget what was reported, so a later crossing back to this
            // scale is announced again.
            self.reported_mismatch = None;
            return;
        }
        if self.reported_mismatch == Some((image_dpi, monitor_dpi)) {
            return;
        }
        self.reported_mismatch = Some((image_dpi, monitor_dpi));
        tracing::debug!(
            target: "switcher_windows::overlay",
            image_dpi, monitor_dpi,
            "badge is on a monitor with a different scale; asking for a re-render"
        );
        let _ = self
            .events
            .send(PlatformEvent::OverlayScaleChanged { dpi: monitor_dpi });
    }
}

impl Drop for OverlayThread {
    fn drop(&mut self) {
        // Runs on the overlay thread, so the thread-local panic record is this thread's.
        // The overlay is not supervised, so a panic caught at the FFI boundary is the end
        // of the badge for this run — the shell has to be told, or the tray would keep
        // claiming the overlay works.
        if let Err(e) = callback_panic_outcome() {
            tracing::error!(
                target: "switcher_windows::overlay",
                detail = %e.detail,
                "the overlay thread is ending after a panic at a callback boundary"
            );
            let _ = self
                .events
                .send(PlatformEvent::CapabilityChanged(CapabilityReport {
                    capability: Capability::Overlay,
                    state: CapabilityState::Off,
                    code: "overlay_callback_panicked",
                    detail: e.detail,
                }));
        }
    }
}

/// Our window procedure. Notes that the monitor facts went stale and wakes the pump so the
/// thread can act on it — it does not act itself, and cannot: it runs re-entrantly from
/// inside our own `SetWindowPos`, so any state it touched could be mutably borrowed at that
/// moment (see [`MONITOR_FACTS_STALE`]). Posting a message is the one thing that is always
/// safe from here, because it only queues.
unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    guard_callback(
        Capability::Overlay,
        || {
            // SAFETY: forwarding the system's own arguments unchanged to the default
            // handler, which is what any message we do not handle must get.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
        || {
            if msg == WM_DPICHANGED || msg == WM_DISPLAYCHANGE {
                MONITOR_FACTS_STALE.set(true);
                tracing::debug!(
                    target: "switcher_windows::overlay",
                    msg,
                    "the overlay window was told the display configuration changed"
                );
                // Without this wake nothing would ever read the flag unless the shell
                // happened to send a command, and a badge anchored to `Fixed` in follow
                // mode stands still forever.
                if let Err(e) = PumpWaker::new(current_thread_id()).wake() {
                    tracing::warn!(
                        target: "switcher_windows::overlay",
                        code = e.code, detail = %e.detail,
                        "could not wake the overlay thread after a display change"
                    );
                }
            }
            // SAFETY: as above — the default handler gets every message, including the two
            // we only took note of.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
    )
}

/// Exactly how many bytes a 32bpp top-down DIB of this image's size needs, or `None` if the
/// image does not describe itself consistently.
///
/// This is the precondition of the `copy_nonoverlapping` in [`OverlayThread::show`], which
/// is the one place in this module where getting a number wrong would be undefined
/// behaviour rather than a misplaced badge. Hence a named, tested function instead of an
/// inline comparison.
fn dib_bytes(image: &BadgeImage) -> Option<usize> {
    let needed = dib_extent(image.width, image.height)?;
    (image.bgra_premul.len() == needed).then_some(needed)
}

/// Byte count for a 32bpp DIB of `width` x `height`, or `None` if that pair cannot be a
/// badge.
///
/// Split out from [`dib_bytes`] so its rules can be tested at the boundary without having to
/// allocate a buffer of the matching size — which for the interesting case would be gigabytes.
///
/// The dimensions must fit `i32`, not merely `usize`, and that is the load-bearing part:
/// every Win32 coordinate is `i32`, and [`BadgeSize::from_image`] *saturates* instead of
/// failing. An image wider than `i32::MAX` would therefore be measured at its true width here
/// but blitted into a surface built for `i32::MAX` — a byte count larger than the
/// destination. GDI would almost certainly refuse a surface that big first, but "the OS
/// rejects it for us" is luck, not a bound.
fn dib_extent(width: u32, height: u32) -> Option<usize> {
    let width = i32::try_from(width).ok()?;
    let height = i32::try_from(height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)
}

/// Which monitor an anchor lands on.
///
/// `Fixed` follows the active window rather than the primary monitor: on a laptop with an
/// external display the primary is almost never the screen the user is typing on (ADR-0005).
fn monitor_for(anchor: ResolvedAnchor) -> Option<HMONITOR> {
    let monitor = match anchor {
        ResolvedAnchor::Cursor(p) | ResolvedAnchor::Caret(p) => {
            // SAFETY: takes the point by value and returns a handle to the nearest monitor.
            // `MONITOR_DEFAULTTONEAREST` is what makes a point outside every monitor — a
            // stale cursor position after a display was unplugged — still resolvable.
            unsafe { MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONEAREST) }
        }
        ResolvedAnchor::Fixed => {
            // SAFETY: reads the foreground window of the session; takes no arguments and
            // may legitimately answer with a null handle (no active window, or one owned by
            // a process we may not query).
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.is_invalid() {
                // Handled explicitly rather than relying on `MonitorFromWindow` doing
                // something sensible with a null `HWND`: the documentation describes the
                // flag's behaviour for a window that is off-screen, not for one that does
                // not exist.
                //
                // SAFETY: the origin of the virtual screen with
                // `MONITOR_DEFAULTTOPRIMARY`, which is documented to answer with the
                // primary monitor when the point is on no monitor at all.
                unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
            } else {
                // SAFETY: `foreground` is a window handle we just read; the call only maps
                // it to a monitor and does not require us to own the window.
                unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTOPRIMARY) }
            }
        }
    };
    (!monitor.is_invalid()).then_some(monitor)
}

fn work_area(monitor: HMONITOR) -> Option<WorkArea> {
    let mut info = MONITORINFO {
        // Set by hand despite the `Default` derive: the call uses `cbSize` to decide which
        // version of the struct it was handed, and a zero would fail it.
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // SAFETY: `info` is a live, fully initialized `MONITORINFO` with a correct `cbSize`,
    // and `monitor` came from a call that just returned a non-null handle. The function
    // returns `BOOL`, not a `Result`, so failure has to be checked by hand.
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info) };
    if !ok.as_bool() {
        return None;
    }

    // `rcWork`, never `rcMonitor`: the latter includes the taskbar (ADR-0005).
    Some(WorkArea {
        left: info.rcWork.left,
        top: info.rcWork.top,
        right: info.rcWork.right,
        bottom: info.rcWork.bottom,
    })
}

/// Effective DPI of one monitor, or [`DEFAULT_DPI`] if the OS will not say.
///
/// **Only truthful while the process is Per-Monitor-V2.** Learn is explicit that this API
/// answers according to the caller's DPI awareness — a DPI-unaware process is told 96 for
/// every display — which is the same precondition ADR-0005 already states for treating
/// coordinates as physical pixels, and the reason `dpi::ensure_per_monitor_v2` runs before
/// the badge window is created.
///
/// Learn also suggests `GetDpiForWindow` as "the DPI-aware version of this API". That one
/// cannot answer the question asked here: the shell needs the scale of the monitor the badge
/// is *about to* be placed on, and `GetDpiForWindow` only reports the monitor a window
/// already sits on.
fn monitor_dpi(monitor: HMONITOR) -> u32 {
    let mut dpi_x = 0u32;
    let mut dpi_y = 0u32;

    // SAFETY: both out-parameters are live locals, and `monitor` is a non-null handle from
    // a call that just produced it. `MDT_EFFECTIVE_DPI` is the scale the user chose, which
    // is the number the badge must match.
    let queried = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };

    match queried {
        Ok(()) => {
            if dpi_x != dpi_y {
                // Learn states the two are identical for this API ("The values of *dpiX and
                // *dpiY are identical. You only need to record one"), so this branch should
                // be unreachable. Kept as a log line rather than deleted: it costs one
                // comparison and would be the only warning if that ever stopped holding.
                tracing::debug!(
                    target: "switcher_windows::overlay",
                    dpi_x, dpi_y,
                    "monitor reported different DPI per axis; using the horizontal one"
                );
            }
            if dpi_x == 0 {
                tracing::warn!(
                    target: "switcher_windows::overlay",
                    "monitor reported a zero DPI; assuming 100%"
                );
                return DEFAULT_DPI;
            }
            dpi_x
        }
        Err(e) => {
            tracing::warn!(
                target: "switcher_windows::overlay",
                error = %e,
                "GetDpiForMonitor failed; assuming 100%"
            );
            DEFAULT_DPI
        }
    }
}

/// Gathers everything [`place`] needs. Callable from any thread — none of the calls it makes
/// takes our window's `HWND`, which is what lets `dpi_for` answer without a thread hop.
fn monitor_facts(anchor: ResolvedAnchor, cache: &mut DpiCache) -> MonitorFacts {
    let Some(monitor) = monitor_for(anchor) else {
        tracing::warn!(
            target: "switcher_windows::overlay",
            ?anchor,
            "no monitor could be resolved for the anchor; placing the badge at the origin"
        );
        return MonitorFacts {
            work: WorkArea {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dpi: DEFAULT_DPI,
        };
    };

    let work = work_area(monitor).unwrap_or_else(|| {
        tracing::warn!(
            target: "switcher_windows::overlay",
            "GetMonitorInfoW failed; placing the badge at the origin"
        );
        WorkArea {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    });

    let dpi = *cache
        .entry(monitor.0 as isize)
        .or_insert_with(|| monitor_dpi(monitor));

    MonitorFacts { work, dpi }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::dpi;

    fn image(width: u32, height: u32, len: usize) -> BadgeImage {
        BadgeImage {
            width,
            height,
            bgra_premul: vec![0u8; len],
            dpi: 96,
        }
    }

    #[test]
    fn dib_extent_rejects_dimensions_that_are_not_win32_coordinates() {
        assert_eq!(dib_extent(44, 26), Some(44 * 26 * 4));
        assert_eq!(dib_extent(0, 26), None, "a zero dimension is not a badge");
        assert_eq!(dib_extent(44, 0), None);
        assert_eq!(
            dib_extent(i32::MAX as u32 + 1, 1),
            None,
            "a width past i32::MAX must be rejected here, because BadgeSize::from_image \
             would silently saturate it and the blit would then overrun the surface"
        );
        assert_eq!(dib_extent(u32::MAX, u32::MAX), None);

        // `i32::MAX * i32::MAX * 4` does not overflow `usize` on a 64-bit target, so
        // `dib_extent` answers with the real product rather than `None`. That is fine, and
        // the bound that closes the case sits one level up: no `Vec` can be that long, so
        // the length equality in `dib_bytes` can never hold.
        assert_eq!(
            dib_bytes(&image(i32::MAX as u32, i32::MAX as u32, 16)),
            None,
            "an image claiming a size no allocation can hold must be refused"
        );
    }

    #[test]
    fn dib_bytes_accepts_only_an_exact_buffer() {
        assert_eq!(dib_bytes(&image(44, 26, 44 * 26 * 4)), Some(44 * 26 * 4));
        assert_eq!(
            dib_bytes(&image(44, 26, 44 * 26 * 4 - 1)),
            None,
            "a buffer one byte short must be rejected: the blit reads width*height*4"
        );
        assert_eq!(dib_bytes(&image(44, 26, 44 * 26 * 4 + 1)), None);
        assert_eq!(
            dib_bytes(&image(0, 26, 0)),
            None,
            "a zero dimension is not a badge"
        );
        assert_eq!(dib_bytes(&image(44, 0, 0)), None);
        assert_eq!(
            dib_bytes(&image(u32::MAX, u32::MAX, 16)),
            None,
            "dimensions whose product overflows must not wrap into a small length"
        );
    }

    /// The port hands `Overlay` around as `Send`, and the shell shares it behind an `Arc`,
    /// so `Sync` is required too. Both are auto-derived — this test is what notices if a
    /// future field takes one of them away.
    #[test]
    fn overlay_is_send_and_sync() {
        fn require<T: Send + Sync>() {}
        require::<Overlay>();
    }

    /// The thread-affinity question from
    /// `docs/research/2026-08-25-m1-api-research/geometry.md`: the four calls behind
    /// `dpi_for` are documented without a word about which thread may make them, and the
    /// whole no-hop design rests on the answer. So it is exercised instead of assumed: two
    /// threads ask, neither hangs, and both get the same number.
    ///
    /// **What this proves and what it does not.** It proves `dpi_for` is callable from a
    /// foreign thread at all — no hang, no deadlock against the overlay's own message
    /// handling, no thread-dependent failure — and that both answers agree. It does **not**
    /// prove the number is a real per-monitor value: `GetDpiForMonitor` is awareness
    /// dependent (Learn: "you will receive different DPI values depending on the DPI
    /// awareness of the calling application"; a DPI-unaware process gets 96 for every
    /// display), and 96 is also this module's fallback, so on a DPI-unaware run — or simply
    /// on a single 100% monitor, as on CI — the equality holds trivially. The evidence that
    /// the values are real is the smoke run on a mixed-DPI desktop, where `dpi_for` reads
    /// 144 on one monitor and 96 on the other (`docs/smoke/m1-windows.md`, section C).
    #[test]
    fn dpi_for_answers_from_a_foreign_thread() {
        // Asked for explicitly rather than assumed: a test binary carries no manifest
        // (ADR-0010), and the geometry contract of this whole module presumes
        // Per-Monitor-V2. Memoized, so this is idempotent. It is deliberately not asserted
        // on — another test in this binary may have created a window first, after which
        // Windows refuses to change the awareness mode, and a test that fails on the order
        // libtest happened to pick would be flaky rather than informative.
        let awareness = dpi::ensure_per_monitor_v2();

        let (events, _events_rx) = crossbeam_channel::unbounded();
        let overlay = Arc::new(Overlay::new(events).expect("the overlay window must open"));

        let here = overlay.dpi_for(ResolvedAnchor::Fixed);

        let elsewhere = {
            let overlay = Arc::clone(&overlay);
            std::thread::spawn(move || overlay.dpi_for(ResolvedAnchor::Fixed))
                .join()
                .expect("the foreign thread must not panic or hang")
        };

        assert_eq!(
            here, elsewhere,
            "dpi_for must not depend on which thread asks (per_monitor_v2 was {})",
            awareness.per_monitor_v2
        );
        assert!(
            here >= DEFAULT_DPI,
            "a monitor cannot be scaled below 100%, and the fallback is exactly 100%"
        );
    }
}
