//! Message-only windows and the message pumps that own them.
//!
//! Adapter-internal by intent (it is `pub` only because the sibling modules in this crate
//! are separate compilation units). Nothing here crosses a port boundary: the app shell
//! sees `PlatformEvent`s and `TrayCommand`s, never an `HWND`.
//!
//! Two shapes of pump, deliberately not one:
//! - [`spawn_pump`] raises a **new** thread that owns a hidden window and pumps it. Every
//!   OS hook gets one of these (ADR-0009), so a slow hook cannot stall anything else.
//! - [`pump_messages`] pumps on the **calling** thread. The tray needs this one, because
//!   `TrayIcon` and the `muda` menu types are `!Send` and their setters `SendMessageW`
//!   into their own window: they can only be touched from the thread that pumps them.
//!
//! Both stop without relying on an undocumented mechanism. Microsoft documents
//! `PostThreadMessage` (the MSG's `hwnd` is NULL, such messages cannot be dispatched, the
//! target thread must already have a queue) but says **nothing** about posting `WM_QUIT`
//! through it, so this module does not: cross-thread shutdown uses a private message that
//! the pump loop itself recognises, and `WM_QUIT` only ever arrives from
//! `PostQuitMessage` on the pump's own thread, which *is* documented.

use std::ffi::c_void;
use std::thread::JoinHandle;

use switcher_platform::ports::PlatformError;
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HMODULE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    MSG, PM_NOREMOVE, PeekMessageW, PostQuitMessage, PostThreadMessageW, RegisterClassW,
    TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_USER, WNDCLASSW, WNDPROC, WS_OVERLAPPED,
};
use windows::core::PCWSTR;

/// Private cross-thread stop request. Deliberately not `WM_QUIT`: see the module docs.
const WM_PUMP_STOP: u32 = WM_APP + 0x1F00;
/// Private "you have work in your channel" nudge, used by [`PumpWaker`].
pub const WM_PUMP_WAKE: u32 = WM_APP + 0x1F01;

/// UTF-16, NUL-terminated — the shape every `*W` Win32 entry point expects.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Substituted whenever a caller passes no window procedure.
///
/// A window class with a NULL `lpfnWndProc` is not a class without behaviour — it is a
/// class Windows will call through a null pointer, and the process dies with
/// `STATUS_FATAL_USER_CALLBACK_EXCEPTION` during `CreateWindowExW`. Since `WNDPROC` is an
/// `Option` in windows-rs, that mistake is one keystroke away, so it is closed here rather
/// than documented.
unsafe extern "system" fn default_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: passing the parameters the system handed us, unchanged, to the default
    // handler — the documented behaviour for any message a class does not handle itself.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

pub fn current_thread_id() -> u32 {
    // SAFETY: no preconditions; reads the calling thread's own id and cannot fail.
    unsafe { GetCurrentThreadId() }
}

/// Publishes a usable wake target before any producer thread starts.
pub fn current_thread_waker() -> PumpWaker {
    ensure_message_queue();
    PumpWaker::new(current_thread_id())
}

/// Asks the pump on **this** thread to leave its loop. Documented for the same-thread
/// case: it posts `WM_QUIT`, and the next `GetMessageW` returns 0.
pub fn post_quit() {
    // SAFETY: posts WM_QUIT to the calling thread's own queue. No preconditions, no
    // return value to check.
    unsafe { PostQuitMessage(0) };
}

