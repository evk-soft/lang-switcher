//! Configuration file I/O, with protection for unreadable or unsupported files.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use switcher_core::config::Config;

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub warnings: Vec<String>,
    pub store: ConfigStore,
}

#[derive(Debug)]
pub struct ConfigStore {
    path: PathBuf,
    writable: bool,
}

impl ConfigStore {
    /// Loading never creates or rewrites a file. Unsupported/malformed files stay
    /// protected for this session so a later tray action cannot destroy them.
    pub fn load(path: PathBuf) -> LoadedConfig {
        let (config, warnings, writable) = match fs::read_to_string(&path) {
            Ok(text) => match Config::from_toml_str(&text) {
                Ok((config, warnings)) => (config, warnings, true),
                Err(error) => (Config::default(), vec![error.to_string()], false),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (Config::default(), vec![], true)
            }
            Err(error) => (Config::default(), vec![error.to_string()], false),
        };
        LoadedConfig {
            config,
            warnings,
            store: Self { path, writable },
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn save(&self, config: &Config) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::other(
                "original configuration is unreadable or unsupported; persistence disabled",
            ));
        }
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| io::Error::other("configuration path has no parent"))?;
        fs::create_dir_all(parent)?;
        // Same-directory rename replaces the old file without exposing truncated TOML.
        // create_new prevents overwriting an existing temporary file after PID reuse.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let (temp, mut file) = loop {
            let temp = parent.join(format!(
                ".lang-switcher-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(file) => break (temp, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let result = file
            .write_all(config.to_toml_string().as_bytes())
            .and_then(|()| file.sync_all());
        drop(file); // Windows must release the file handle before rename/remove.
        let result = result.and_then(|()| fs::rename(&temp, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct TempDir(pub std::path::PathBuf);
    impl TempDir {
        pub(crate) fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "lang-switcher-config-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_config_is_not_written_until_saved_and_replacement_roundtrips() {
        let dir = TempDir::new();
        let path = dir.0.join("nested/config.toml");
        let mut loaded = ConfigStore::load(path.clone());
        assert!(!path.exists());
        assert!(loaded.warnings.is_empty());
        loaded.store.save(&loaded.config).unwrap();
        loaded.config.sound.enabled = false;
        loaded.store.save(&loaded.config).unwrap();
        let reread = ConfigStore::load(path);
        assert_eq!(reread.config, loaded.config);
        assert_eq!(std::fs::read_dir(dir.0.join("nested")).unwrap().count(), 1);
    }

    #[test]
    fn invalid_or_future_config_survives_later_persistence_attempts() {
        let dir = TempDir::new();
        let path = dir.0.join("config.toml");
        for text in ["version = 999", "[broken"] {
            std::fs::write(&path, text).unwrap();
            let loaded = ConfigStore::load(path.clone());
            assert!(!loaded.warnings.is_empty());
            assert!(loaded.store.save(&loaded.config).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
    }
}
