use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn data_dir() -> Result<PathBuf> {
    // SHMARK_DATA_DIR is the integration-test escape hatch — it lets a test
    // harness boot a fresh AppState in a tempdir without polluting the real
    // ~/Library/Application Support/shmark/. Production callers leave it
    // unset and get the OS-standard data dir.
    if let Ok(custom) = std::env::var("SHMARK_DATA_DIR") {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let dir = dirs::data_dir().context("could not determine data dir")?;
    Ok(dir.join("shmark"))
}

pub fn ensure_data_dir() -> Result<PathBuf> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create data dir {}", dir.display()))?;
    Ok(dir)
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("shmark.sock"))
}

pub fn identity_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("identity.json"))
}

pub fn device_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("device.json"))
}

pub fn pid_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("shmark.pid"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("shmark.log"))
}

pub fn groups_state_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("groups.json"))
}
