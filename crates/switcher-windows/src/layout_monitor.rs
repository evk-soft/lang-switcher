//! Shell/foreground notifications plus user-controlled 200ms fallback (ADR-0019).

pub mod classify;
mod control;
mod snapshot;
pub(crate) use snapshot::language_for;

pub use snapshot::current;

use crate::supervise::{StopToken, SupervisedThread, guard_callback, spawn_supervised};
use crate::win_util::{
    HiddenWindow, PumpHandler, PumpVerdict, PumpWaker, WM_PUMP_WAKE, current_thread_id,
    pump_with_handler,
};
use classify::{ForegroundFacts, PollDecision, decide};
use control::Control;
use crossbeam_channel::Sender;
use std::cell::Cell;
use std::sync::Arc;
use switcher_platform::events::{
    Capability, CapabilityReport, CapabilityState, LangTag, LayoutId, LayoutSource, PlatformEvent,
};
use switcher_platform::ports::{LayoutMonitor, PlatformError};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DeregisterShellHookWindow, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow,
    GetWindowThreadProcessId, HSHELL_LANGUAGE, KillTimer, RegisterShellHookWindow,
    RegisterWindowMessageW, SetTimer, WINEVENT_OUTOFCONTEXT, WM_TIMER,
};
use windows::core::w;

const FOREGROUND: u32 = 1;
const SHELL: u32 = 2;
const TIMER: u32 = 4;
const FALLBACK_TIMER: usize = 1;
const FALLBACK_INTERVAL_MS: u32 = 200;
thread_local! {
    static SHELL_MESSAGE: Cell<u32> = const { Cell::new(0) };
    static PENDING: Cell<u32> = const { Cell::new(0) };
}

/// Its synchronous reader remains usable even if subscriptions could not be started.
#[derive(Debug)]
pub struct LayoutHooks {
    _worker: Option<SupervisedThread>,
    control: Option<Arc<Control>>,
}

impl LayoutHooks {
    pub fn new(events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        Self::with_fallback(events, true)
    }

    pub fn with_fallback(
        events: Sender<PlatformEvent>,
        enabled: bool,
    ) -> Result<Self, PlatformError> {
        let control = Arc::new(Control::new(enabled));
        let thread_control = Arc::clone(&control);
        let worker = spawn_supervised(
            &[
                Capability::LayoutShellHook,
                Capability::LayoutForegroundHook,
            ],
            events,
            move |events, stop| run(events, stop, Arc::clone(&thread_control)),
        )?;
        Ok(Self {
            _worker: Some(worker),
            control: Some(control),
        })
    }

    pub fn reader_only() -> Self {
        Self {
            _worker: None,
            control: None,
        }
    }
}

impl LayoutMonitor for LayoutHooks {
    fn current(&self) -> Result<(LayoutId, LangTag), PlatformError> {
        current()
    }

    fn set_fallback_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        match &self.control {
            Some(control) => control.set_enabled(enabled),
            None if !enabled => Ok(()),
            None => Err(PlatformError::new(
                "layout_control_unavailable",
                "layout worker is unavailable",
            )),
        }
    }
}

fn notify(reason: u32) {
    notify_with(reason, || PumpWaker::new(current_thread_id()).wake());
}

fn notify_with(reason: u32, wake: impl FnOnce() -> Result<(), PlatformError>) {
    let before = PENDING.get();
    PENDING.set(before | reason);
    if before == 0
        && let Err(error) = wake()
    {
        PENDING.set(0);
        tracing::warn!(?error, "layout pump wake failed");
    }
}

