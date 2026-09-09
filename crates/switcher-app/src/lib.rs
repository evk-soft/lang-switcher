//! Application services, kept separate from native startup so they can be tested.

#![forbid(unsafe_code)]

pub mod capability;
pub mod config_io;
pub mod i18n;
pub mod logging;
pub mod menu;
pub mod paths;
pub mod render;
pub mod runtime;
pub mod sound;
#[cfg(windows)]
pub mod startup;
pub mod tray;
