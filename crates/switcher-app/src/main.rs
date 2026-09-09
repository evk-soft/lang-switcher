//! Windows M1 entry point. All service lifetimes are owned by startup::run.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    let options = switcher_app::startup::Options::from_args()?;
    switcher_app::startup::run(options)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("lang-switcher M1 currently supports Windows only");
}
