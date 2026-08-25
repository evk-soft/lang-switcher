//! TODO(task 20): temporary body. Task 9 needs a runnable binary for one reason only —
//! to prove that the Per-Monitor-V2 manifest actually ends up in the executable, which no
//! unit test can show. Task 20 replaces all of this with the real wiring.

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(false)
        .init();

    #[cfg(windows)]
    {
        // First thing, before any HWND exists in the process — that is the documented
        // precondition for changing DPI awareness at runtime (ADR-0010).
        let report = switcher_windows::dpi::ensure_per_monitor_v2();
        tracing::info!(
            from_manifest = report.from_manifest,
            per_monitor_v2 = report.per_monitor_v2,
            "dpi awareness report"
        );
        if !report.from_manifest {
            tracing::warn!(
                "the manifest did not take effect; see ADR-0010 and crates/switcher-app/build.rs"
            );
        }
    }
}
