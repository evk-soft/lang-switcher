//! HKCU Run adapter. Results stay in the port; the runtime applies the two-phase
//! autostart acknowledgement and publishes capabilities (ADR-0007).

use crate::win_util::wide;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use switcher_platform::ports::{Autostart, PlatformError};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::core::{Owned, PCWSTR, w};

fn run_value_bytes(exe: &Path) -> Result<Vec<u8>, PlatformError> {
    let path: Vec<u16> = exe.as_os_str().encode_wide().collect();
    if path.is_empty() || path.iter().any(|&c| c == 0 || c == u16::from(b'"')) {
        return Err(PlatformError::new(
            "exe_path_unavailable",
            "executable path cannot be represented as a quoted Run command",
        ));
    }
    // Microsoft limits a Run command to 260 characters, including our quotes.
    if path.len() + 2 > 260 {
        return Err(PlatformError::new(
            "exe_path_unavailable",
            "Run command exceeds the Windows 260-character limit",
        ));
    }
    Ok(std::iter::once(u16::from(b'"'))
        .chain(path)
        .chain([u16::from(b'"'), 0])
        .flat_map(u16::to_le_bytes)
        .collect())
}

fn code_for(error: WIN32_ERROR) -> &'static str {
    match error {
        ERROR_ACCESS_DENIED => "registry_write_denied",
        ERROR_FILE_NOT_FOUND => "registry_value_missing",
        _ => "registry_error",
    }
}

fn registry_error(error: WIN32_ERROR) -> PlatformError {
    PlatformError::new(code_for(error), windows::core::Error::from(error).message())
}

#[derive(Debug)]
pub struct RegistryAutostart {
    subkey: Vec<u16>,
}

impl Default for RegistryAutostart {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryAutostart {
    pub fn new() -> Self {
        Self {
            subkey: wide(r"Software\Microsoft\Windows\CurrentVersion\Run"),
        }
    }

    fn open(&self, access: REG_SAM_FLAGS) -> Result<Option<Owned<HKEY>>, PlatformError> {
        let mut key = HKEY::default();
        // SAFETY: predefined borrowed HKCU, terminated owned subkey and live output.
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(self.subkey.as_ptr()),
                None,
                access,
                &mut key,
            )
        };
        match result {
            ERROR_FILE_NOT_FOUND => Ok(None),
            ERROR_SUCCESS => {
                // SAFETY: successful open transferred one key; Owned closes it once.
                Ok(Some(unsafe { Owned::new(key) }))
            }
            error => Err(registry_error(error)),
        }
    }
}

