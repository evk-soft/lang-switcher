//! The user's Windows display language preference (ADR-0022).
//!
//! Reads `GetUserPreferredUILanguages`, which answers "what language should the interface
//! be in" — not `GetKeyboardLayout`, which answers "what is being typed", and not the
//! regional format settings.
//!
//! [Microsoft contract](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-getuserpreferreduilanguages):
//! two calls. The first passes a null buffer and a zero size and receives the required
//! size, including the two terminating nulls. The second fills an ordered,
//! null-delimited list that ends with two nulls.

use switcher_platform::ports::{PlatformError, UiLanguages};
use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// A pathological answer must not turn into a multi-megabyte allocation. Windows returns
/// at most a handful of installed display languages; 4096 UTF-16 units is far above any
/// real list and still bounded.
const MAX_BUFFER_CHARS: u32 = 4096;

/// Splits the double-null-terminated multi-string Windows filled in.
///
/// Driven by the buffer's own null delimiters rather than by `pulNumLanguages`: the count
/// and the buffer are two separate out-parameters, and trusting the count to bound a walk
/// through the buffer would put an OS-supplied number in charge of how far we read.
fn split_multi_string(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

fn failed(context: &str, error: windows::core::Error) -> PlatformError {
    PlatformError::new(
        "ui_language_query_failed",
        format!("{context}: {}", error.message()),
    )
}

/// Reads the display language list on the calling thread. Holds no handle and no state,
/// so it is re-read rather than cached: the user can change the display language while
/// the application is running.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemUiLanguages;

impl UiLanguages for SystemUiLanguages {
    fn preferred(&self) -> Result<Vec<String>, PlatformError> {
        let mut count: u32 = 0;
        let mut chars: u32 = 0;
        // SAFETY: the documented sizing call. `pwszLanguagesBuffer` is `None` (a null
        // buffer) exactly when `pcchLanguagesBuffer` points at a zero, which is the
        // combination Microsoft specifies for "tell me the required size"; nothing is
        // written through the null buffer. `count` and `chars` are live locals for the
        // duration of the call, so both out-pointers are valid and aligned.
        unsafe {
            GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut chars)
                .map_err(|error| failed("could not size the display language list", error))?;
        }
        // The required size counts both terminating nulls, so a well-formed empty list is
        // 1 or 2. Nothing to split, and no reason to make a second call.
        if chars <= 2 {
            return Ok(Vec::new());
        }
        if chars > MAX_BUFFER_CHARS {
            return Err(PlatformError::new(
                "ui_language_query_failed",
                format!("display language list of {chars} characters is implausible"),
            ));
        }
        let mut buffer = vec![0u16; chars as usize];
        // SAFETY: `buffer` holds exactly `chars` UTF-16 units, which is the size Windows
        // just reported as required, and `chars` is passed unchanged as the in/out size —
        // so the callee cannot write past the allocation. The pointer comes from a
        // `Vec<u16>` that outlives the call and is not aliased elsewhere. `count` and
        // `chars` are live locals. On failure the buffer keeps its zero initialization,
        // and it is dropped without being read.
        unsafe {
            GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut chars,
            )
            .map_err(|error| failed("could not read the display language list", error))?;
        }
        // Windows may report back a shorter used length than it asked us to allocate.
        let used = (chars as usize).min(buffer.len());
        let languages = split_multi_string(&buffer[..used]);
        tracing::debug!(?languages, count, "windows display language preference");
        Ok(languages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_string_splits_on_nulls_and_drops_the_terminator() {
        let buffer: Vec<u16> = "en-US\0ru-RU\0\0".encode_utf16().collect();
        assert_eq!(split_multi_string(&buffer), ["en-US", "ru-RU"]);
    }

    #[test]
    fn empty_and_unterminated_buffers_do_not_produce_empty_entries() {
        assert!(split_multi_string(&[]).is_empty());
        assert!(split_multi_string(&[0, 0]).is_empty());
        let unterminated: Vec<u16> = "de-DE".encode_utf16().collect();
        assert_eq!(split_multi_string(&unterminated), ["de-DE"]);
    }

    /// The real call against the machine running the tests. The list is whatever this
    /// Windows is configured with, so the assertions are about shape, not content.
    #[test]
    fn windows_returns_parsable_language_tags() {
        let languages = SystemUiLanguages.preferred().expect("display languages");
        for tag in &languages {
            assert!(!tag.is_empty());
            assert!(
                !tag.contains('\0'),
                "a null must never survive into a tag: {tag:?}"
            );
            assert!(
                tag.starts_with(|c: char| c.is_ascii_alphabetic()),
                "a display language must start with a language subtag: {tag:?}"
            );
        }
    }
}