/// Registers a window class if it is not registered yet, and hands back the module handle
/// its windows must be created with.
///
/// Passing `None` for `wndproc` means "I only care about thread messages" and gets the
/// default handler — never a NULL procedure, which aborts the process from inside
/// `CreateWindowExW` (found the hard way; see [`default_wndproc`]).
///
/// **Precondition: `class_name` must be unique per window procedure in this process.**
/// Window classes are process-wide, so if the name is already registered this reuses the
/// existing class *as it was registered* — a second caller passing a different `wndproc`
/// under the same name would silently get the first one's, and its messages would go
/// somewhere it never wrote. Every call site in this crate uses a name derived from its own
/// module for that reason.
///
/// The class is never unregistered: see the note on [`HiddenWindow`]'s `Drop`.
pub fn register_class(class_name: &str, wndproc: WNDPROC) -> Result<HMODULE, PlatformError> {
    let wndproc = wndproc.or(Some(default_wndproc));
    let class = wide(class_name);

    // SAFETY: `GetModuleHandleW` with a null name returns a handle to the current process'
    // own module. That handle is not owned (nothing to free) and stays valid for the
    // process lifetime.
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }
        .map_err(|e| PlatformError::new("module_handle_failed", e.message()))?;

    let descriptor = WNDCLASSW {
        lpfnWndProc: wndproc,
        hInstance: module.into(),
        lpszClassName: PCWSTR::from_raw(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: `descriptor` lives across the call, and the string it points at (`class`)
    // outlives the call too and is NUL-terminated by `wide`. `wndproc` is an
    // `extern "system"` function pointer with the ABI Windows expects, and it points at
    // code that lives for the whole program, which is required because the class outlives
    // this call (see the note on unregistering below).
    let atom = unsafe { RegisterClassW(&descriptor) };
    if atom == 0 {
        // SAFETY: reads the calling thread's last-error value; no preconditions.
        let err = unsafe { GetLastError() };
        // A class registered by an earlier window of the same purpose is a success: the
        // class is process-wide and we intentionally never unregister it. Logged rather
        // than silent, because it is also what a name collision between two different
        // window procedures looks like (see the precondition above).
        if err == ERROR_CLASS_ALREADY_EXISTS {
            tracing::debug!(
                target: "switcher_windows::win_util",
                class = class_name,
                "reusing an already registered window class"
            );
        } else {
            return Err(PlatformError::new(
                "window_class_register_failed",
                format!("RegisterClassW({class_name}) failed: {err:?}"),
            ));
        }
    }

    Ok(module)
}

/// A message-only window, owned by the thread that created it.
///
/// Not `Send` (`HWND` is a raw pointer), and that is load-bearing rather than incidental:
/// `DestroyWindow` must run on the creating thread, and the type system is what keeps it
/// there.
#[derive(Debug)]
pub struct HiddenWindow {
    hwnd: HWND,
}

impl HiddenWindow {
    /// Registers `class_name` if needed and creates a message-only window for it.
    ///
    /// `create_param` is handed to the window procedure with `WM_NCCREATE`/`WM_CREATE`,
    /// which is how a wndproc gets access to state without a global.
    ///
    /// Carries [`register_class`]' precondition: **`class_name` must be unique per window
    /// procedure in this process.**
    pub fn new(
        class_name: &str,
        wndproc: WNDPROC,
        create_param: Option<*const c_void>,
    ) -> Result<Self, PlatformError> {
        let class = wide(class_name);
        let module = register_class(class_name, wndproc)?;

        // SAFETY: `HWND_MESSAGE` as the parent is what makes this a message-only window
        // (no rendering, not enumerated). The class name buffer is still alive and
        // NUL-terminated; `create_param` is only read by our own wndproc during creation,
        // and callers keep whatever it points at alive for at least that long.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::null(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(module.into()),
                create_param,
            )
        }
        .map_err(|e| PlatformError::new("window_create_failed", e.message()))?;

        Ok(Self { hwnd })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        // NOTE: the window class is never unregistered. `UnregisterClassW` fails while any
        // window of the class still exists, and classes are released when the module
        // unloads, so calling it here would be a no-op that looks like error handling.
        // This is not a leak; it is the documented lifetime of a window class.
        //
        // SAFETY: runs on the creating thread, which `HiddenWindow: !Send` guarantees, and
        // `self.hwnd` came from a successful `CreateWindowExW` and is destroyed exactly
        // once because `Drop` runs once.
        if let Err(e) = unsafe { DestroyWindow(self.hwnd) } {
            tracing::warn!(target: "switcher_windows::win_util", error = %e, "DestroyWindow failed");
        }
    }
}

/// Whether the pump should keep going after a callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpVerdict {
    Continue,
    Quit,
}

