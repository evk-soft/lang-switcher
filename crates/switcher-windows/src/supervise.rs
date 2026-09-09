//! Keeping hook threads alive: restart with backoff, then degrade loudly (ADR-0007).
//!
//! Two halves that only work together:
//!
//! 1. [`spawn_supervised`] wraps a thread body in `catch_unwind` and restarts it on the
//!    [`BACKOFF_MS`] schedule, giving up after the budget is spent and reporting
//!    `CapabilityChanged(.., Off)` so the tray can say so.
//! 2. [`guard_callback`] wraps the body of each of **our** `extern "system"` callbacks.
//!    This one is not optional garnish: `WNDPROC` and `WINEVENTPROC` are declared
//!    `extern "system"`, not `extern "system-unwind"`, and since Rust 1.81 an unwind that
//!    reaches such a boundary **aborts the process**. The hook work happens inside those
//!    callbacks, so without (2) the `catch_unwind` in (1) would never see the panics that
//!    actually matter — the app would die instead, without even flushing its log.
//!
//! The whole mechanism also assumes `panic = "unwind"`; `panic = "abort"` is forbidden in
//! every profile for exactly this reason (see the note in the workspace `Cargo.toml`).
//!
//! The numbers (250/1000/4000 ms, healthy after 60 s) are engineering judgement, not
//! derived from any documentation. They are meant to be checked empirically — kill
//! `explorer.exe` and measure how long the shell hook takes to come back.

use std::cell::Cell;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, bounded};
use switcher_platform::events::{Capability, CapabilityReport, CapabilityState, PlatformEvent};
use switcher_platform::ports::PlatformError;

use crate::win_util::{
    PumpWaker, current_thread_id, discard_previous_quit, ensure_message_queue, post_quit,
};

/// Delay before each restart attempt. Three restarts inside ~5.25 s: a transient (an
/// `explorer.exe` restart taking the shell hook with it) recovers on the first or second,
/// and a thread that died three times in five seconds will not be revived by persistence.
pub const BACKOFF_MS: [u64; 3] = [250, 1_000, 4_000];

/// A thread that ran this long without dying is healthy: reset its restart budget. Without
/// this, a process running for a week accumulates unrelated failures and eventually
/// silences a source that was working fine.
pub const HEALTHY_AFTER_MS: u64 = 60_000;

/// The restart bookkeeping, split out so it can be tested without threads.
#[derive(Debug, Default)]
pub struct RestartBudget {
    attempt: usize,
}

impl RestartBudget {
    pub const fn new() -> Self {
        Self { attempt: 0 }
    }

    /// `ran_ms` is how long the run that just ended lasted. Returns how long to wait
    /// before the next attempt, or `None` once the budget is spent.
    pub fn on_exit(&mut self, ran_ms: u64) -> Option<u64> {
        if ran_ms >= HEALTHY_AFTER_MS {
            self.attempt = 0;
        }
        let delay = BACKOFF_MS.get(self.attempt).copied();
        if delay.is_some() {
            // Saturating in spirit: once past the end of the schedule the index stops
            // moving, so an exhausted budget stays exhausted instead of indexing further.
            self.attempt += 1;
        }
        delay
    }
}

thread_local! {
    /// Set when `guard_callback` caught a panic on this thread. Read by
    /// [`callback_panic_outcome`].
    static CALLBACK_PANICKED: Cell<bool> = const { Cell::new(false) };
}

/// Runs the body of one of our `extern "system"` callbacks so a panic cannot cross the FFI
/// boundary and abort the process.
///
/// On a caught panic it logs, remembers the failure for [`callback_panic_outcome`], and
/// asks this thread's pump to exit — so the supervisor sees the thread end, learns it
/// ended badly, and restarts it with backoff.
///
/// `on_panic` produces what the callback returns to Windows in that case
/// (`DefWindowProcW`'s result for a wndproc, `()` for a hook proc). It is a **closure, not
/// a value**, and deliberately so: the obvious spelling of a wndproc is
/// `guard_callback(cap, || DefWindowProcW(..), || { .. })`, and with an eagerly evaluated
/// default that `DefWindowProcW` would run on every single message, including the ones the
/// body handled itself — a message processed twice, on the happy path, forever.
pub fn guard_callback<T>(
    cap: Capability,
    on_panic: impl FnOnce() -> T,
    body: impl FnOnce() -> T,
) -> T {
    match std::panic::catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            CALLBACK_PANICKED.set(true);
            tracing::error!(
                target: "switcher_windows::supervise",
                cap = cap.key(),
                panic = %panic_text(&payload),
                "panic inside an extern \"system\" callback; asking the thread to restart"
            );
            post_quit();
            on_panic()
        }
    }
}

