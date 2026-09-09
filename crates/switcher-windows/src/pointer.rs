//! Event-driven pointer tracking, armed only while a cursor-anchored badge is visible.

use crate::supervise::{callback_panic_outcome, guard_callback};
use crate::win_util::{
    HiddenWindow, PumpHandler, PumpThread, PumpVerdict, PumpWaker, WM_PUMP_WAKE, current_thread_id,
    spawn_pump,
};
use crossbeam_channel::Sender;
use std::cell::Cell;
use switcher_platform::events::{
    Capability, CapabilityReport, CapabilityState, PlatformEvent, Point,
};
use switcher_platform::ports::{PlatformError, PointerTracker};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetCursorPos, MSG, PM_REMOVE, PeekMessageW, WM_APP, WM_INPUT, WM_QUIT,
};

const WM_POINTER_ACTIVE: u32 = WM_APP + 0x210;
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

fn raw_input_request(hwnd: HWND, arm: bool) -> RAWINPUTDEVICE {
    RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: 2,
        dwFlags: if arm { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if arm { hwnd } else { HWND::default() },
    }
}

#[derive(Default)]
struct MotionState {
    active: bool,
    last: Option<Point>,
}

impl MotionState {
    fn observe(&mut self, point: Point) -> Option<Point> {
        if !self.active || self.last == Some(point) {
            return None;
        }
        self.last = Some(point);
        Some(point)
    }
}

/// The application owns one pointer tracker: Raw Input registration is process-wide.
#[derive(Debug)]
pub struct Pointer {
    pump: PumpThread,
    events: Sender<PlatformEvent>,
}

impl Pointer {
    pub fn new(events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        let thread_events = events.clone();
        let pump = spawn_pump("pointer", move || {
            ACTIVE.set(false);
            PENDING.set(false);
            let window = HiddenWindow::new("LangSwitcherPointer", Some(pointer_wndproc), None)?;
            Ok(PointerThread {
                window,
                events: thread_events,
                registered: false,
                motion: MotionState::default(),
            })
        })?;
        Ok(Self { pump, events })
    }
}

impl PointerTracker for Pointer {
    fn set_active(&self, active: bool) {
        if let Err(error) =
            self.pump
                .post(WM_POINTER_ACTIVE, WPARAM(usize::from(active)), LPARAM(0))
        {
            report(&self.events, CapabilityState::Off, error.code, error.detail);
        }
    }

    fn cursor_pos(&self) -> Option<Point> {
        cursor_position()
    }
}

fn cursor_position() -> Option<Point> {
    let mut pos = POINT::default();
    // SAFETY: the output points to a live POINT. No HWND/thread affinity is required;
    // access to the input desktop may fail and is represented as None.
    unsafe { GetCursorPos(&mut pos) }.ok()?;
    Some(Point { x: pos.x, y: pos.y })
}

struct PointerThread {
    window: HiddenWindow,
    events: Sender<PlatformEvent>,
    registered: bool,
    motion: MotionState,
}

impl PointerThread {
    fn set_active(&mut self, active: bool) {
        if self.registered == active && self.motion.active == active {
            return;
        }
        // Suppress queued input immediately on a disarm, even if removing the OS
        // registration fails. `registered` remains true so shutdown can retry removal.
        if !active {
            ACTIVE.set(false);
            self.motion.active = false;
        }
        let request = raw_input_request(self.window.hwnd(), active);
        // SAFETY: one aligned, live RAWINPUTDEVICE; cbSize matches it exactly. The
        // target is our live window when arming and NULL for RIDEV_REMOVE.
        let result =
            unsafe { RegisterRawInputDevices(&[request], size_of::<RAWINPUTDEVICE>() as u32) };
        match result {
            Ok(()) => {
                self.registered = active;
                self.motion.active = active;
                self.motion.last = None;
                ACTIVE.set(active);
                PENDING.set(false);
                let code = if active {
                    "pointer_armed"
                } else {
                    "pointer_disarmed"
                };
                tracing::debug!(active, "raw input registration changed");
                report(&self.events, CapabilityState::Ok, code, code);
            }
            Err(error) => report(
                &self.events,
                CapabilityState::Degraded,
                "pointer_registration_failed",
                error.message(),
            ),
        }
    }

