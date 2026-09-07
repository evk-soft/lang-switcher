//! Native observation harness. Enter schedules a snapshot in 3 seconds; q exits. A numeric argument
//! limits the observation to that many seconds, for unattended registration checks.
//! It does not switch layouts or change the foreground window.

use std::cell::Cell;
use std::time::Duration;
use switcher_platform::events::Capability;
use switcher_platform::ports::PlatformError;
use switcher_windows::supervise::{callback_panic_outcome, guard_callback};
use switcher_windows::win_util::{
    HiddenWindow, PumpHandler, PumpVerdict, current_thread_id, spawn_pump,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DeregisterShellHookWindow, EVENT_SYSTEM_FOREGROUND, GetClassNameW,
    GetForegroundWindow, GetWindowThreadProcessId, RegisterShellHookWindow, RegisterWindowMessageW,
    WINEVENT_OUTOFCONTEXT,
};
use windows::core::w;

thread_local! {
    static SHELL_MESSAGE: Cell<u32> = const { Cell::new(0) };
}

fn snapshot(reason: &str) {
    let mut pid = 0;
    let mut class = [0u16; 256];
    // SAFETY: querying the current desktop. Handles are borrowed for each call;
    // window destruction is allowed and yields zero, which is not read as our TID.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        tracing::info!(reason, "no foreground window");
        return;
    }
    // SAFETY: live writable PID and class buffers, with exact capacity supplied by
    // the generated slice binding. These queries do not modify the foreign window.
    let (tid, length) = unsafe {
        (
            GetWindowThreadProcessId(hwnd, Some(&mut pid)),
            GetClassNameW(hwnd, &mut class),
        )
    };
    if tid == 0 {
        tracing::info!(reason, "foreground window disappeared");
        return;
    }
    // SAFETY: nonzero thread ID returned by Windows. This is a diagnostic snapshot;
    // a subsequent foreground change may make it stale and is logged separately.
    let hkl = unsafe { GetKeyboardLayout(tid) };
    let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
    tracing::info!(reason, hwnd = ?hwnd, tid, pid, class, hkl = ?hkl,
        langid = hkl.0 as usize & 0xffff, reader_thread = current_thread_id(), "foreground snapshot");
}

struct Probe {
    window: HiddenWindow,
    hook: HWINEVENTHOOK,
    shell_registered: bool,
}

impl Drop for Probe {
    fn drop(&mut self) {
        // SAFETY: both subscriptions were installed on this thread and their window
        // remains alive until after Drop. Each successful registration is removed once.
        unsafe {
            if !self.hook.is_invalid() && !UnhookWinEvent(self.hook).as_bool() {
                tracing::warn!("UnhookWinEvent failed");
            }
            if self.shell_registered && !DeregisterShellHookWindow(self.window.hwnd()).as_bool() {
                tracing::warn!("DeregisterShellHookWindow failed");
            }
        }
        if let Err(error) = callback_panic_outcome() {
            tracing::error!(?error, "probe callback failed");
        }
    }
}

impl PumpHandler for Probe {
    fn on_thread_message(&mut self, _: u32, _: WPARAM, _: LPARAM) -> PumpVerdict {
        PumpVerdict::Continue
    }
}

unsafe extern "system" fn shell_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    guard_callback(
        Capability::LayoutShellHook,
        || {
            // SAFETY: unchanged system-provided message parameters.
            unsafe { DefWindowProcW(hwnd, msg, w, l) }
        },
        || {
            if msg == SHELL_MESSAGE.get() {
                tracing::info!(
                    code = w.0,
                    payload = l.0,
                    thread = current_thread_id(),
                    "shell notification"
                );
                snapshot("shell");
            }
            // SAFETY: no message parameters have been altered or dereferenced.
            unsafe { DefWindowProcW(hwnd, msg, w, l) }
        },
    )
}

unsafe extern "system" fn foreground_proc(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    event_thread: u32,
    event_time: u32,
) {
    guard_callback(
        Capability::LayoutForegroundHook,
        || (),
        || {
            tracing::info!(
                event,
                ?hwnd,
                event_thread,
                event_time,
                callback_thread = current_thread_id(),
                "foreground notification"
            );
            snapshot("foreground");
        },
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let mut pump = spawn_pump("layout-probe", || {
        // SAFETY: static NUL-terminated string, registration has process lifetime.
        let message = unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) };
        if message == 0 {
            return Err(PlatformError::new(
                "register_message_failed",
                "SHELLHOOK message registration failed",
            ));
        }
        SHELL_MESSAGE.set(message);
        let window = HiddenWindow::new("LangSwitcherLayoutProbe", Some(shell_proc), None)?;
        // SAFETY: our live window on this thread; no foreign HWND is registered.
        let shell_registered = unsafe { RegisterShellHookWindow(window.hwnd()) }.as_bool();
        // SAFETY: static callback, out-of-context delivery onto this pumping thread;
        // no DLL, all processes and threads on this desktop. Drop unhooks it here.
        let hook = unsafe {
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
        tracing::info!(
            shell_registered,
            foreground_registered = !hook.is_invalid(),
            installer_thread = current_thread_id(),
            "probe registered (delivery still needs observation)"
        );
        Ok(Probe {
            window,
            hook,
            shell_registered,
        })
    })?;
    snapshot("startup");
    if let Some(seconds) = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<u64>().ok())
    {
        // This bounded observation wait belongs only to the experiment, not an adapter.
        std::thread::sleep(Duration::from_secs(seconds));
    } else {
        tracing::info!(
            "open/close an ordinary app for a positive shell control, switch layouts 10 times, then Enter for a snapshot; q exits"
        );
        loop {
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line)? == 0 || line.trim() == "q" {
                break;
            }
            tracing::info!("snapshot in 3 seconds; focus the target application now");
            std::thread::sleep(Duration::from_secs(3));
            snapshot("manual_delayed");
        }
    }
    pump.shutdown();
    Ok(())
}