/// What a supervised thread body must return at its end: `Err` if a callback on this thread
/// panicked, so the supervisor restarts instead of treating the exit as intentional.
///
/// Without this the flow is subtly wrong: `guard_callback` stops the pump, the body returns
/// `Ok(())`, and the supervisor reads that as "asked to stop" and never restarts.
pub fn callback_panic_outcome() -> Result<(), PlatformError> {
    if CALLBACK_PANICKED.replace(false) {
        return Err(PlatformError::new(
            "callback_panicked",
            "a panic was caught at an extern \"system\" boundary on this thread",
        ));
    }
    Ok(())
}

/// Runs `run` on a dedicated thread, restarting it on failure per [`BACKOFF_MS`].
///
/// `run` returning `Ok(())` means "I was asked to stop" and ends the supervision. Any
/// `Err`, or a panic in a Rust frame, is a failure worth retrying.
///
/// **`run` must own its message pump inline**, on the thread this function gives it — call
/// [`crate::win_util::pump_with_handler`], never `spawn_pump`. Spawning a second thread
/// from inside a supervised body puts the callbacks somewhere the supervisor cannot see:
/// [`guard_callback`]'s record of a panic is thread-local, so it would be set on the inner
/// thread while [`callback_panic_outcome`] on the supervised thread reports success, and
/// the source would go quiet without ever being restarted.
///
/// Returns an error rather than panicking when the OS refuses a thread, because the whole
/// point of this module is degrading instead of dying (ADR-0007).
pub fn spawn_supervised<F>(
    capabilities: &'static [Capability],
    tx: Sender<PlatformEvent>,
    run: F,
) -> Result<SupervisedThread, PlatformError>
where
    F: Fn(&Sender<PlatformEvent>, &StopToken) -> Result<(), PlatformError> + Send + 'static,
{
    let Some(first) = capabilities.first() else {
        return Err(PlatformError::new(
            "empty_capabilities",
            "a source thread must own a capability",
        ));
    };
    let stopped = Arc::new(AtomicBool::new(false));
    let (stop_tx, stop_rx) = bounded(1);
    let token = StopToken {
        stopped: Arc::clone(&stopped),
        receiver: stop_rx,
    };
    let (ready_tx, ready_rx) = bounded(1);
    let join = std::thread::Builder::new()
        .name(format!("switcher-{}", first.key()))
        .spawn(move || {
            ensure_message_queue();
            if ready_tx.send(current_thread_id()).is_err() {
                return;
            }
            supervise_loop(capabilities, tx, &token, run);
        })
        .map_err(|e| PlatformError::new("supervised_thread_spawn_failed", e.to_string()))?;
    match ready_rx.recv() {
        Ok(thread_id) => Ok(SupervisedThread {
            stopped,
            stop_tx,
            waker: PumpWaker::new(thread_id),
            join: Some(join),
        }),
        Err(error) => {
            let _ = join.join();
            Err(PlatformError::new(
                "supervised_thread_died",
                error.to_string(),
            ))
        }
    }
}

/// Native pumps inspect this only when woken; backoff waits on its channel (ADR-0012).
#[derive(Debug)]
pub struct StopToken {
    stopped: Arc<AtomicBool>,
    receiver: Receiver<()>,
}

impl StopToken {
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub struct SupervisedThread {
    stopped: Arc<AtomicBool>,
    stop_tx: Sender<()>,
    waker: PumpWaker,
    join: Option<JoinHandle<()>>,
}

impl SupervisedThread {
    pub fn shutdown(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        self.stopped.store(true, Ordering::Release);
        let _ = self.stop_tx.try_send(());
        // The queue is established before construction returns. If it has already
        // gone away the thread finished; if it is full, the next iteration sees stop.
        let _ = self.waker.wake();
        if join.join().is_err() {
            tracing::error!("supervised thread panicked outside its run body");
        }
    }
}

impl Drop for SupervisedThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn supervise_loop<F>(
    capabilities: &[Capability],
    tx: Sender<PlatformEvent>,
    stop: &StopToken,
    run: F,
) where
    F: Fn(&Sender<PlatformEvent>, &StopToken) -> Result<(), PlatformError>,
{
    let mut budget = RestartBudget::new();
    let mut attempt = 0usize;

    while !stop.is_stopped() {
        discard_previous_quit();
        // Start each attempt with a clean slate. Without this a panic whose flag was never
        // consumed — because the body returned early for some other reason — would leak
        // into the *next* attempt and turn a legitimate clean exit into a restart.
        let _ = CALLBACK_PANICKED.replace(false);

        let started = Instant::now();
        // `AssertUnwindSafe` because `Sender` is not `UnwindSafe`. That is acceptable here:
        // a panic cannot leave the channel in a torn state, it can only drop a message.
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| run(&tx, stop)));
        let callback_outcome = callback_panic_outcome();
        let ran_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        if stop.is_stopped() {
            return;
        }
        let outcome = outcome.map(|result| result.and(callback_outcome));
        let reason = match outcome {
            Ok(Ok(())) => return,
            Ok(Err(e)) => format!("{}: {}", e.code, e.detail),
            Err(payload) => format!("panic: {}", panic_text(&payload)),
        };
        attempt += 1;