    fn flush_motion(&mut self) -> PumpVerdict {
        PENDING.set(false);
        // Bound the drain: a continuously busy device must not starve stop/disarm.
        // Remaining input triggers the next wake. Each consumed message still goes
        // through DefWindowProc, including foreground RIM_INPUT cleanup.
        for _ in 0..256 {
            let mut message = MSG::default();
            // SAFETY: live MSG; filter only our live window's WM_INPUT messages.
            if !unsafe {
                PeekMessageW(
                    &mut message,
                    Some(self.window.hwnd()),
                    WM_INPUT,
                    WM_INPUT,
                    PM_REMOVE,
                )
            }
            .as_bool()
            {
                break;
            }
            // PeekMessage returns WM_QUIT regardless of the requested range. A
            // reentrant callback may have posted it; do not swallow its shutdown.
            if message.message == WM_QUIT {
                return PumpVerdict::Quit;
            }
            // SAFETY: forward the retrieved WM_INPUT unchanged to its default handler.
            unsafe {
                DefWindowProcW(
                    message.hwnd,
                    message.message,
                    message.wParam,
                    message.lParam,
                )
            };
        }
        if let Some(pos) = cursor_position().and_then(|pos| self.motion.observe(pos)) {
            tracing::trace!(target: "switcher_windows::pointer", x = pos.x, y = pos.y, "PointerMoved");
            let _ = self.events.send(PlatformEvent::PointerMoved { pos });
        }
        PumpVerdict::Continue
    }
}

