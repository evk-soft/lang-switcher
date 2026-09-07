//! Per-user roaming configuration and local logs.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
#[error("could not resolve the user's configuration directory")]
pub struct PathsError;

pub fn resolve() -> Result<AppPaths, PathsError> {
    let dirs = directories::ProjectDirs::from("", "evk-soft", "lang-switcher").ok_or(PathsError)?;
    Ok(layout_from(dirs.config_dir(), dirs.data_local_dir()))
}

fn layout_from(config_dir: &Path, data_local_dir: &Path) -> AppPaths {
    AppPaths {
        config_file: config_dir.join("config.toml"),
        log_dir: data_local_dir.join("logs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_append_only_the_file_and_logs_directory() {
        let p = layout_from(Path::new("roaming/config"), Path::new("local/data"));
        assert_eq!(p.config_file, Path::new("roaming/config/config.toml"));
        assert_eq!(p.log_dir, Path::new("local/data/logs"));
        let real = resolve().unwrap();
        assert!(real.config_file.is_absolute());
        assert!(real.log_dir.is_absolute());
    }
}
