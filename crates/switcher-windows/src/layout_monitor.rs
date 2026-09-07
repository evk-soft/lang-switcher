//! Shell and foreground notifications with a conditional 500ms fallback (ADR-0007).

pub mod classify;
mod snapshot;
pub(crate) use snapshot::language_for;

pub use snapshot::current;

use crate::supervise::{StopToken, SupervisedThread, guard_callback, spawn_supervised};
use crate::win_util::{
    HiddenWindow, PumpHandler, PumpVerdict, PumpWaker, WM_PUMP_WAKE, current_thread_id,
    pump_with_handler,
};
use classify::{ForegroundFacts, OpenOutcome, PackageOutcome, PollDecision, decide};
use crossbeam_channel::Sender;
use std::cell::Cell;
use switcher_platform::events::{
    Capability, CapabilityReport, CapabilityState, LangTag, LayoutId, LayoutSource, PlatformEvent,
};
use switcher_platform::ports::{LayoutMonitor, PlatformError};
use windows::Win32::Foundation::{
    APPMODEL_ERROR_NO_PACKAGE, ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, HWND, LPARAM,
    LRESULT, WIN32_ERROR, WPARAM,
};
use windows::Win32::Storage::Packaging::Appx::GetPackageFullName;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_INFORMATION,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DeregisterShellHookWindow, EVENT_SYSTEM_FOREGROUND, GetClassNameW,
    GetForegroundWindow, GetWindowThreadProcessId, HSHELL_LANGUAGE, KillTimer,
    RegisterShellHookWindow, RegisterWindowMessageW, SetTimer, WINEVENT_OUTOFCONTEXT, WM_TIMER,
};
use windows::core::{Owned, w};

const FOREGROUND: u32 = 1;
const SHELL: u32 = 2;
const TIMER: u32 = 4;
const FALLBACK_TIMER: usize = 1;
thread_local! {
    static SHELL_MESSAGE: Cell<u32> = const { Cell::new(0) };
    static PENDING: Cell<u32> = const { Cell::new(0) };
}

/// Its synchronous reader remains usable even if subscriptions could not be started.
#[derive(Debug)]
pub struct LayoutHooks {
    _worker: Option<SupervisedThread>,
}

impl LayoutHooks {
    pub fn new(events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        let worker = spawn_supervised(
            &[
                Capability::LayoutShellHook,
                Capability::LayoutForegroundHook,
            ],
            events,
            run,
        )?;
        Ok(Self {
            _worker: Some(worker),
        })
    }

    pub fn reader_only() -> Self {
        Self { _worker: None }
    }
}

impl LayoutMonitor for LayoutHooks {
    fn current(&self) -> Result<(LayoutId, LangTag), PlatformError> {
        current()
    }
}

fn notify(reason: u32) {
    notify_with(reason, || PumpWaker::new(current_thread_id()).wake());
}

