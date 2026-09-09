//! Read-only, bounded comparison of foreground Win32 HKL and addressed TSF profiles.
//! An independent watchdog ends this diagnostic process if a COM call blocks shutdown.
//! Microsoft marks GetInputProcessorProfiles "Should not be used": this is only an
//! investigation tool, not a supported production layout authority or API recommendation.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use switcher_windows::com::StaApartment;
use windows::{
    Win32::{
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            Threading::{GetCurrentProcess, GetCurrentThreadId, TerminateProcess},
        },
        UI::{
            Input::KeyboardAndMouse::GetKeyboardLayout,
            TextServices::{
                CLSID_TF_InputProcessorProfiles, CLSID_TF_LangBarMgr, GUID_TFCAT_TIP_KEYBOARD,
                ITfInputProcessorProfileMgr, ITfInputProcessorProfiles, ITfLangBarMgr,
                TF_INPUTPROCESSORPROFILE,
            },
            WindowsAndMessaging::{
                DispatchMessageW, GUITHREADINFO, GetClassNameW, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowThreadProcessId, MSG, PM_REMOVE, PeekMessageW,
                TranslateMessage, WM_QUIT,
            },
        },
    },
    core::Interface,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let seconds = args.next().unwrap_or("45".into()).parse::<u64>()?;
    let mut watchdog_test = false;
    let stop_file = match args.next().as_deref() {
        None => None,
        Some("--watchdog-test") => {
            watchdog_test = true;
            None
        }
        Some("--stop-file") => Some(PathBuf::from(args.next().ok_or("missing stop-file path")?)),
        Some(_) => return Err("unknown argument".into()),
    };
    if !(1..=120).contains(&seconds) || args.next().is_some() {
        return Err(
            "usage: layout_tsf_probe [1..120 seconds [--stop-file PATH | --watchdog-test]]".into(),
        );
    }
    // Deliberately detached and diagnostic-only: main's normal return ends the process.
    // If native COM or teardown blocks main, this thread terminates only our process
    // with an explicit incomplete-result code. It does not wait on stdout or a PS prompt.
    std::thread::Builder::new()
        .name("probe-watchdog".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds + 5));
            // SAFETY: terminate only this diagnostic's own process via its pseudo-handle.
            // Unlike ExitProcess, this avoids running DLL teardown on a blocked COM call.
            // No external process ID/handle is accepted; normal shutdown never reaches here.
            unsafe {
                let _ = TerminateProcess(GetCurrentProcess(), 124);
            }
            std::process::abort(); // Self-termination normally does not return.
        })?;
    if watchdog_test {
        // Exercise the hard deadline without opening COM or touching another process.
        loop {
            std::thread::park();
        }
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let _sta = StaApartment::new()?;
    // SAFETY: owning STA outlives these in-process interfaces; no aggregation.
    let bar: ITfLangBarMgr =
        unsafe { CoCreateInstance(&CLSID_TF_LangBarMgr, None, CLSCTX_INPROC_SERVER) }?;
    // SAFETY: same live STA; this is intentionally only a local comparison.
    let local: ITfInputProcessorProfiles =
        unsafe { CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER) }?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if stop_file.as_ref().is_some_and(|path| path.exists()) {
            break;
        }
        let mut message = MSG::default();
        // SAFETY: current STA queue, initialized output; bounded to preserve the deadline.
        unsafe {
            for _ in 0..128 {
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
                if message.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        // SAFETY: read-only HWND query; null/zero identities are rejected below.
        let hwnd = unsafe { GetForegroundWindow() };
        let mut pid = 0;
        let mut class = [0u16; 256];
        // SAFETY: borrowed HWND and initialized writable PID/class output.
        let (tid, class_len) = unsafe {
            (
                GetWindowThreadProcessId(hwnd, Some(&mut pid)),
                GetClassNameW(hwnd, &mut class),
            )
        };
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: initialized GUI struct with required size; zero queries foreground input.
        let focus_result = unsafe { GetGUIThreadInfo(0, &mut gui) };
        let focus_tid = if focus_result.is_ok() && !gui.hwndFocus.is_invalid() {
            // SAFETY: read-only borrowed focus HWND; zero is handled as unavailable.
            unsafe { GetWindowThreadProcessId(gui.hwndFocus, None) }
        } else {
            0
        };
        if !hwnd.is_invalid() && tid != 0 && focus_tid != 0 {
            let mut profiles = None;
            let mut returned_tid = 0;
            // SAFETY: nonzero focus TID and initialized outputs; returned proxy stays on
            // this STA. It is never replaced by a local manager when unavailable.
            let status = unsafe {
                bar.GetInputProcessorProfiles(focus_tid, &mut profiles, &mut returned_tid)
            };
            // SAFETY: all TIDs are nonzero; own-thread HKL is only a labeled comparison.
            let (hkl, focus_hkl, own_tid, own_hkl, local_language) = unsafe {
                (
                    GetKeyboardLayout(tid),
                    GetKeyboardLayout(focus_tid),
                    GetCurrentThreadId(),
                    GetKeyboardLayout(GetCurrentThreadId()),
                    local.GetCurrentLanguage(),
                )
            };
            let language = profiles.as_ref().map(|p| {
                // SAFETY: returned proxy remains on this STA, queried read-only.
                unsafe { p.GetCurrentLanguage() }
            });
            let active = profiles.as_ref().map(|p| -> windows::core::Result<_> {
                let manager: ITfInputProcessorProfileMgr = p.cast()?;
                let mut value = TF_INPUTPROCESSORPROFILE::default();
                // SAFETY: initialized output and supported category. Preserve S_FALSE:
                // generated Result<()> would collapse absence into success.
                let result = unsafe {
                    (Interface::vtable(&manager).GetActiveProfile)(
                        Interface::as_raw(&manager),
                        &GUID_TFCAT_TIP_KEYBOARD,
                        &mut value,
                    )
                };
                result.ok()?;
                Ok((
                    result.0,
                    value.dwProfileType,
                    value.langid,
                    value.hkl,
                    value.guidProfile,
                ))
            });
            let mut after = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: repeated read-only identity check; no atomic/ABA guarantee is claimed.
            let stable = unsafe {
                GetGUIThreadInfo(0, &mut after).is_ok()
                    && GetForegroundWindow() == hwnd
                    && GetWindowThreadProcessId(hwnd, None) == tid
                    && after.hwndFocus == gui.hwndFocus
                    && GetWindowThreadProcessId(after.hwndFocus, None) == focus_tid
            };
            tracing::info!(
                ?hwnd,
                tid,
                pid,
                class = String::from_utf16_lossy(&class[..class_len.max(0) as usize]),
                focus_tid,
                returned_tid,
                stable,
                ?hkl,
                ?focus_hkl,
                ?status,
                ?language,
                ?active,
                own_tid,
                ?own_hkl,
                ?local_language,
                "layout state comparison"
            );
        } else {
            tracing::info!(?hwnd, tid, focus_tid, focus_error = ?focus_result.err(), "input identity unavailable");
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(200)),
        );
    }
    Ok(())
}
