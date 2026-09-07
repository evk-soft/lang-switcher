//! Prints native layout notifications and re-reads their authoritative current state.
use std::time::{Duration, Instant};
use switcher_platform::events::PlatformEvent;
use switcher_platform::ports::LayoutMonitor;
use switcher_windows::layout_monitor::LayoutHooks;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let started = Instant::now();
    let (events, received) = crossbeam_channel::unbounded();
    let monitor = LayoutHooks::new(events)?;
    tracing::info!(snapshot = ?monitor.current(), "startup snapshot");
    let seconds = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<u64>().ok());
    let timeout = seconds
        .map(|seconds| crossbeam_channel::after(Duration::from_secs(seconds)))
        .unwrap_or_else(crossbeam_channel::never);
    loop {
        crossbeam_channel::select! {
            recv(timeout) -> _ => break,
            recv(received) -> event => match event {
                Ok(event @ PlatformEvent::LayoutChanged { .. }) => tracing::info!(elapsed_ms = started.elapsed().as_millis(), ?event, current = ?monitor.current(), "layout event"),
                Ok(event) => tracing::info!(?event, "adapter status"),
                Err(_) => break,
            }
        }
    }
    Ok(())
}
