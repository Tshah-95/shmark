use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// User-tunable settings persisted to data_dir/settings.json. Anything
/// unset on disk falls back to the field's `default` annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Tauri-style accelerator string for the share-from-clipboard hotkey.
    /// Default: CmdOrCtrl+Shift+P. Empty string disables the hotkey.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Project roots searched when paths_resolve gets a relative path or
    /// basename. If empty, the built-in defaults from `resolve::default_roots`
    /// are used.
    #[serde(default)]
    pub search_roots: Vec<String>,

    /// When true, recipients fetch shared blobs into local storage on
    /// receipt rather than waiting for a viewer to demand them. Default true.
    #[serde(default = "default_true")]
    pub auto_pin: bool,
}

fn default_hotkey() -> String {
    "CmdOrCtrl+Shift+P".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            search_roots: Vec::new(),
            auto_pin: true,
        }
    }
}

impl Settings {
    pub fn load_or_default() -> Result<Self> {
        let path = settings_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load(&path)
    }

    fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path)
            .with_context(|| format!("read settings {}", path.display()))?;
        let parsed: Self = serde_json::from_str(&s)
            .with_context(|| format!("parse settings {}", path.display()))?;
        Ok(parsed)
    }

    pub fn save(&self) -> Result<()> {
        let path = settings_path()?;
        paths::ensure_data_dir()?;
        let s = serde_json::to_string_pretty(self)?;
        fs::write(&path, s)
            .with_context(|| format!("write settings {}", path.display()))?;
        Ok(())
    }

    /// Returns the actual roots to search — user's list if non-empty,
    /// otherwise the built-in defaults.
    pub fn effective_search_roots(&self) -> Vec<PathBuf> {
        if self.search_roots.is_empty() {
            crate::resolve::default_roots()
        } else {
            self.search_roots.iter().map(PathBuf::from).collect()
        }
    }
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(paths::data_dir()?.join("settings.json"))
}
