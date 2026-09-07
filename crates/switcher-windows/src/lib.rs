//! Win32 adapters. The only crate (besides future -macos/-linux) where `unsafe` is allowed;
//! every unsafe block carries a `// SAFETY:` comment (workspace lint enforces it).
#![cfg(windows)]

pub mod autostart;
pub mod dpi;
pub mod layout_monitor;
pub mod overlay;
pub mod pointer;
pub mod quit_signal;
pub mod supervise;
pub mod tsf;
pub mod win_util;
