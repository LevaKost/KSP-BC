//! User configuration stored under the platform config directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::DEFAULT_PORT;
use crate::Result;

const CONFIG_FILE: &str = "config.toml";
const APP_DIR: &str = "ksp-share";

/// Persisted user preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default TCP port for `send`/`receive`.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional override for the detected KSP install root.
    #[serde(default)]
    pub ksp_root: Option<PathBuf>,

    #[serde(skip)]
    path: PathBuf,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            ksp_root: None,
            path: default_config_path(),
        }
    }
}

impl Config {
    /// Load the user's config file, returning a populated default when absent.
    pub fn load_or_default() -> Result<Self> {
        let path = default_config_path();
        Self::load_from(&path)
    }

    /// Load a config from a specific path, returning defaults if it does not exist.
    pub fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let mut cfg: Config = toml_lite::from_str(&text);
                cfg.path = path.to_path_buf();
                Ok(cfg)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self {
                path: path.to_path_buf(),
                ..Self::default()
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// Path of the config file backing this instance.
    pub fn config_path(&self) -> &Path {
        &self.path
    }
}

fn default_config_path() -> PathBuf {
    if let Some(dir) = dirs::config_dir() {
        return dir.join(APP_DIR).join(CONFIG_FILE);
    }
    PathBuf::from(CONFIG_FILE)
}

/// Tiny ad-hoc TOML reader so we don't pull in a full TOML crate just to
/// read two scalar keys. Lines like `key = "value"` and `key = 1234` are
/// understood; everything else (comments, blank lines) is ignored.
mod toml_lite {
    use super::Config;

    pub fn from_str(text: &str) -> Config {
        let mut cfg = Config::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "port" => {
                    if let Ok(p) = value.parse::<u16>() {
                        cfg.port = p;
                    }
                }
                "ksp_root" if !value.is_empty() => {
                    cfg.ksp_root = Some(value.into());
                }
                _ => {}
            }
        }
        cfg
    }
}
