//! Read-only inspection of the real HKCU Run entry. The native roundtrip test writes
//! an isolated non-Run key, so running checks never enables autostart of a test binary.
use switcher_platform::ports::Autostart;
use switcher_windows::autostart::RegistryAutostart;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    println!(
        "autostart enabled: {}",
        RegistryAutostart::new().is_enabled()?
    );
    Ok(())
}