unsafe extern "system" fn layout_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    guard_callback(
        Capability::LayoutShellHook,
        || {
            // SAFETY: unchanged system-provided parameters.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
        || {
            if msg == SHELL_MESSAGE.get() && wparam.0 == HSHELL_LANGUAGE as usize {
                notify(SHELL);
            }
            if msg == WM_TIMER && wparam.0 == FALLBACK_TIMER {
                notify(TIMER);
            }
            // SAFETY: no message payload is dereferenced or modified.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        },
    )
}

unsafe extern "system" fn foreground_proc(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    guard_callback(
        Capability::LayoutForegroundHook,
        || (),
        || notify(FOREGROUND),
    );
}

struct LayoutThread<'a> {
    window: HiddenWindow,
    hook: HWINEVENTHOOK,
    shell_registered: bool,
    shell_observed: bool,
    fallback: bool,
    ticks: u64,
    events: Sender<PlatformEvent>,
    stop: &'a StopToken,
    last_read_error: Option<&'static str>,
    failure: Option<PlatformError>,
    control: Arc<Control>,
    enabled: bool,
}

fn run(
    events: &Sender<PlatformEvent>,
    stop: &StopToken,
    control: Arc<Control>,
) -> Result<(), PlatformError> {
    PENDING.set(0);
    // SAFETY: static terminated string; the registered message ID has process lifetime.
    let message = unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) };
    SHELL_MESSAGE.set(message);
    let window = HiddenWindow::new("LangSwitcherLayout", Some(layout_proc), None)?;
    let mut state = LayoutThread {
        window,
        hook: HWINEVENTHOOK::default(),
        shell_registered: false,
        shell_observed: false,
        fallback: false,
        ticks: 0,
        events: events.clone(),
        stop,
        last_read_error: None,
        failure: None,
        enabled: control.enabled(),
        control,
    };
    state.control.attach(PumpWaker::new(current_thread_id()));
    // SAFETY: static out-of-context callback runs on this thread, which pumps inline.
    // Its subscription is removed by LayoutThread::drop on this same thread.
    state.hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    // SAFETY: the message-only window belongs to this thread and outlives registration.
    state.shell_registered =
        message != 0 && unsafe { RegisterShellHookWindow(state.window.hwnd()) }.as_bool();
    if state.shell_registered {
        report(
            events,
            Capability::LayoutShellHook,
            CapabilityState::Degraded,
            "shell_delivery_unverified",
            "registered; waiting for the first language notification",
        );
    } else {
        report(
            events,
            Capability::LayoutShellHook,
            CapabilityState::Off,
            "shell_hook_failed",
            "shell subscription failed",
        );
    }
    // Subscriptions are optional accelerators. Their failure must not disable the
    // selected fallback or its control channel, including when polling starts off.
    state.update_fallback();
    if let Some(error) = state.failure.take() {
        return Err(error);
    }
    state.report_foreground();
    pump_with_handler("layout", &mut state)?;
    match state.failure.take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

