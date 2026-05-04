//! `autopilot.toml` config loader.
//!
//! Spec §8: tiny ledger-scoped config — `storage.db_path`,
//! `epic.default_max_attempts`, `suppression.default_window_hours`. Sections are
//! all optional; missing files yield [`Config::default`]. Field-level defaults
//! match the values previously hardcoded in `main.rs` / `cmd::task`.
//!
//! Precedence (resolved by callers, not here): CLI flag > env var > file > default.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub storage: StorageConfig,
    pub epic: EpicConfig,
    pub suppression: SuppressionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub db_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EpicConfig {
    pub default_max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SuppressionConfig {
    // TODO: wire into agent suppression flows. Defined here so operators can
    // tune the default `--until` window, but the autopilot CLI itself never
    // reads this — consumer agents pick it up from `autopilot.toml`.
    #[allow(dead_code)]
    pub default_window_hours: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from(".autopilot/state.db"),
        }
    }
}

impl Default for EpicConfig {
    fn default() -> Self {
        Self {
            default_max_attempts: 3,
        }
    }
}

impl Default for SuppressionConfig {
    fn default() -> Self {
        Self {
            default_window_hours: 24,
        }
    }
}

impl Config {
    /// Loads `path` if it exists; otherwise returns [`Config::default`].
    /// Returns Err only on read or parse failure (not on missing file).
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&raw).with_context(|| format!("parsing config {}", path.display()))?;
        Ok(cfg)
    }
}
