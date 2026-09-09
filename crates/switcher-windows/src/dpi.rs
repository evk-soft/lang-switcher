//! Per-Monitor-V2 DPI awareness: first a diagnosis, then a fallback (ADR-0010).
//!
//! Every screen coordinate this crate produces is claimed to be a physical pixel. That
//! claim is only true while the process is Per-Monitor-V2 aware; otherwise DPI
//! virtualization quietly rewrites `GetCursorPos`, monitor work areas and window
//! positions, and the badge is misplaced on every scale other than 100%. The manifest
//! embedded by `switcher-app`'s build script is what normally provides it.
//!
//! The order here matters and is not stylistic: the awareness is **read first**, so the
//! log line can distinguish "the manifest worked" from "we had to set it at runtime". If
//! we set it first, the subsequent read would always succeed and the warning would stop
//! being a detector of a missing manifest.

use std::sync::OnceLock;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetAwarenessFromDpiAwarenessContext,
    GetDpiAwarenessContextForProcess, SetProcessDpiAwarenessContext,
};

/// What the process' DPI awareness looked like when `ensure_per_monitor_v2` first ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiAwarenessReport {
    /// The process was already Per-Monitor-V2 before we touched anything — i.e. the
    /// manifest is in the binary and took effect. This is the only proof of that fact.
    pub from_manifest: bool,
    /// The process is Per-Monitor-V2 now, whether from the manifest or from our fallback.
    pub per_monitor_v2: bool,
}

static REPORT: OnceLock<DpiAwarenessReport> = OnceLock::new();

/// Makes sure the process is Per-Monitor-V2 aware and reports how that came about.
///
/// Idempotent: the work happens once and later calls return the same report, so `main`
/// and every example may call it unconditionally.
///
/// MUST run before any `HWND` exists in the process. Microsoft documents the mode as
/// unchangeable afterwards: "Once a window (an HWND) has been created in your process,
/// changing the DPI awareness mode is no longer supported." In practice that means before
/// the tray is built and before any overlay window is created.
pub fn ensure_per_monitor_v2() -> DpiAwarenessReport {
    *REPORT.get_or_init(|| {
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle to the current process. It
        // is not a real kernel handle, is always valid for the life of the process, and
        // must not be closed — so there is nothing to release here.
        let process: HANDLE = unsafe { GetCurrentProcess() };

        // SAFETY: reads the calling process' DPI awareness context. The only precondition
        // is a valid process handle, which the pseudo-handle above satisfies. No
        // ownership is transferred: the returned context is an opaque sentinel value, not
        // an allocation, so there is nothing to free.
        let current = unsafe { GetDpiAwarenessContextForProcess(process) };
        let from_manifest = contexts_equal(current, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        if from_manifest {
            tracing::debug!(
                target: "switcher_windows::dpi",
                "dpi awareness from manifest: per-monitor-v2"
            );
            return DpiAwarenessReport {
                from_manifest: true,
                per_monitor_v2: true,
            };
        }

        // SAFETY: sets the process-wide DPI awareness. The documented precondition is
        // that no HWND has been created in this process yet; that is this function's own
        // contract (see the doc comment), enforced by call-site discipline rather than by
        // the compiler. Failure is expected and handled, not undefined behaviour.
        let applied =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

        // SAFETY: same as the read above — an opaque read of process state.
        let awareness = unsafe { GetAwarenessFromDpiAwarenessContext(current) };
        tracing::warn!(
            target: "switcher_windows::dpi",
            awareness = ?awareness,
            applied = ?applied,
            "process is NOT Per-Monitor-V2 from a manifest; setting it at runtime"
        );

        DpiAwarenessReport {
            from_manifest: false,
            per_monitor_v2: applied.is_ok(),
        }
    })
}

/// `DPI_AWARENESS` only distinguishes unaware / system / per-monitor, so V1 and V2 are
/// indistinguishable through it. Identifying V2 requires this comparison, and the
/// contexts are opaque sentinels — comparing the raw pointers is not equivalent.
fn contexts_equal(a: DPI_AWARENESS_CONTEXT, b: DPI_AWARENESS_CONTEXT) -> bool {
    // SAFETY: both arguments are DPI awareness context values obtained from this API
    // family (one read from the process, one a library constant); the call only compares
    // them and has no other precondition.
    unsafe { AreDpiAwarenessContextsEqual(a, b) }.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only memoization is asserted, and that is deliberate. Whether the runtime fallback
    /// *succeeds* depends on whether any test in this binary created a window first —
    /// libtest runs tests in parallel threads inside one process, and the awareness mode
    /// is process-wide and unchangeable once an HWND exists. Asserting
    /// `per_monitor_v2 == true` here would pass or fail depending on test scheduling,
    /// which is worse than not asserting it. The real check for the manifest is the
    /// negative control in the smoke checklist, where the process is the actual binary.
    #[test]
    fn ensure_per_monitor_v2_is_memoized() {
        let first = ensure_per_monitor_v2();
        let second = ensure_per_monitor_v2();
        assert_eq!(first, second, "the report must be memoized, not recomputed");
    }
}
