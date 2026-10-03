use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayConfig {
    #[serde(default = "default_true")]
    pub decoration: bool,
    #[serde(default)]
    pub italic_fallback: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DisplayConfig {
    fn default() -> Self {
        DisplayConfig {
            decoration: true,
            italic_fallback: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            display: DisplayConfig {
                decoration: true,
                italic_fallback: false,
            },
        }
    }
}

impl Config {
    /// Return the standard path to `~/.config/mustang/config.toml`.
    pub fn config_path() -> Option<PathBuf> {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))?;
        Some(PathBuf::from(home).join(".config").join("mustang").join("config.toml"))
    }

    /// Load config from the default path. If the file does not exist, or if
    /// parsing fails due to corruption or invalid values, returns `Config::default()`.
    pub fn load() -> Self {
        match Self::config_path() {
            Some(path) => Self::load_from_path(&path),
            None => Config::default(),
        }
    }

    /// Load config from a specific path. Returns default on error or non-existence.
    pub fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            return Config::default();
        }
        let Ok(content) = fs::read_to_string(path) else {
            return Config::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    /// Save config to the default path. Creates parent directories if needed.
    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not determine home directory"))?;
        self.save_to_path(&path)
    }

    /// Save config to a specific path. Creates parent directories if needed.
    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_decoration_enabled() {
        let cfg = Config::default();
        assert!(cfg.display.decoration);
        assert!(!cfg.display.italic_fallback);
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let path = Path::new("nonexistent_config_file_test.toml");
        let cfg = Config::load_from_path(path);
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn load_corrupted_returns_default() {
        let dir = std::env::temp_dir().join(format!("mustang-cfg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupt.toml");
        fs::write(&path, "[display\ndecoration = invalid toml").unwrap();

        let cfg = Config::load_from_path(&path);
        assert_eq!(cfg, Config::default());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_partial_config_fills_missing_keys_with_defaults() {
        let dir = std::env::temp_dir().join(format!("mustang-cfg-test-partial-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("partial.toml");
        fs::write(&path, "[display]\nitalic_fallback = true\n").unwrap();

        let cfg = Config::load_from_path(&path);
        assert!(cfg.display.decoration); // default filled
        assert!(cfg.display.italic_fallback);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_ignores_unknown_keys_and_other_sections() {
        let dir = std::env::temp_dir().join(format!("mustang-cfg-test-unknown-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("unknown.toml");
        fs::write(&path, "[display]\ndecoration = false\nunknown_key = 123\n\n[future_section]\nfoo = 'bar'\n").unwrap();

        let cfg = Config::load_from_path(&path);
        assert!(!cfg.display.decoration);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mustang-cfg-test-save-{}", std::process::id()));
        let path = dir.join("sub").join("config.toml");

        let mut cfg = Config::default();
        cfg.display.decoration = false;
        cfg.display.italic_fallback = true;
        cfg.save_to_path(&path).unwrap();

        assert!(path.exists());
        let loaded = Config::load_from_path(&path);
        assert_eq!(loaded, cfg);

        let _ = fs::remove_dir_all(&dir);
    }
}
