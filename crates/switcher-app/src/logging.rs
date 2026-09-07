//! Fallible rolling logs; the returned guard must live through shutdown.

use std::path::Path;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("could not create the log directory: {0}")]
    Directory(#[from] std::io::Error),
    #[error("could not open the log: {0}")]
    File(#[from] tracing_appender::rolling::InitError),
    #[error("could not install the log subscriber: {0}")]
    Subscriber(String),
}

pub fn init(log_dir: &Path, level: &str) -> Result<WorkerGuard, LogError> {
    // The appender's retention scan runs before it creates the first log file.
    // Pre-create the directory so a normal first launch does not print a false error.
    std::fs::create_dir_all(log_dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("lang-switcher")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(parse_level(level).into())
                .with_env_var("LANG_SWITCHER_LOG")
                .from_env_lossy(),
        )
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .map_err(|e| LogError::Subscriber(e.to_string()))?;
    Ok(guard)
}

fn parse_level(level: &str) -> LevelFilter {
    if level.is_empty() {
        LevelFilter::INFO
    } else {
        level.parse().unwrap_or(LevelFilter::INFO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_unknown_levels_use_info() {
        assert_eq!(parse_level("debug"), LevelFilter::DEBUG);
        assert_eq!(parse_level("WARN"), LevelFilter::WARN);
        assert_eq!(parse_level(""), LevelFilter::INFO);
        assert_eq!(parse_level("nonsense"), LevelFilter::INFO);
    }

    #[test]
    fn file_in_place_of_log_directory_returns_an_error() {
        let dir = crate::config_io::tests::TempDir::new();
        let file = dir.0.join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(init(&file, "info").is_err());
    }
}