/// The thread-message half of a pump's behaviour. Window messages go to the wndproc;
/// everything posted with [`PumpThread::post`] arrives here instead, because a message
/// with a NULL `hwnd` cannot be dispatched to a window at all.
pub trait PumpHandler {
    /// Checked between messages and before blocking; cancellation wakes the queue.
    fn should_stop(&self) -> bool {
        false
    }
    fn on_thread_message(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> PumpVerdict;
}

/// Establish the message queue before another thread is allowed to wake this one.
pub(crate) fn ensure_message_queue() {
    let mut probe = MSG::default();
    // SAFETY: live MSG, no queue entries removed. This is Microsoft's documented
    // PostThreadMessage handshake: a User call creates the calling thread's queue.
    let _ = unsafe { PeekMessageW(&mut probe, None, WM_USER, WM_USER, PM_NOREMOVE) };
}

/// A callback can fail during setup before a pump consumes its PostQuitMessage.
/// Clear the previous attempt's queue only after ALL of its native owners are gone.
/// WM_QUIT is synthesized after ordinary posts: filtering only for quit leaves a
/// latent quit behind an old wake, which would terminate the replacement pump.
pub(crate) fn discard_previous_quit() {
    let mut message = MSG::default();
    // SAFETY: the supervisor owns this entire thread, and the previous run has
    // destroyed its windows, unhooked callbacks and uninitialized COM. No live
    // source's messages are discarded. Cancellation lives in a separate atomic/channel.
    while unsafe {
        PeekMessageW(
            &mut message,
            None,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
        )
    }
    .as_bool()
    {}
}

/// Handle to a pump running on its own thread. Dropping it stops that thread.
#[derive(Debug)]
pub struct PumpThread {
    name: &'static str,
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl PumpThread {
    pub fn thread_id(&self) -> u32 {
        self.thread_id
    }

    /// A waker for this pump, safe to move to another thread.
    pub fn waker(&self) -> PumpWaker {
        PumpWaker {
            thread_id: self.thread_id,
        }
    }

    /// Posts a thread message, which the pump hands to its [`PumpHandler`].
    pub fn post(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Result<(), PlatformError> {
        post_thread_message(self.thread_id, msg, wparam, lparam)
    }

    /// Stops the pump and joins its thread. Idempotent, and also called from `Drop`.
    pub fn shutdown(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        // A failure here means the thread already left its loop (its queue is gone), which
        // is exactly the state we are trying to reach — so it is logged, not propagated.
        if let Err(e) = post_thread_message(self.thread_id, WM_PUMP_STOP, WPARAM(0), LPARAM(0)) {
            tracing::debug!(
                target: "switcher_windows::win_util",
                pump = self.name, error = %e.detail,
                "stop request not delivered; the pump had already exited"
            );
        }
        if join.join().is_err() {
            tracing::error!(
                target: "switcher_windows::win_util",
                pump = self.name,
                "pump thread panicked"
            );
        }
    }
}

impl Drop for PumpThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Wakes a pump owned by another thread. `Send`, because it is just a thread id — no
/// handle crosses the boundary (ADR-0009).
#[derive(Debug, Clone, Copy)]
pub struct PumpWaker {
    thread_id: u32,
}

impl PumpWaker {
    pub fn new(thread_id: u32) -> Self {
        Self { thread_id }
    }

    /// Nudges the target pump so it drains its command channel.
    ///
    /// Caveat worth knowing at the call site: Microsoft documents that thread messages are
    /// **lost** if the recipient is inside a modal loop (`TrackPopupMenu`, `MessageBox`).
    /// A pump must therefore never treat this nudge as its only trigger to check for work.
    pub fn wake(&self) -> Result<(), PlatformError> {
        post_thread_message(self.thread_id, WM_PUMP_WAKE, WPARAM(0), LPARAM(0))
    }
}

fn post_thread_message(
    thread_id: u32,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Result<(), PlatformError> {
    // SAFETY: `thread_id` was obtained from a pump that had already created its message
    // queue before publishing the id (see `spawn_pump`), which is the documented
    // precondition — without a queue the call fails with ERROR_INVALID_THREAD_ID.
    unsafe { PostThreadMessageW(thread_id, msg, wparam, lparam) }
        .map_err(|e| PlatformError::new("post_thread_message_failed", e.message()))
}

/// Raises a thread that runs `setup` and then pumps messages until stopped.
///
/// `setup` runs **on the new thread** on purpose: it is where the hidden window is created,
/// and `HiddenWindow` is `!Send`, so it could not be handed over even if we wanted to.
/// The call returns only after `setup` finished, so a caller that gets an `Ok` back knows
/// the pump is ready to receive posts.
///
/// **Never call this from inside a `spawn_supervised` body.** The supervisor already owns a
/// thread; spawning another one puts the callbacks where the supervisor cannot see them.
/// `guard_callback` records a caught panic in a thread-local, so the record would land on
/// this inner thread while `callback_panic_outcome` on the supervised thread reports
/// success — the source would fall silent and never be restarted. Supervised hooks call
/// [`pump_with_handler`] inline instead.
pub fn spawn_pump<S, H>(name: &'static str, setup: S) -> Result<PumpThread, PlatformError>
where
    S: FnOnce() -> Result<H, PlatformError> + Send + 'static,
    H: PumpHandler,
{
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<u32, PlatformError>>(1);

    let join = std::thread::Builder::new()
        .name(format!("switcher-{name}"))
        .spawn(move || {
            let mut handler = match setup() {
                Ok(handler) => handler,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };

            // Force the message queue into existence before publishing the thread id.
            // This is the method Microsoft prescribes for exactly this race: a
            // `PostThreadMessage` sent before the target thread has a queue fails with
            // ERROR_INVALID_THREAD_ID, and the first post would be silently lost.
            // `setup` will usually have created a window already (which also creates the
            // queue), but a handler without a window is legitimate and must not break the
            // handshake.
            let mut probe = MSG::default();
            // SAFETY: `probe` is a live, zero-initialized MSG that the call may fill in.
            // PM_NOREMOVE leaves the queue untouched; the only effect we want is the queue
            // being created as a side effect of the first User call on this thread.
            let _ = unsafe { PeekMessageW(&mut probe, None, WM_USER, WM_USER, PM_NOREMOVE) };

            if ready_tx.send(Ok(current_thread_id())).is_err() {
                // Nobody is waiting for us any more: the caller went away between spawn
                // and handshake. Tear down rather than pump forever.
                return;
            }
            if let Err(error) = pump_with_handler(name, &mut handler) {
                tracing::error!(pump = name, ?error, "pump failed");
            }
        })
        .map_err(|e| PlatformError::new("pump_thread_spawn_failed", e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(PumpThread {
            name,
            thread_id,
            join: Some(join),
        }),
        Ok(Err(setup_error)) => {
            let _ = join.join();
            Err(setup_error)
        }
        Err(_) => {
            let _ = join.join();
            Err(PlatformError::new(
                "pump_thread_died",
                format!("the {name} pump thread ended before its handshake"),
            ))
        }
    }
}

/// Pumps messages on the **calling** thread until stopped, handing thread messages to
/// `handler` and window messages to their wndproc.
///
/// This is the shape a supervised hook wants (`spawn_supervised` already owns the thread,
/// so it must pump inline rather than spawn another one — see the warning on
/// [`spawn_pump`]). Returns when it receives a stop request, when `handler` answers
/// [`PumpVerdict::Quit`], or when `WM_QUIT` arrives from this thread's own `post_quit`.
///
/// A caller that needs to be stoppable from elsewhere publishes
/// `PumpWaker::new(current_thread_id())` before entering, and the owner posts a stop
/// through it.
pub fn pump_with_handler<H: PumpHandler>(
    name: &'static str,
    handler: &mut H,
) -> Result<(), PlatformError> {
    while !handler.should_stop() {
        let mut msg = MSG::default();
        // SAFETY: `msg` is a live MSG the call fills in before we read it. A NULL window
        // filter means "any message for this thread", which is what a pump wants.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };

        if got.0 == -1 {
            // SAFETY: reads the calling thread's last-error value; no preconditions.
            let err = unsafe { GetLastError() };
            tracing::error!(
                target: "switcher_windows::win_util",
                pump = name, error = ?err,
                "GetMessageW failed; leaving the pump loop"
            );
            return Err(PlatformError::new(
                "get_message_failed",
                format!("{name}: {err:?}"),
            ));
        }
        if got.0 == 0 {
            // WM_QUIT — only ever posted by this thread itself via `post_quit`.
            return Ok(());
        }

        // A NULL hwnd marks a thread message. Microsoft is explicit that such messages
        // cannot be dispatched to a window, so they must be handled here instead of being
        // handed to `DispatchMessageW`.
        if msg.hwnd.0.is_null() {
            if msg.message == WM_PUMP_STOP {
                return Ok(());
            }
            if handler.on_thread_message(msg.message, msg.wParam, msg.lParam) == PumpVerdict::Quit {
                return Ok(());
            }
            continue;
        }

        // SAFETY: `msg` was filled in by `GetMessageW` above and describes a real window
        // message for a window owned by this thread.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Pumps messages on the **calling** thread until it is asked to quit.
///
/// Drain commands initially, before dispatch, and after dispatch returns. The last
/// drain is essential: a nested modal menu can consume all posted thread wakes.
/// Commands themselves remain in the caller's channel and must run before blocking.
/// A modal callback entered by a sent message *inside GetMessageW* must itself post
/// a message on return; GetMessageW otherwise keeps waiting. tray-icon 0.24.1 does
/// this with WM_NULL after TrackPopupMenu. This is not a general-purpose modal pump.
pub fn pump_messages(mut on_iter: impl FnMut() -> PumpVerdict) -> Result<(), PlatformError> {
    ensure_message_queue();
    if on_iter() == PumpVerdict::Quit {
        post_quit();
    }
    loop {
        let mut msg = MSG::default();
        // SAFETY: as in `run_pump` — a live MSG, no window filter.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };

        if got.0 == -1 {
            // SAFETY: reads the calling thread's last-error value; no preconditions.
            let err = unsafe { GetLastError() };
            return Err(PlatformError::new(
                "get_message_failed",
                format!("GetMessageW failed: {err:?}"),
            ));
        }
        if got.0 == 0 {
            return Ok(());
        }

        if on_iter() == PumpVerdict::Quit {
            post_quit();
            continue;
        }

        if !msg.hwnd.0.is_null() {
            // SAFETY: as in `run_pump` — a window message for a window of this thread.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if on_iter() == PumpVerdict::Quit {
                post_quit();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    struct Recorder {
        seen: Arc<Mutex<Vec<u32>>>,
        stop_after: u32,
    }

    impl PumpHandler for Recorder {
        fn on_thread_message(&mut self, msg: u32, _w: WPARAM, _l: LPARAM) -> PumpVerdict {
            self.seen.lock().expect("test mutex").push(msg);
            if msg == self.stop_after {
                PumpVerdict::Quit
            } else {
                PumpVerdict::Continue
            }
        }
    }

    /// Reaching the handler *is* the assertion: `run_pump` only routes a message there
    /// when its `hwnd` is NULL, which is what Microsoft documents for posted thread
    /// messages — and the same reason they must never go to `DispatchMessageW`.
    #[test]
    fn thread_messages_reach_the_handler_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        let mut pump = spawn_pump("test-thread-msgs", move || {
            Ok(Recorder {
                seen: recorded,
                stop_after: WM_APP + 2,
            })
        })
        .expect("pump starts");

        pump.post(WM_APP + 1, WPARAM(0), LPARAM(0)).expect("post 1");
        pump.post(WM_APP + 2, WPARAM(0), LPARAM(0)).expect("post 2");
        pump.shutdown();

        assert_eq!(
            *seen.lock().expect("test mutex"),
            vec![WM_APP + 1, WM_APP + 2]
        );
    }

    static WNDPROC_HITS: AtomicUsize = AtomicUsize::new(0);
    static MODAL_COMMAND_PENDING: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    unsafe extern "system" fn modal_test_wndproc(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> LRESULT {
        crate::supervise::guard_callback(
            switcher_platform::events::Capability::Overlay,
            || LRESULT(0),
            || {
                if msg == WM_APP + 8 {
                    let mut discarded = MSG::default();
                    // SAFETY: live MSG, this thread's queue. Emulate a modal loop consuming
                    // the thread wake while its corresponding channel command remains.
                    let got = unsafe {
                        PeekMessageW(
                            &mut discarded,
                            None,
                            WM_PUMP_WAKE,
                            WM_PUMP_WAKE,
                            windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
                        )
                    };
                    MODAL_COMMAND_PENDING.store(got.as_bool(), Ordering::Release);
                    return LRESULT(0);
                }
                // SAFETY: untouched arguments delivered by Windows to this window.
                unsafe { DefWindowProcW(hwnd, msg, w, l) }
            },
        )
    }

    #[test]
    fn channel_commands_are_drained_after_a_modal_loop_consumes_the_wake() {
        MODAL_COMMAND_PENDING.store(false, Ordering::Release);
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let (done_tx, done_rx) = crossbeam_channel::bounded(1);
        let thread = std::thread::spawn(move || {
            let window =
                HiddenWindow::new("SwitcherModalDrainTest", Some(modal_test_wndproc), None)
                    .unwrap();
            let tid = current_thread_id();
            // SAFETY: the window lives on this thread through the pump, messages have
            // no pointer arguments. Queue the modal entry before its wake.
            unsafe { PostMessageW(Some(window.hwnd()), WM_APP + 8, WPARAM(0), LPARAM(0)) }.unwrap();
            PumpWaker::new(tid).wake().unwrap();
            ready_tx.send(tid).unwrap();
            pump_messages(|| {
                if MODAL_COMMAND_PENDING.swap(false, Ordering::AcqRel) {
                    let _ = done_tx.send(());
                    PumpVerdict::Quit
                } else {
                    PumpVerdict::Continue
                }
            })
            .unwrap();
        });
        let tid = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let completed_without_an_extra_message =
            done_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        if !completed_without_an_extra_message {
            PumpWaker::new(tid).wake().unwrap();
        }
        thread.join().unwrap();
        assert!(
            completed_without_an_extra_message,
            "pump slept with a pending channel command after modal return"
        );
    }
    const WM_TEST_PING: u32 = WM_APP + 7;

    unsafe extern "system" fn counting_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_TEST_PING {
            WNDPROC_HITS.fetch_add(1, Ordering::SeqCst);
            return LRESULT(0);
        }
        // SAFETY: forwarding a message we do not handle to the default handler, with the
        // parameters the system passed in unchanged.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    struct WindowOwner {
        _window: HiddenWindow,
    }

    impl PumpHandler for WindowOwner {
        fn on_thread_message(&mut self, _msg: u32, _w: WPARAM, _l: LPARAM) -> PumpVerdict {
            PumpVerdict::Continue
        }
    }

    #[test]
    fn window_messages_are_dispatched_to_the_wndproc() {
        WNDPROC_HITS.store(0, Ordering::SeqCst);
        let (hwnd_tx, hwnd_rx) = crossbeam_channel::bounded::<usize>(1);

        let mut pump = spawn_pump("test-wndproc", move || {
            let window =
                HiddenWindow::new("SwitcherTestCountingWndproc", Some(counting_wndproc), None)?;
            // Only a number crosses the thread boundary; the window itself stays with the
            // thread that will destroy it.
            let _ = hwnd_tx.send(window.hwnd().0 as usize);
            Ok(WindowOwner { _window: window })
        })
        .expect("pump starts");

        let raw = hwnd_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("setup published its hwnd");

        // SAFETY: `raw` is the HWND the pump thread just created and still owns, so the
        // window is alive for this call. Posting is thread-safe by design: the message is
        // queued for the owning thread rather than executed here.
        unsafe {
            PostMessageW(
                Some(HWND(raw as *mut c_void)),
                WM_TEST_PING,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .expect("post window message");

        pump.shutdown();
        assert_eq!(WNDPROC_HITS.load(Ordering::SeqCst), 1);
    }

    struct ThreadIdOnDrop {
        _window: HiddenWindow,
        dropped_on: Arc<AtomicU32>,
    }

    impl PumpHandler for ThreadIdOnDrop {
        fn on_thread_message(&mut self, _msg: u32, _w: WPARAM, _l: LPARAM) -> PumpVerdict {
            PumpVerdict::Continue
        }
    }

    impl Drop for ThreadIdOnDrop {
        fn drop(&mut self) {
            self.dropped_on.store(current_thread_id(), Ordering::SeqCst);
        }
    }

    /// A threading-contract test, not decoration: `DestroyWindow` has to run on the thread
    /// that created the window, and the handler (which owns it) is dropped by the pump
    /// thread. If that ever changed, this catches it.
    #[test]
    fn the_window_owner_is_dropped_on_the_pump_thread() {
        let dropped_on = Arc::new(AtomicU32::new(0));
        let recorded = Arc::clone(&dropped_on);

        let mut pump = spawn_pump("test-drop-thread", move || {
            let window = HiddenWindow::new("SwitcherTestDropThread", None, None)?;
            Ok(ThreadIdOnDrop {
                _window: window,
                dropped_on: recorded,
            })
        })
        .expect("pump starts");

        let pump_thread = pump.thread_id();
        pump.shutdown();

        assert_eq!(dropped_on.load(Ordering::SeqCst), pump_thread);
        assert_ne!(
            pump_thread,
            current_thread_id(),
            "the pump must own a thread of its own"
        );
    }

    #[test]
    fn wide_is_nul_terminated() {
        assert_eq!(wide("ab"), vec![b'a' as u16, b'b' as u16, 0]);
        assert_eq!(wide(""), vec![0]);
    }
}