impl PumpHandler for PointerThread {
    fn on_thread_message(&mut self, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> PumpVerdict {
        match msg {
            WM_POINTER_ACTIVE => self.set_active(wparam.0 != 0),
            WM_PUMP_WAKE if PENDING.get() => return self.flush_motion(),
            _ => {}
        }
        PumpVerdict::Continue
    }
}

impl Drop for PointerThread {
    fn drop(&mut self) {
        self.set_active(false);
        ACTIVE.set(false);
        PENDING.set(false);
        if let Err(error) = callback_panic_outcome() {
            report(&self.events, CapabilityState::Off, error.code, error.detail);
        }
    }
}

fn report(
    events: &Sender<PlatformEvent>,
    state: CapabilityState,
    code: &'static str,
    detail: impl Into<String>,
) {
    let detail = detail.into();
    if state != CapabilityState::Ok {
        tracing::warn!(code, %detail, "pointer capability changed");
    }
    let _ = events.send(PlatformEvent::CapabilityChanged(CapabilityReport {
        capability: Capability::Pointer,
        state,
        code,
        detail,
    }));
}

unsafe extern "system" fn pointer_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    guard_callback(
        Capability::Pointer,
        || {
            // SAFETY: unchanged parameters delivered by Windows to our procedure.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
        || {
            if msg == WM_INPUT
                && ACTIVE.get()
                && !PENDING.replace(true)
                && let Err(error) = PumpWaker::new(current_thread_id()).wake()
            {
                PENDING.set(false);
                tracing::warn!(code = error.code, "could not wake pointer pump");
            }
            // SAFETY: WM_INPUT requires default cleanup for foreground raw input. Always
            // forwarding also covers messages ignored while disarmed; no RAWINPUT is read.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use windows::Win32::UI::Input::GetRegisteredRawInputDevices;

    #[test]
    fn input_drain_preserves_callback_shutdown_and_emits_no_motion() {
        std::thread::spawn(|| {
            let (events, received) = crossbeam_channel::unbounded();
            let window =
                HiddenWindow::new("LangSwitcherPointerQuitTest", None, None).expect("test window");
            let mut pointer = PointerThread {
                window,
                events,
                registered: false,
                motion: MotionState {
                    active: true,
                    last: None,
                },
            };
            let mut message = MSG::default();
            // SAFETY: live MSG, this isolated test thread's own queue; discard
            // creation messages before PostQuitMessage synthesizes WM_QUIT.
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {}
            crate::win_util::post_quit();
            assert_eq!(pointer.flush_motion(), PumpVerdict::Quit);
            assert!(received.try_recv().is_err(), "shutdown sends no motion");
            pointer.motion.active = false;
        })
        .join()
        .expect("isolated native test exits");
    }

    fn mouse_registration() -> Option<RAWINPUTDEVICE> {
        // No other test in this process registers Raw Input. Leave room for other
        // device classes so this checks actual OS state, not just our bookkeeping.
        let mut devices = [RAWINPUTDEVICE::default(); 8];
        let mut count = devices.len() as u32;
        // SAFETY: aligned, writable buffer with the advertised capacity and size;
        // the count pointer lives across the call. UINT_MAX is checked before slicing.
        let written = unsafe {
            GetRegisteredRawInputDevices(
                Some(devices.as_mut_ptr()),
                &mut count,
                size_of::<RAWINPUTDEVICE>() as u32,
            )
        };
        assert_ne!(written, u32::MAX, "query registered devices");
        devices[..written as usize]
            .iter()
            .find(|device| device.usUsagePage == 1 && device.usUsage == 2)
            .copied()
    }

    fn wait_for_registration(events: &crossbeam_channel::Receiver<PlatformEvent>, code: &str) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let event = events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("pointer pump acknowledges registration");
            if let PlatformEvent::CapabilityChanged(report) = event {
                assert_eq!(report.state, CapabilityState::Ok, "{report:?}");
                if report.code == code {
                    return;
                }
            }
        }
    }

    #[test]
    fn native_registration_is_lazy_disarmed_and_released_on_drop() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let pointer = Pointer::new(tx).expect("start pointer pump");
        assert!(mouse_registration().is_none(), "no subscription at startup");
        pointer.set_active(true);
        wait_for_registration(&rx, "pointer_armed");
        let registered = mouse_registration().expect("mouse registered with Windows");
        assert_eq!(registered.dwFlags, RIDEV_INPUTSINK);
        assert!(!registered.hwndTarget.is_invalid());
        pointer.set_active(false);
        wait_for_registration(&rx, "pointer_disarmed");
        assert!(
            mouse_registration().is_none(),
            "Windows removed the registration"
        );
        pointer.set_active(true);
        wait_for_registration(&rx, "pointer_armed");
        drop(pointer);
        assert!(mouse_registration().is_none(), "Drop joins and unregisters");
    }

    #[test]
    fn disarm_request_has_no_target_and_arm_keeps_target() {
        let hwnd = HWND(0x1234usize as *mut _);
        let arm = raw_input_request(hwnd, true);
        assert_eq!((arm.usUsagePage, arm.usUsage), (1, 2));
        assert_eq!(arm.dwFlags, RIDEV_INPUTSINK);
        assert_eq!(arm.hwndTarget, hwnd);
        let disarm = raw_input_request(hwnd, false);
        assert_eq!(disarm.dwFlags, RIDEV_REMOVE);
        assert!(disarm.hwndTarget.is_invalid());
    }

    #[test]
    fn inactive_tracker_drops_motion_and_active_tracker_deduplicates_positions() {
        let mut state = MotionState::default();
        let p = Point { x: -500, y: 123 };
        assert_eq!(state.observe(p), None);
        state.active = true;
        assert_eq!(state.observe(p), Some(p));
        assert_eq!(state.observe(p), None);
        let moved = Point { x: -499, y: 123 };
        assert_eq!(state.observe(moved), Some(moved));
        state.active = false;
        assert_eq!(state.observe(p), None);
    }
}
