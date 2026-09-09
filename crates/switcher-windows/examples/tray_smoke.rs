//! Native regression for the pinned tray-icon hover opt-out (ADR-0017).
//! Creates temporary icons; sends only to our own HWND, never moves cursor/focus.
//! Explorer must expose their bounds: open the hidden-icons flyout if necessary.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, SendMessageW, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MOUSEMOVE, WM_TIMER,
        },
    };
    // Test-only protocol from the pinned vendor source, never used by the application.
    const TRAY_CALLBACK: u32 = 6002;
    for hover in [true, false] {
        let tray = TrayIconBuilder::new()
            .with_tooltip("lang-switcher tray regression")
            .with_icon(Icon::from_rgba(vec![255; 16 * 16 * 4], 16, 16)?)
            .with_hover_tracking(hover)
            .build()?;
        let hwnd = HWND(tray.window_handle());
        let registered = Instant::now() + Duration::from_secs(2);
        while tray.rect().is_none() && Instant::now() < registered {
            let mut message = MSG::default();
            // SAFETY: output MSG and native callbacks belong to the current thread.
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            tray.rect().is_some(),
            "requires the interactive Explorer tray"
        );
        for _ in TrayIconEvent::receiver().try_iter() {}
        for notification in [WM_MOUSEMOVE, WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP] {
            // SAFETY: our live same-thread tray window, integer-only pinned callback protocol.
            unsafe {
                SendMessageW(
                    hwnd,
                    TRAY_CALLBACK,
                    Some(WPARAM(0)),
                    Some(LPARAM(notification as isize)),
                )
            };
        }
        let until = Instant::now() + Duration::from_millis(150);
        let mut timers = 0;
        while Instant::now() < until {
            let mut message = MSG::default();
            // SAFETY: valid output MSG, only this thread's messages are removed.
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                if message.hwnd == hwnd && message.message == WM_TIMER {
                    timers += 1;
                }
                // SAFETY: unchanged OS message; live tray-owned callbacks on the creating thread.
                unsafe { DispatchMessageW(&message) };
            }
            std::thread::sleep(Duration::from_millis(5)); // Bounded diagnostic harness only.
        }
        let mut hover_events = 0;
        let mut clicks = 0;
        for event in TrayIconEvent::receiver()
            .try_iter()
            .filter(|e| e.id() == tray.id())
        {
            match event {
                TrayIconEvent::Enter { .. }
                | TrayIconEvent::Move { .. }
                | TrayIconEvent::Leave { .. } => hover_events += 1,
                TrayIconEvent::Click { .. } => clicks += 1,
                _ => {}
            }
        }
        println!("hover={hover} timers={timers} hover_events={hover_events} clicks={clicks}");
        assert!(clicks >= 2, "click handling was lost");
        if hover {
            assert!(
                timers > 0 && hover_events > 0,
                "control did not exercise the native hover path"
            );
        } else {
            assert_eq!(timers, 0, "disabled hover still arms a native timer");
            assert_eq!(hover_events, 0, "disabled hover still sends events");
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
