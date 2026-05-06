use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn data_dir() -> Result<PathBuf> {
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
