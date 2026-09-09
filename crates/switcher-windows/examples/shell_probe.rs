//! Bounded diagnostic: compare real shell messages to message-only/top-level windows.
//! This does not synthesize shell notifications or change another window's layout.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        cell::Cell,
        io::Write,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use switcher_windows::win_util::{HiddenWindow, register_class};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DeregisterShellHookWindow, DestroyWindow,
                DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, RegisterShellHookWindow,
                RegisterWindowMessageW, WINDOW_EX_STYLE, WS_OVERLAPPED,
            },
        },
        core::w,
    };
    thread_local! { static SHELL: Cell<u32> = const { Cell::new(0) }; }
    unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // Diagnostic callbacks must not unwind into User32 either.
        let _ = std::panic::catch_unwind(|| {
            if msg == SHELL.get() && msg != 0 {
                let epoch_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                // Redirect stdout to a file (or consume it continuously). In particular,
                // a closed pipe must not panic across the native callback boundary.
                let _ = writeln!(
                    std::io::stdout(),
                    "shell epoch_ms={epoch_ms} hwnd={hwnd:?} event={} payload={:#x}",
                    wp.0,
                    lp.0
                );
            }
        });
        // SAFETY: unchanged system parameters; no borrowed message payload is accessed.
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
    struct TopLevel(HWND);
    impl Drop for TopLevel {
        fn drop(&mut self) {
            // SAFETY: live hidden window owned by this same thread; one destruction.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
    struct Shell(HWND);
    impl Drop for Shell {
        fn drop(&mut self) {
            // SAFETY: successful same-thread registration; owner windows outlive this guard.
            let _ = unsafe { DeregisterShellHookWindow(self.0) };
        }
    }
    // SAFETY: static NUL-terminated string, registered identifier has process lifetime.
    SHELL.set(unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) });
    if SHELL.get() == 0 {
        return Err("RegisterWindowMessageW failed".into());
    }
    let message_only = HiddenWindow::new("LangSwitcherShellProbeMessage", Some(procedure), None)?;
    let module = register_class("LangSwitcherShellProbeTop", Some(procedure))?;
    // SAFETY: registered procedure, static text, live module, no pointers/parent/menu.
    // No WS_VISIBLE: this diagnostic cannot activate or display the probe window.
    let top = TopLevel(unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("LangSwitcherShellProbeTop"),
            w!(""),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
    }?);
    let mut subscriptions = Vec::new();
    for (kind, hwnd) in [("message-only", message_only.hwnd()), ("top-level", top.0)] {
        // SAFETY: live same-thread windows, deregistration guards drop before windows.
        let registered = unsafe { RegisterShellHookWindow(hwnd) }.as_bool();
        println!("registered kind={kind} hwnd={hwnd:?} ok={registered}");
        if !registered {
            return Err("shell registration failed".into());
        }
        subscriptions.push(Shell(hwnd));
    }
    let seconds: u64 = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(55);
    if !(1..=120).contains(&seconds) {
        return Err("duration must be 1..=120 seconds".into());
    }
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        let mut msg = MSG::default();
        // SAFETY: initialized output, current thread's queue, unchanged messages dispatched.
        // Polling is bounded diagnostic instrumentation, never used by the application.
        unsafe {
            for _ in 0..128 {
                if !PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(subscriptions);
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
