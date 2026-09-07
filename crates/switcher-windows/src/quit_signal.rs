//! Main-thread shutdown that remains dispatchable inside a native modal menu (ADR-0015).

use crate::win_util::{HiddenWindow, post_quit};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use switcher_platform::ports::PlatformError;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{DefWindowProcW, PostMessageW, WM_APP},
};

const WM_REQUEST_QUIT: u32 = WM_APP + 0x1F02;

/// Keep on the main thread until all senders have stopped. Owns the native window.
#[derive(Debug)]
pub struct QuitWindow {
    window: HiddenWindow,
    target: Arc<AtomicUsize>,
}

impl QuitWindow {
    pub fn new() -> Result<Self, PlatformError> {
        let window = HiddenWindow::new("LangSwitcherQuitSignal", Some(quit_proc), None)?;
        let target = Arc::new(AtomicUsize::new(window.hwnd().0 as usize));
        Ok(Self { window, target })
    }

    pub fn signal(&self) -> QuitSignal {
        QuitSignal {
            target: Arc::clone(&self.target),
        }
    }
}

impl Drop for QuitWindow {
    fn drop(&mut self) {
        self.target.store(0, Ordering::Release);
        // Touch the owner explicitly: its Drop follows this body on the same thread.
        let _ = self.window.hwnd();
    }
}

/// A revocable numeric target, no HWND wrapper or borrowed Rust pointer crosses threads.
#[derive(Debug, Clone)]
pub struct QuitSignal {
    target: Arc<AtomicUsize>,
}

impl QuitSignal {
    pub fn request(&self) -> Result<(), PlatformError> {
        let target = self.target.load(Ordering::Acquire);
        if target == 0 {
            return Ok(());
        }
        // SAFETY: the owner keeps this target alive until worker completion. Posted
        // messages are thread-safe, contain no pointers, and execute in its wndproc.
        unsafe {
            PostMessageW(
                Some(HWND(target as *mut std::ffi::c_void)),
                WM_REQUEST_QUIT,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .map_err(|error| PlatformError::new("quit_post_failed", error.to_string()))
    }
}

unsafe extern "system" fn quit_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    std::panic::catch_unwind(|| {
        if message == WM_REQUEST_QUIT {
            // PostQuitMessage on the owning thread is recognized by system modal loops.
            post_quit();
            LRESULT(0)
        } else {
            // SAFETY: unchanged arguments supplied to this window by Windows.
            unsafe { DefWindowProcW(hwnd, message, w, l) }
        }
    })
    .unwrap_or_else(|_| {
        post_quit();
        LRESULT(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, SetTimer, WM_TIMER,
    };

    #[test]
    fn quit_request_is_dispatched_inside_a_modal_loop_and_propagates_outward() {
        let thread = std::thread::spawn(|| {
            let window = QuitWindow::new().unwrap();
            let signal = window.signal();
            // SAFETY: a timer on this owned window bounds this regression even if
            // shutdown delivery is broken. DestroyWindow removes the timer.
            let timer = unsafe { SetTimer(Some(window.window.hwnd()), 1, 2000, None) };
            assert_ne!(timer, 0);
            let sender = std::thread::spawn(move || signal.request().unwrap());
            let received_quit = loop {
                let mut message = MSG::default();
                // SAFETY: live MSG; this thread owns the queue and both windows.
                let got = unsafe { GetMessageW(&mut message, None, 0, 0) };
                assert_ne!(got.0, -1);
                if got.0 == 0 {
                    break true;
                }
                if message.message == WM_TIMER {
                    break false;
                }
                // Deliberately no custom channel drain, like a system modal loop.
                // SAFETY: unmodified message returned for this thread's window.
                unsafe {
                    DispatchMessageW(&message);
                }
            };
            sender.join().unwrap();
            assert!(
                received_quit,
                "private quit request was not delivered inside modal dispatch"
            );
            post_quit(); // Modal loops re-post WM_QUIT for their caller.
            crate::win_util::pump_messages(|| crate::win_util::PumpVerdict::Continue).unwrap();
            let signal = window.signal();
            drop(window);
            signal.request().unwrap(); // Revoked signal is harmless.
        });
        thread.join().unwrap();
    }
}
