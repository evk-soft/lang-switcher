//! Controlled external foreground fixture for app/layout_smoke acceptance.
//! Changes only its own window using already installed HKLs; no keyboard injection.
#[cfg(windows)]
mod native {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use switcher_windows::win_util::register_class;
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::{
                Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList, HKL},
                WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
                    IsWindow, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
                    SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, WINDOW_EX_STYLE,
                    WM_INPUTLANGCHANGEREQUEST, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW,
                    WS_VISIBLE,
                },
            },
        },
        core::w,
    };

    struct Fixture {
        hwnd: HWND,
        previous: HWND,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // SAFETY: only restore focus if it remains on our fixture; never override user
            // navigation. All windows are touched on the creating thread; validity checked.
            unsafe {
                if GetForegroundWindow() == self.hwnd && IsWindow(Some(self.previous)).as_bool() {
                    let _ = SetForegroundWindow(self.previous);
                }
                if IsWindow(Some(self.hwnd)).as_bool() {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
    }

    fn pump() {
        let mut message = MSG::default();
        // SAFETY: current thread's messages, initialized output, system-owned DefWindowProc.
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: count query has no output buffer; the subsequent slice is fully allocated.
        let count = unsafe { GetKeyboardLayoutList(None) };
        if count <= 0 {
            return Err("no installed keyboard layouts".into());
        }
        let mut layouts = vec![HKL::default(); count as usize];
        // SAFETY: writable slice of initialized HKLs, size supplied by the wrapper.
        let count = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) };
        if count <= 0 {
            return Err("cannot enumerate keyboard layouts".into());
        }
        layouts.truncate(count as usize);
        let ru = layouts
            .iter()
            .find(|hkl| hkl.0 as usize & 0x3ff == 0x19)
            .copied();
        let en = layouts
            .iter()
            .find(|hkl| hkl.0 as usize & 0x3ff == 0x09)
            .copied();
        let (Some(ru), Some(en)) = (ru, en) else {
            return Err("install RU and EN before this smoke test".into());
        };
        let module = register_class("LangSwitcherLayoutFixture", None)?;
        // SAFETY: read-only snapshot; may be null and is checked before restoration.
        let previous = unsafe { GetForegroundWindow() };
        // SAFETY: registered default wndproc, static strings, module alive for process lifetime;
        // no parent/menu/creation pointer. The local owner destroys only this new window.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("LangSwitcherLayoutFixture"),
                w!("lang-switcher: click here to start 20 RU/EN checks"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                520,
                140,
                None,
                None,
                Some(module.into()),
                None,
            )
        }?;
        let fixture = Fixture { hwnd, previous };
        // SAFETY: standard EDIT control; static text and a live same-thread parent. Parent
        // destruction owns child cleanup. It provides a real input/focus target for UIA.
        let _edit = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!("RU/EN verification; changing focus stops the test."),
                WS_CHILD | WS_VISIBLE | WS_BORDER,
                10,
                10,
                480,
                40,
                Some(hwnd),
                None,
                Some(module.into()),
                None,
            )
        }?;
        // SAFETY: live owned HWND; OS may reject foreground activation, checked below.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            // A hidden helper launch can override the first call via STARTUPINFO.
            // This fixture explicitly needs its own visible foreground window.
            if !IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        let activate_by = Instant::now() + Duration::from_secs(30);
        loop {
            pump();
            // SAFETY: only reads foreground state. Manual activation is needed when Windows
            // refuses a background process's request; do not repeatedly steal foreground.
            if unsafe { GetForegroundWindow() } == hwnd {
                break;
            }
            if Instant::now() >= activate_by {
                return Err("activate the fixture within 30 seconds to start the test".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        for index in 0..20 {
            // SAFETY: snapshot of current foreground and this thread's active HKL.
            let (foreground, before) = unsafe { (GetForegroundWindow(), GetKeyboardLayout(0)) };
            if foreground != fixture.hwnd {
                return Err(
                    "fixture lost foreground; stopped without changing another window".into(),
                );
            }
            let desired = if before.0 as usize & 0x3ff == 0x19 {
                en
            } else {
                ru
            };
            let epoch_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            let started = Instant::now();
            // SAFETY: posted only to our live window; documented integer-only message accepts
            // a currently installed HKL. DefWindowProc activates it on this window's thread.
            unsafe {
                PostMessageW(
                    Some(hwnd),
                    WM_INPUTLANGCHANGEREQUEST,
                    WPARAM(0),
                    LPARAM(desired.0 as isize),
                )
            }?;
            let mut changed_ms = None;
            while started.elapsed() < Duration::from_millis(600) {
                pump();
                // SAFETY: reads the current thread only; no foreign-thread mutation.
                if changed_ms.is_none() && unsafe { GetKeyboardLayout(0) } == desired {
                    changed_ms = Some(started.elapsed().as_millis());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if changed_ms.is_none() {
                return Err("OS did not accept the fixture's layout request".into());
            }
            println!(
                "index={index} epoch_ms={epoch_ms} hkl={:#x} accepted_ms={}",
                desired.0 as usize,
                changed_ms.unwrap()
            );
        }
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(not(windows))]
fn main() {}
