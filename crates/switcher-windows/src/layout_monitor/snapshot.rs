//! Re-read the foreground at notification consumption time, never trust queued HKLs.

use switcher_platform::events::{LangTag, LayoutId};
use switcher_platform::ports::PlatformError;
use windows::Win32::Globalization::LCIDToLocaleName;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Foreground {
    hwnd: usize,
    tid: u32,
}

fn read_stable(
    mut foreground: impl FnMut() -> Option<Foreground>,
    mut layout: impl FnMut(u32) -> Option<LayoutId>,
) -> Result<LayoutId, PlatformError> {
    for _ in 0..3 {
        let before = foreground().ok_or_else(|| {
            PlatformError::new(
                "foreground_unavailable",
                "no foreground thread is available",
            )
        })?;
        let value = layout(before.tid).ok_or_else(|| {
            PlatformError::new("layout_read_failed", "GetKeyboardLayout returned NULL")
        })?;
        if foreground() == Some(before) {
            return Ok(value);
        }
    }
    Err(PlatformError::new(
        "foreground_raced",
        "foreground changed during three consecutive reads",
    ))
}

pub fn current() -> Result<(LayoutId, LangTag), PlatformError> {
    read_expected(None)
}

pub(super) fn current_if_foreground(
    hwnd: usize,
    tid: u32,
) -> Result<(LayoutId, LangTag), PlatformError> {
    read_expected(Some(Foreground { hwnd, tid }))
}

fn read_expected(expected: Option<Foreground>) -> Result<(LayoutId, LangTag), PlatformError> {
    let layout = read_matching(
        expected,
        || {
            // SAFETY: read-only desktop queries with no retained HWND. A vanished
            // foreground returns None so TID zero never selects our own layout.
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd.is_invalid() {
                return None;
            }
            // SAFETY: borrowed HWND is queried without dereferencing it; zero is checked.
            let tid = unsafe { GetWindowThreadProcessId(hwnd, None) };
            let observed = Foreground {
                hwnd: hwnd.0 as usize,
                tid,
            };
            (tid != 0).then_some(observed)
        },
        |tid| {
            // SAFETY: nonzero foreground TID from the preceding observation. No layout
            // handle is owned; it is an opaque identifier, never dereferenced or closed.
            let hkl = unsafe { GetKeyboardLayout(tid) };
            (!hkl.is_invalid()).then_some(LayoutId(hkl.0 as usize as u64))
        },
    )?;
    Ok((layout, language_for(layout)))
}

fn read_matching(
    expected: Option<Foreground>,
    mut foreground: impl FnMut() -> Option<Foreground>,
    layout: impl FnMut(u32) -> Option<LayoutId>,
) -> Result<LayoutId, PlatformError> {
    read_stable(
        || foreground().filter(|observed| expected.is_none_or(|value| value == *observed)),
        layout,
    )
}

pub(super) fn language_for(layout: LayoutId) -> LangTag {
    // GetKeyboardLayout documents the low word as LANGID. An LCID with sort ID
    // zero has exactly these low 16 bits (MAKELCID(langid, SORT_DEFAULT)).
    let langid = (layout.0 & 0xffff) as u32;
    // LOCALE_NAME_MAX_LENGTH is 85 in Win32/System/SystemServices; avoid enabling
    // an otherwise unused feature for that single SDK constant (ADR-0008).
    let mut name = [0u16; 85];
    // SAFETY: aligned writable UTF-16 buffer; generated binding supplies its length.
    let length = unsafe { LCIDToLocaleName(langid, Some(&mut name), 0) };
    if length <= 1 {
        return LangTag::new("");
    }
    LangTag::new(String::from_utf16_lossy(&name[..length as usize - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: Foreground = Foreground { hwnd: 1, tid: 10 };
    const B: Foreground = Foreground { hwnd: 2, tid: 20 };

    #[test]
    fn conditional_poll_never_reads_a_replacement_foreground() {
        let mut observations = [A, B, B].into_iter();
        let mut threads_read = Vec::new();
        let result = read_matching(
            Some(A),
            || observations.next(),
            |tid| {
                threads_read.push(tid);
                Some(LayoutId(tid.into()))
            },
        );
        assert!(result.is_err(), "the classified window has changed");
        assert_eq!(threads_read, vec![10], "never poll the replacement thread");
    }

    #[test]
    fn native_locale_names_describe_the_same_hkl() {
        assert_eq!(language_for(LayoutId(0x4090409)).as_str(), "en-US");
        assert_eq!(language_for(LayoutId(0x4190419)).as_str(), "ru-RU");
    }

    #[test]
    fn stable_snapshot_reads_the_foreground_thread() {
        assert_eq!(
            read_stable(
                || Some(A),
                |tid| {
                    assert_eq!(tid, 10);
                    Some(LayoutId(0x4090409))
                }
            )
            .unwrap(),
            LayoutId(0x4090409)
        );
    }

    #[test]
    fn racing_foreground_is_retried_before_a_layout_is_published() {
        let mut observations = [A, B, B, B].into_iter();
        assert_eq!(
            read_stable(|| observations.next(), |tid| Some(LayoutId(tid.into()))).unwrap(),
            LayoutId(20)
        );
    }

    #[test]
    fn no_foreground_never_falls_back_to_our_thread() {
        let result = read_stable(|| None, |_| panic!("must not read thread zero"));
        assert_eq!(result.unwrap_err().code, "foreground_unavailable");
    }

    #[test]
    fn continual_foreground_changes_have_a_bounded_retry_budget() {
        let mut reads = 0;
        let result = read_stable(
            || {
                reads += 1;
                Some(if reads % 2 == 0 { A } else { B })
            },
            |_| Some(LayoutId(1)),
        );
        assert_eq!(result.unwrap_err().code, "foreground_raced");
        assert_eq!(reads, 6);
    }
}
