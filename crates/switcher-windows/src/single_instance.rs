//! One running copy per user session, and the handshake the installer uses.
//!
//! Two separate problems share one named mutex:
//!
//! 1. A second tray icon indicating the same layout is confusing, and two overlays fight
//!    over the same anchor.
//! 2. Setup and Uninstall must not replace or delete a running executable. Inno Setup's
//!    [`AppMutex`](https://jrsoftware.org/ishelp/topic_setup_appmutex.htm) checks for this
//!    exact name and asks the user to close the application instead.
//!
//! The name is unprefixed, so it lives in the session namespace. That is the right scope
//! for a per-user installation: fast user switching gives each session its own copy, and
//! the installer runs as the same user in the same session.
//!
//! [`CreateMutexW`](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw)
//! returns a handle to the existing object with `ERROR_ALREADY_EXISTS` when the name is
//! taken, so one call both creates and tests.

use switcher_platform::ports::PlatformError;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::Owned;

/// Must match `AppMutex` in `packaging/windows/lang-switcher.iss` exactly — Windows
/// compares kernel object names case-sensitively.
pub const APP_MUTEX_NAME: &str = "lang-switcher-single-instance";

/// Holds the session's claim. Dropping it releases the name; the OS also does that if the
/// process dies, which is why nothing here tries to clean up after a crash.
#[derive(Debug)]
pub struct SingleInstance {
    _handle: Owned<HANDLE>,
}

/// Claims `name` for this process.
///
/// `Ok(None)` means another copy already holds it — not an error, and not a reason to
/// report a lost capability: the correct response is to exit quietly.
pub fn acquire(name: &str) -> Result<Option<SingleInstance>, PlatformError> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the call. Null
    // security attributes give the mutex the creator's default descriptor, which is what
    // a per-user object wants. `false` for the initial owner is what Microsoft documents
    // for the create-or-open pattern: ownership is never taken, only the name matters.
    let handle = unsafe { CreateMutexW(None, false, windows::core::PCWSTR(wide.as_ptr())) }
        .map_err(|error| PlatformError::new("single_instance_unavailable", error.message()))?;
    // SAFETY: read immediately after the call above, on the same thread, with no
    // intervening Win32 call that could overwrite the thread's last-error value. The
    // binding only consults it on failure, so it still carries the creation result.
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    // SAFETY: `handle` is a live mutex handle that this call succeeded in producing and
    // that nothing else owns; `Owned` closes it exactly once, in `Drop`.
    let owned = unsafe { Owned::new(handle) };
    if existed {
        // Dropping our extra handle here is correct: the first copy still holds its own,
        // so the name stays claimed.
        drop(owned);
        return Ok(None);
    }
    Ok(Some(SingleInstance { _handle: owned }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique name per test run: a leftover from a crashed earlier run, or a real
    /// lang-switcher running on the developer's machine, would otherwise decide the result.
    fn unique(label: &str) -> String {
        format!(
            "lang-switcher-test-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )
    }

    #[test]
    fn the_first_claim_wins_and_the_second_is_refused() {
        let name = unique("first");
        let first = acquire(&name).unwrap().expect("the first claim succeeds");
        assert!(
            acquire(&name).unwrap().is_none(),
            "a second copy must be refused while the first holds the name"
        );
        drop(first);
        assert!(
            acquire(&name).unwrap().is_some(),
            "the name must be reusable once the holder is gone"
        );
    }

    #[test]
    fn different_names_do_not_collide() {
        let a = acquire(&unique("a")).unwrap();
        let b = acquire(&unique("b")).unwrap();
        assert!(a.is_some() && b.is_some());
    }

    /// The installer's `AppMutex` is a literal string in a `.iss` file; nothing but this
    /// test connects it to the constant the application actually claims.
    #[test]
    fn the_shipped_name_matches_the_installer_script() {
        let iss = include_str!("../../../packaging/windows/lang-switcher.iss");
        assert!(
            iss.contains(&format!("#define AppMutexName \"{APP_MUTEX_NAME}\"")),
            "packaging/windows/lang-switcher.iss must define AppMutexName as {APP_MUTEX_NAME:?}"
        );
        assert!(
            iss.contains("AppMutex={#AppMutexName}"),
            "the [Setup] section must actually use AppMutexName"
        );
        assert!(
            !APP_MUTEX_NAME.contains('\\'),
            "a kernel object name may not contain a backslash outside a namespace prefix"
        );
    }
}