fn notify_with(reason: u32, wake: impl FnOnce() -> Result<(), PlatformError>) {
    let before = PENDING.get();
    PENDING.set(before | reason);
    if before == 0 {
        if let Err(error) = wake() {
            PENDING.set(0);
            tracing::warn!(?error, "layout pump wake failed");
        }
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
    fallback_degraded: bool,
}

fn run(events: &Sender<PlatformEvent>, stop: &StopToken) -> Result<(), PlatformError> {
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
        fallback_degraded: false,
    };
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
    let unavailable = if state.hook.is_invalid() && !state.shell_registered {
        CapabilityState::Degraded
    } else {
        CapabilityState::Off
    };
    if state.hook.is_invalid() {
        report(
            events,
            Capability::LayoutForegroundHook,
            unavailable,
            "foreground_hook_failed",
            "SetWinEventHook failed",
        );
    } else {
        report(
            events,
            Capability::LayoutForegroundHook,
            CapabilityState::Ok,
            "foreground_hook_ready",
            "foreground notifications registered",
        );
    }
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
            unavailable,
            "shell_hook_failed",
            "shell subscription failed",
        );
    }
    if !state.shell_registered && state.hook.is_invalid() {
        return Err(PlatformError::new(
            "layout_hooks_failed",
            "neither layout hook could be installed",
        ));
    }
    // No permanent timer is a substitute for a failed foreground hook: without it
    // we could not reliably disarm when an ordinary application becomes foreground.
    if !state.hook.is_invalid() {
        state.update_fallback();
    }
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
        let (hwnd, facts) = foreground_facts();
        let decision = decide(&facts);
        match decision {
            PollDecision::Keep => {}
            PollDecision::Disarm if self.fallback => self.disarm(),
            PollDecision::Arm(reason) if !self.fallback => {
                // SAFETY: live window owned by this thread, nonzero window-local ID,
                // no TIMERPROC; WM_TIMER is delivered by this thread's normal pump.
                let armed =
                    unsafe { SetTimer(Some(self.window.hwnd()), FALLBACK_TIMER, 500, None) };
                if armed != 0 {
                    self.fallback = true;
                    self.ticks = 0;
                    self.fallback_recovered();
                    tracing::info!(target: "switcher_windows::layout", ?hwnd, class = facts.class_name, reason, interval_ms = 500, "foreground fallback armed");
                } else {
                    self.fallback_degraded = true;
                    report(
                        &self.events,
                        Capability::LayoutForegroundHook,
                        CapabilityState::Degraded,
                        "fallback_timer_failed",
                        "conditional layout timer could not be armed",
                    );
                }
            }
            _ => {}
        }
        if decision == PollDecision::Disarm && self.failure.is_none() {
            self.fallback_recovered();
        }
        // Keep means unavailable/raced/our own window, not evidence that the
        // current foreground is eligible for polling. Retain the timer but skip its read.
        (matches!(decision, PollDecision::Arm(_)) && self.fallback).then_some((hwnd, facts.tid))
    }

    fn fallback_recovered(&mut self) {
        if self.fallback_degraded {
            self.fallback_degraded = false;
            report(
                &self.events,
                Capability::LayoutForegroundHook,
                CapabilityState::Ok,
                "foreground_hook_ready",
                "foreground detection recovered",
            );
        }
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
        if message != WM_PUMP_WAKE {
            return PumpVerdict::Continue;
        }
        let pending = PENDING.replace(0);
        if pending & FOREGROUND != 0 {
            self.update_fallback();
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
        if pending & TIMER != 0 && self.fallback {
            // KillTimer leaves queued WM_TIMER messages behind. Also re-check the
            // foreground before publishing so a delayed foreground event cannot
            // turn an ordinary desktop window into a polled one.
            if let Some((hwnd, tid)) = self.update_fallback() {
                self.ticks += 1;
                // Bind the read to the same identity that justified this timer.
                // A focus change must not publish a polled ordinary-app snapshot.
                self.publish_read(
                    LayoutSource::ForegroundPoll,
                    snapshot::current_if_foreground(hwnd.0 as usize, tid),
                );
            }
        }
        PumpVerdict::Continue
    }
}

impl Drop for LayoutThread<'_> {
    fn drop(&mut self) {
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
        class_name: String::new(),
        open_process: OpenOutcome::OtherError,
        package: PackageOutcome::ProbeFailed,
    };
    // SAFETY: read-only desktop lookup; null is represented as tid=0/Keep.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return (hwnd, facts);
    }
    let mut pid = 0;
    let mut class = [0u16; 256];
    // SAFETY: live writable output buffers, no foreign memory is accessed directly.
    unsafe {
        facts.tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        facts.is_own_process = pid == GetCurrentProcessId();
    }
    if facts.tid == 0 || facts.is_own_process {
        return (hwnd, facts);
    }
    // SAFETY: borrowed HWND, live output buffer and exact slice capacity.
    let length = unsafe { GetClassNameW(hwnd, &mut class) };
    if length == 0 {
        return (hwnd, facts);
    }
    facts.class_name = String::from_utf16_lossy(&class[..length as usize]);
    // SAFETY: querying only, no privilege adjustment or mutation of the process.
    match unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) } {
        Ok(handle) => {
            // SAFETY: successful OpenProcess transfers one owned handle; Owned closes
            // it exactly once. No duplicate owner is retained or sent across a channel.
            let process = unsafe { Owned::new(handle) };
            facts.open_process = OpenOutcome::Ok;
            let mut length = 0;
            // SAFETY: owned process handle has query permission. A NULL output buffer
            // with zero length asks only whether a package identity exists.
            facts.package = match unsafe { GetPackageFullName(*process, &mut length, None) } {
                APPMODEL_ERROR_NO_PACKAGE => PackageOutcome::NoPackage,
                ERROR_INSUFFICIENT_BUFFER => PackageOutcome::Packaged,
                _ => PackageOutcome::ProbeFailed,
            };
        }
        Err(error) => {
            facts.open_process = if WIN32_ERROR::from_error(&error) == Some(ERROR_ACCESS_DENIED) {
                OpenOutcome::AccessDenied
            } else {
                OpenOutcome::OtherError
            };
        }
    }
    // SAFETY: re-check identity after the process probes, which can race focus changes.
    if unsafe { GetForegroundWindow() != hwnd || GetWindowThreadProcessId(hwnd, None) != facts.tid }
    {
        facts.tid = 0;
    }
    (hwnd, facts)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    fallback_degraded: false,
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