/// No channel or optimistic config write here: runtime acknowledges the actual
/// registry result and then re-reads OS truth for the tray/config (ADR-0007).
impl Autostart for RegistryAutostart {
    fn is_enabled(&self) -> Result<bool, PlatformError> {
        let Some(key) = self.open(KEY_QUERY_VALUE)? else {
            return Ok(false);
        };
        let mut size = 0;
        let mut kind = REG_VALUE_TYPE::default();
        // SAFETY: key has query permission; static value name and live metadata
        // outputs. NULL data with size output asks for metadata without reading bytes.
        let result = unsafe {
            RegQueryValueExW(
                *key,
                w!("lang-switcher"),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        match result {
            ERROR_FILE_NOT_FOUND => Ok(false),
            ERROR_SUCCESS => {
                // Presence is the M1 contract, independent of a moved executable.
                // Optional diagnostics never change the truth returned by the first read.
                if tracing::enabled!(tracing::Level::DEBUG) && kind == REG_SZ && size <= 4096 {
                    let mut bytes = vec![0u8; size as usize];
                    // SAFETY: aligned byte buffer of `size` bytes, held across the call;
                    // a concurrent value growth returns ERROR_MORE_DATA and is not decoded.
                    let read = unsafe {
                        RegQueryValueExW(
                            *key,
                            w!("lang-switcher"),
                            None,
                            Some(&mut kind),
                            Some(bytes.as_mut_ptr()),
                            Some(&mut size),
                        )
                    };
                    if read == ERROR_SUCCESS && kind == REG_SZ {
                        // `as_chunks` rather than `chunks_exact(2)`: the pair is a
                        // fixed-size array, so the element accesses below need no bounds
                        // checks. A trailing odd byte is discarded either way.
                        let units: Vec<u16> = bytes[..size as usize]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| u16::from_le_bytes(*b))
                            .take_while(|&c| c != 0)
                            .collect();
                        tracing::debug!(
                            command = String::from_utf16_lossy(&units),
                            "existing autostart entry"
                        );
                    }
                }
                Ok(true)
            }
            error => Err(registry_error(error)),
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        if !enabled {
            let Some(key) = self.open(KEY_SET_VALUE)? else {
                return Ok(());
            };
            // SAFETY: key opened for set/delete; only our exact named value is removed.
            let result = unsafe { RegDeleteValueW(*key, w!("lang-switcher")) };
            return match result {
                ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
                error => Err(registry_error(error)),
            };
        }
        let exe = std::env::current_exe()
            .map_err(|error| PlatformError::new("exe_path_unavailable", error.to_string()))?;
        let bytes = run_value_bytes(&exe)?;
        let mut key = HKEY::default();
        // SAFETY: terminated subkey, live output, noninheritable handle with set-value
        // permission. No privileged backup/restore flags and no custom security descriptor.
        let result = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(self.subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(registry_error(result));
        }
        // SAFETY: key is the newly owned handle from successful RegCreateKeyExW.
        let key = unsafe { Owned::new(key) };
        // SAFETY: valid set-value handle, static name, terminated UTF-16LE bytes. The
        // generated slice binding supplies byte length including the string terminator.
        let result =
            unsafe { RegSetValueExW(*key, w!("lang-switcher"), None, REG_SZ, Some(&bytes)) };
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(registry_error(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND};

    #[test]
    fn run_value_is_quoted_utf16le_with_terminator() {
        let bytes = run_value_bytes(Path::new(r"C:\Program Files\ls\lang-switcher.exe")).unwrap();
        let text: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        assert_eq!(
            String::from_utf16(&text[..text.len() - 1]).unwrap(),
            "\"C:\\Program Files\\ls\\lang-switcher.exe\""
        );
        assert_eq!(text.last(), Some(&0));
    }

    #[test]
    fn run_command_preserves_non_unicode_paths_and_rejects_unusable_commands() {
        let path = OsString::from_wide(&[67, 58, 92, 0xd800, 46, 101, 120, 101]);
        let bytes = run_value_bytes(Path::new(&path)).unwrap();
        assert!(bytes.windows(2).any(|b| b == [0, 0xd8]));
        assert!(run_value_bytes(Path::new("bad\0path")).is_err());
        assert!(run_value_bytes(Path::new("bad\"path")).is_err());
        assert!(
            run_value_bytes(Path::new(&"x".repeat(259))).is_err(),
            "Run command limit includes quotes"
        );
    }

    #[test]
    fn registry_errors_map_to_stable_codes() {
        assert_eq!(code_for(ERROR_ACCESS_DENIED), "registry_write_denied");
        assert_eq!(code_for(ERROR_FILE_NOT_FOUND), "registry_value_missing");
        assert_eq!(code_for(WIN32_ERROR(1234)), "registry_error");
    }

    #[test]
    fn native_registry_roundtrip_uses_an_isolated_non_autostart_key() {
        use windows::Win32::System::Registry::RegDeleteKeyW;
        struct Cleanup(Vec<u16>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                // SAFETY: exact unique test key below HKCU\Software, confirmed absent
                // before creation. Nonrecursive removal never touches the real Run key.
                let result = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(self.0.as_ptr())) };
                if result != ERROR_SUCCESS && result != ERROR_FILE_NOT_FOUND {
                    tracing::error!(?result, "temporary registry key cleanup failed");
                }
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let adapter = RegistryAutostart {
            subkey: wide(&format!(
                r"Software\LangSwitcherAutostartTest-{}-{nonce}",
                std::process::id()
            )),
        };
        assert!(
            adapter.open(KEY_QUERY_VALUE).unwrap().is_none(),
            "test key must not preexist"
        );
        let _cleanup = Cleanup(adapter.subkey.clone());
        assert!(!adapter.is_enabled().unwrap());
        adapter.set_enabled(false).unwrap();
        adapter.set_enabled(true).unwrap();
        adapter.set_enabled(true).unwrap();
        assert!(adapter.is_enabled().unwrap());
        let key = adapter.open(KEY_QUERY_VALUE).unwrap().unwrap();
        let mut bytes = [0u8; 1024];
        let mut size = bytes.len() as u32;
        let mut kind = REG_VALUE_TYPE::default();
        // SAFETY: live query handle and writable buffer with exact capacity in bytes.
        let result = unsafe {
            RegQueryValueExW(
                *key,
                w!("lang-switcher"),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
        };
        assert_eq!(result, ERROR_SUCCESS);
        assert_eq!(kind, REG_SZ);
        assert_eq!(
            &bytes[..size as usize],
            run_value_bytes(&std::env::current_exe().unwrap()).unwrap()
        );
        drop(key);
        adapter.set_enabled(false).unwrap();
        adapter.set_enabled(false).unwrap();
        assert!(!adapter.is_enabled().unwrap());
        drop(_cleanup);
        assert!(
            adapter.open(KEY_QUERY_VALUE).unwrap().is_none(),
            "temporary key was removed"
        );
    }
}
