//! Win32 adapters. The only crate (besides future -macos/-linux) where `unsafe` is allowed;
//! every unsafe block carries a `// SAFETY:` comment (workspace lint enforces it).
#![cfg(windows)]

pub mod dpi;
pub mod overlay;
pub mod supervise;
pub mod win_util;