impl LayoutThread<'_> {
    fn publish(&mut self, source: LayoutSource) {
        self.publish_read(source, current());
    }

    fn publish_read(
        &mut self,
        source: LayoutSource,
        result: Result<(LayoutId, LangTag), PlatformError>,
    ) {
        match result {
            Ok((layout, lang)) => {
                self.last_read_error = None;
                tracing::trace!(target: "switcher_windows::layout", ?source, ?layout, lang = lang.as_str(), "layout notification");
                let _ = self.events.send(PlatformEvent::LayoutChanged {
                    layout,
                    lang,
                    source,
                });
            }
            Err(error) => {
                if self.last_read_error != Some(error.code) {
                    tracing::debug!(target: "switcher_windows::layout", ?error, "foreground snapshot unavailable; retaining previous state");
                    self.last_read_error = Some(error.code);
                }
            }
        }
    }

    fn update_fallback(&mut self) -> Option<(HWND, u32)> {
        self.enabled = self.control.enabled();
        let (hwnd, facts) = foreground_facts();
        self.reconcile_timer(decide(&facts, self.enabled, !self.hook.is_invalid()));
        // A discovery heartbeat with no foreground hook is not permission to read
        // an own or unknown thread. Revalidate this identity inside snapshot as well.
        (self.failure.is_none() && self.fallback && self.enabled && facts.is_foreign())
            .then_some((hwnd, facts.tid))
    }

    fn reconcile_timer(&mut self, decision: PollDecision) {
        match decision {
            PollDecision::Disarm if self.fallback => self.disarm(),
            PollDecision::Arm(reason) if !self.fallback => {
                // SAFETY: live owned window, nonzero window-local ID; no TIMERPROC.
                // Do not reset an existing timer on frequent foreground events.
                let armed = unsafe {
                    SetTimer(
                        Some(self.window.hwnd()),
                        FALLBACK_TIMER,
                        FALLBACK_INTERVAL_MS,
                        None,
                    )
                };
                if armed == 0 {
                    self.failure = Some(PlatformError::new(
                        "fallback_timer_failed",
                        windows::core::Error::from_thread().message(),
                    ));
                } else {
                    self.fallback = true;
                    self.ticks = 0;
                    tracing::info!(target: "switcher_windows::layout", reason,
                        interval_ms = FALLBACK_INTERVAL_MS, "foreground fallback armed");
                }
            }
            _ => {}
        }
    }

    fn report_foreground(&self) {
        let (state, code, detail) = if !self.hook.is_invalid() {
            (
                CapabilityState::Ok,
                "foreground_hook_ready",
                "foreground notifications registered",
            )
        } else if self.enabled && self.fallback {
            (
                CapabilityState::Degraded,
                "foreground_poll_only",
                "foreground notifications unavailable; using 200ms fallback",
            )
        } else {
            (
                CapabilityState::Off,
                "foreground_hook_failed",
                "foreground notifications unavailable; fallback disabled",
            )
        };
        report(
            &self.events,
            Capability::LayoutForegroundHook,
            state,
            code,
            detail,
        );
    }

    fn disarm(&mut self) {
        // SAFETY: exactly the window and ID supplied to our successful SetTimer.
        match unsafe { KillTimer(Some(self.window.hwnd()), FALLBACK_TIMER) } {
            Ok(()) => {
                self.fallback = false;
                tracing::info!(target: "switcher_windows::layout", ticks = self.ticks, "foreground fallback disarmed");
            }
            Err(error) => {
                self.fallback = false;
                self.failure = Some(PlatformError::new(
                    "fallback_disarm_failed",
                    error.message(),
                ));
                // Stop publishing immediately. The failed timer is owned by our
                // window, which teardown destroys before supervision retries.
            }
        }
    }
}

impl PumpHandler for LayoutThread<'_> {
    fn should_stop(&self) -> bool {
        self.stop.is_stopped() || self.failure.is_some()
    }

    fn on_thread_message(&mut self, message: u32, _: WPARAM, _: LPARAM) -> PumpVerdict {
        if self.should_stop() {
            return PumpVerdict::Quit;
        }
        if message != WM_PUMP_WAKE {
            return PumpVerdict::Continue;
        }
        let pending = PENDING.replace(0);
        let was_enabled = self.enabled;
        // A preference wake may have no PENDING bits. Apply it before queued timer
        // work, and stop publication immediately if creating/removing the timer fails.
        let target = self.update_fallback();
        if self.should_stop() {
            return PumpVerdict::Quit;
        }
        if was_enabled != self.enabled {
            tracing::info!(target: "switcher_windows::layout", enabled = self.enabled, "layout fallback preference applied");
            self.report_foreground();
        }
        if pending & FOREGROUND != 0 {
            self.publish(LayoutSource::ForegroundChange);
        }
        if pending & SHELL != 0 {
            if !self.shell_observed {
                self.shell_observed = true;
                report(
                    &self.events,
                    Capability::LayoutShellHook,
                    CapabilityState::Ok,
                    "shell_language_observed",
                    "language notification received",
                );
            }
            self.publish(LayoutSource::ShellHook);
        }
        if (pending & TIMER != 0 || (!was_enabled && self.enabled))
            && let Some((hwnd, tid)) = target
        {
            self.ticks += 1;
            self.publish_read(
                LayoutSource::ForegroundPoll,
                snapshot::current_if_foreground(hwnd.0 as usize, tid),
            );
        }
        PumpVerdict::Continue
    }
}