        match budget.on_exit(ran_ms) {
            Some(delay_ms) => {
                tracing::warn!(
                    target: "switcher_windows::supervise",
                    ?capabilities, attempt, ran_ms, delay_ms, reason = %reason,
                    "supervised thread ended; restarting after backoff"
                );
                report_failure(
                    &tx,
                    capabilities,
                    CapabilityState::Degraded,
                    "source_restart_pending",
                    &reason,
                );
                if stop
                    .receiver
                    .recv_timeout(Duration::from_millis(delay_ms))
                    .is_ok()
                    || stop.is_stopped()
                {
                    return;
                }
            }
            None => {
                tracing::error!(
                    target: "switcher_windows::supervise",
                    ?capabilities, attempt, ran_ms, reason = %reason,
                    "restart budget exhausted; capability is off for the rest of this run"
                );
                // Same channel as every other event: ordering against LayoutChanged is
                // meaningful, so a second channel would be wrong (ADR-0007).
                report_failure(
                    &tx,
                    capabilities,
                    CapabilityState::Off,
                    "restart_budget_exhausted",
                    &reason,
                );
                return;
            }
        }
    }
}

fn report_failure(
    tx: &Sender<PlatformEvent>,
    capabilities: &[Capability],
    state: CapabilityState,
    code: &'static str,
    detail: &str,
) {
    for &capability in capabilities {
        let _ = tx.send(PlatformEvent::CapabilityChanged(CapabilityReport {
            capability,
            state,
            code,
            detail: detail.into(),
        }));
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_interrupts_backoff_and_covers_all_capabilities() {
        let (events, received) = crossbeam_channel::unbounded();
        let mut worker = spawn_supervised(
            &[
                Capability::LayoutShellHook,
                Capability::LayoutForegroundHook,
            ],
            events,
            |_, _| {
                Err(PlatformError::new(
                    "test_failure",
                    "intentional setup failure",
                ))
            },
        )
        .expect("spawn supervised worker");
        // The third failed attempt starts the four-second backoff. Each failure
        // must mark BOTH capabilities degraded before waiting.
        for _ in 0..3 {
            for capability in [
                Capability::LayoutShellHook,
                Capability::LayoutForegroundHook,
            ] {
                let event = received
                    .recv_timeout(Duration::from_secs(3))
                    .expect("failure report");
                let PlatformEvent::CapabilityChanged(report) = event else {
                    panic!("unexpected event");
                };
                assert_eq!(report.capability, capability);
                assert_eq!(report.state, CapabilityState::Degraded);
            }
        }
        let started = Instant::now();
        worker.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "shutdown must interrupt four-second backoff"
        );
        assert!(
            received.try_recv().is_err(),
            "stopping is not another failure"
        );
    }

    #[test]
    fn stop_wakes_a_blocked_native_pump() {
        use crate::win_util::{PumpHandler, PumpVerdict, pump_with_handler};
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        struct Waiting<'a>(&'a StopToken);
        impl PumpHandler for Waiting<'_> {
            fn should_stop(&self) -> bool {
                self.0.is_stopped()
            }
            fn on_thread_message(&mut self, _: u32, _: WPARAM, _: LPARAM) -> PumpVerdict {
                PumpVerdict::Continue
            }
        }
        let (events, _) = crossbeam_channel::unbounded();
        let (entered, ready) = crossbeam_channel::bounded(1);
        let (exited, done) = crossbeam_channel::bounded(1);
        let mut worker = spawn_supervised(&[Capability::LayoutTsf], events, move |_, stop| {
            entered.send(()).expect("announce pump");
            pump_with_handler("test-cancel", &mut Waiting(stop))?;
            exited.send(()).expect("announce pump exit");
            Ok(())
        })
        .expect("spawn pump");
        ready
            .recv_timeout(Duration::from_secs(2))
            .expect("pump entered");
        worker.shutdown();
        done.recv_timeout(Duration::from_secs(2))
            .expect("native loop completed");
    }

    #[test]
    fn quit_from_failed_setup_does_not_terminate_the_next_attempt() {
        use std::sync::atomic::AtomicUsize;
        use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_NOREMOVE, PeekMessageW};
        let calls = AtomicUsize::new(0);
        let (events, _) = crossbeam_channel::unbounded();
        let (result, received) = bounded(1);
        let mut worker = spawn_supervised(&[Capability::LayoutTsf], events, move |_, _| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                PumpWaker::new(current_thread_id())
                    .wake()
                    .expect("ordinary message before quit");
                guard_callback(
                    Capability::LayoutTsf,
                    || (),
                    || panic!("callback during setup"),
                );
                return Err(PlatformError::new(
                    "setup_failed",
                    "before the pump started",
                ));
            }
            let mut message = MSG::default();
            // SAFETY: live output; inspect this isolated worker's queue without removal.
            let stale = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) }.as_bool();
            result.send(stale).expect("publish queue observation");
            Ok(())
        })
        .expect("spawn worker");
        assert!(
            !received
                .recv_timeout(Duration::from_secs(2))
                .expect("second attempt")
        );
        worker.shutdown();
    }

    #[test]
    fn backoff_walks_the_schedule_then_gives_up() {
        let mut budget = RestartBudget::new();
        assert_eq!(budget.on_exit(10), Some(250));
        assert_eq!(budget.on_exit(10), Some(1_000));
        assert_eq!(budget.on_exit(10), Some(4_000));
        assert_eq!(budget.on_exit(10), None);
    }

    /// Guards against indexing `BACKOFF_MS` past its end once the budget is spent.
    #[test]
    fn exhausted_budget_stays_exhausted_and_never_panics() {
        let mut budget = RestartBudget::new();
        for _ in 0..BACKOFF_MS.len() {
            assert!(budget.on_exit(10).is_some());
        }
        assert_eq!(budget.on_exit(10), None);
        assert_eq!(budget.on_exit(10), None);
    }

    #[test]
    fn a_healthy_run_resets_the_budget() {
        let mut budget = RestartBudget::new();
        assert_eq!(budget.on_exit(10), Some(250));
        assert_eq!(budget.on_exit(10), Some(1_000));
        assert_eq!(
            budget.on_exit(HEALTHY_AFTER_MS),
            Some(250),
            "a run that lasted the healthy threshold must start the schedule over"
        );
    }

    #[test]
    fn just_under_the_healthy_threshold_does_not_reset() {
        let mut budget = RestartBudget::new();
        assert_eq!(budget.on_exit(10), Some(250));
        assert_eq!(
            budget.on_exit(HEALTHY_AFTER_MS - 1),
            Some(1_000),
            "one millisecond short of healthy must continue the schedule, not restart it"
        );
    }

    #[test]
    fn guard_callback_passes_values_through_and_leaves_no_failure_behind() {
        let fallback_calls = Cell::new(0);
        let value = guard_callback(
            Capability::Overlay,
            || {
                fallback_calls.set(fallback_calls.get() + 1);
                -1
            },
            || 42,
        );
        assert_eq!(value, 42);
        assert_eq!(
            fallback_calls.get(),
            0,
            "the fallback must stay unevaluated on the happy path: for a wndproc it is \
             DefWindowProcW, and calling it here would process every message twice"
        );
        assert!(callback_panic_outcome().is_ok());
    }

    /// The panic message from the caught panic shows up in the test output; that is the
    /// default panic hook doing its job, not a failure. Installing a custom hook would be
    /// process-global and could swallow another test's output.
    #[test]
    fn guard_callback_contains_a_panic_and_reports_it_to_the_supervisor() {
        let value = guard_callback(Capability::LayoutTsf, || -1, || panic!("boom"));
        assert_eq!(
            value, -1,
            "the callback must return the safe default to Windows"
        );

        let outcome = callback_panic_outcome();
        let err = outcome.expect_err("the supervisor must be told the thread ended badly");
        assert_eq!(err.code, "callback_panicked");

        assert!(
            callback_panic_outcome().is_ok(),
            "the flag must be consumed, so one panic cannot cause endless restarts"
        );
    }
}
