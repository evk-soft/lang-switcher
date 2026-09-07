//! Application services, kept separate from native startup so they can be tested.

#![forbid(unsafe_code)]

pub mod config_io;
pub mod logging;
pub mod paths;
pub mod render;
pub mod sound;