impl Drop for LayoutThread<'_> {
    fn drop(&mut self) {
        self.control.detach();
        if self.fallback {
            self.disarm();
        }
        // SAFETY: these resources were installed on this thread and still belong to
        // it. The hidden window is destroyed only after all subscriptions are removed.
        unsafe {
            if !self.hook.is_invalid() && !UnhookWinEvent(self.hook).as_bool() {
                tracing::warn!("UnhookWinEvent failed");
            }
            if self.shell_registered && !DeregisterShellHookWindow(self.window.hwnd()).as_bool() {
                tracing::warn!("DeregisterShellHookWindow failed");
            }
        }
        PENDING.set(0);
        SHELL_MESSAGE.set(0);
    }
}

fn report(
    events: &Sender<PlatformEvent>,
    capability: Capability,
    state: CapabilityState,
    code: &'static str,
    detail: impl Into<String>,
) {
    let _ = events.send(PlatformEvent::CapabilityChanged(CapabilityReport {
        capability,
        state,
        code,
        detail: detail.into(),
    }));
}

fn foreground_facts() -> (HWND, ForegroundFacts) {
    let mut facts = ForegroundFacts {
        tid: 0,
        is_own_process: false,
    };
    // SAFETY: read-only desktop lookup; a null foreground is not our input thread.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return (hwnd, facts);
    }
    let mut pid = 0;
    // SAFETY: borrowed HWND, live PID output. No foreign process handle is opened.
    unsafe {
        facts.tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        facts.is_own_process = pid == GetCurrentProcessId();
    }
    // SAFETY: recheck the same borrowed window/thread. The actual layout read
    // performs its own before/after identity checks; this is not an atomic snapshot.
    let raced = unsafe {
        GetForegroundWindow() != hwnd || GetWindowThreadProcessId(hwnd, None) != facts.tid
    };
    if pid == 0 || raced {
        facts.tid = 0;
    }
    (hwnd, facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch_timer(hwnd: HWND, wait_ms: u64) -> bool {
        use std::time::{Duration, Instant};
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, WM_QUIT,
        };
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        while Instant::now() < deadline {
            let mut message = MSG::default();
            // SAFETY: only this test thread's owned window and initialized MSG.
            // No external window is activated or sent an input message.
            if unsafe { PeekMessageW(&mut message, Some(hwnd), WM_TIMER, WM_TIMER, PM_REMOVE) }
                .as_bool()
            {
                assert_ne!(
                    message.message, WM_QUIT,
                    "unexpected quit during timer test"
                );
                // SAFETY: unchanged message from this thread's queue.
                unsafe {
                    DispatchMessageW(&message);
                }
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn native_fallback_toggle_removes_the_timer_and_rejects_a_queued_tick() {
        let (events, received) = crossbeam_channel::unbounded();
        let (asserted, done) = crossbeam_channel::bounded(1);
        let mut worker = spawn_supervised(
            &[Capability::LayoutForegroundHook],
            events,
            move |events, stop| {
                PENDING.set(0);
                let control = Arc::new(Control::new(false));
                let mut state = LayoutThread {
                    window: HiddenWindow::new(
                        "LangSwitcherFallbackToggleTest",
                        Some(layout_proc),
                        None,
                    )?,
                    hook: HWINEVENTHOOK::default(),
                    shell_registered: false,
                    shell_observed: false,
                    fallback: false,
                    ticks: 0,
                    events: events.clone(),
                    stop,
                    last_read_error: None,
                    failure: None,
                    control: Arc::clone(&control),
                    enabled: false,
                };
                control.attach(PumpWaker::new(current_thread_id()));
                state.update_fallback();
                assert!(
                    !state.fallback,
                    "saved false must not briefly arm at startup"
                );

                control.set_enabled(true)?;
                state.on_thread_message(WM_PUMP_WAKE, WPARAM(0), LPARAM(0));
                assert!(state.fallback, "fallback works without either subscription");
                assert!(dispatch_timer(state.window.hwnd(), 2000));
                assert_ne!(
                    PENDING.get() & TIMER,
                    0,
                    "native timer produced pending work"
                );
                let ticks_before_disable = state.ticks;

                control.set_enabled(false)?;
                state.on_thread_message(WM_PUMP_WAKE, WPARAM(0), LPARAM(0));
                assert!(!state.fallback);
                assert_eq!(
                    state.ticks, ticks_before_disable,
                    "disable wins over queued tick"
                );
                // KillTimer does not erase old queued messages; remove any residue first.
                assert!(state.failure.is_none());
                for _ in 0..8 {
                    if !dispatch_timer(state.window.hwnd(), 1) {
                        break;
                    }
                }
                assert!(
                    !dispatch_timer(state.window.hwnd(), 500),
                    "disabled timer must stop native wakeups"
                );

                control.set_enabled(true)?;
                state.on_thread_message(WM_PUMP_WAKE, WPARAM(0), LPARAM(0));
                assert!(state.fallback);
                assert!(
                    dispatch_timer(state.window.hwnd(), 2000),
                    "reenable creates a working timer"
                );
                asserted.send(()).expect("finish toggle assertions");
                Ok(())
            },
        )
        .expect("test worker");
        done.recv_timeout(std::time::Duration::from_secs(6))
            .expect("native toggle finished");
        worker.shutdown();
        assert!(!received.try_iter().any(|event| matches!(event,
            PlatformEvent::CapabilityChanged(report) if report.code == "source_restart_pending")));
    }

    #[test]
    fn a_failed_wake_does_not_block_future_notifications() {
        PENDING.set(0);
        notify_with(FOREGROUND, || {
            Err(PlatformError::new("queue_full", "injected"))
        });
        let retried = Cell::new(false);
        notify_with(SHELL, || {
            retried.set(true);
            Ok(())
        });
        assert!(retried.get(), "next native event retries the wake");
        assert_eq!(PENDING.replace(0), SHELL);
    }

    #[test]
    fn failed_timer_removal_stops_publication_and_requests_teardown() {
        let (events, received) = crossbeam_channel::unbounded();
        let (asserted, done) = crossbeam_channel::bounded(1);
        let mut worker = spawn_supervised(
            &[Capability::LayoutForegroundHook],
            events,
            move |events, stop| {
                let mut state = LayoutThread {
                    window: HiddenWindow::new("LangSwitcherTimerRemovalTest", None, None)?,
                    hook: HWINEVENTHOOK::default(),
                    shell_registered: false,
                    shell_observed: false,
                    fallback: true,
                    ticks: 0,
                    events: events.clone(),
                    stop,
                    last_read_error: None,
                    failure: None,
                    control: Arc::new(Control::new(true)),
                    enabled: true,
                };
                // No timer exists: make KillTimer fail while our bookkeeping says
                // armed. A failed removal must not leave the poll path enabled.
                state.disarm();
                assert!(!state.fallback);
                assert!(state.should_stop());
                assert_eq!(
                    state.failure.as_ref().unwrap().code,
                    "fallback_disarm_failed"
                );
                PENDING.set(TIMER);
                state.on_thread_message(WM_PUMP_WAKE, WPARAM(0), LPARAM(0));
                asserted.send(()).expect("finish assertions");
                Ok(())
            },
        )
        .expect("test worker");
        done.recv_timeout(std::time::Duration::from_secs(2))
            .expect("native test finished");
        worker.shutdown();
        assert!(
            received.try_recv().is_err(),
            "no polled event after failed removal"
        );
    }
}
