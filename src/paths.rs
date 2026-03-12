use std::path::PathBuf;

use anyhow::{Context, Result};

pub fn data_dir() -> Result<PathBuf> {
    let mut path = dirs::data_local_dir()
        .or_else(fallback_data_dir)
        .context("failed to determine a writable data directory for Clipway")?;

    path.push("clipway");
    std::fs::create_dir_all(&path)
        .with_context(|| format!("failed to create data directory {}", path.display()))?;

    Ok(path)
}

pub fn database_path() -> Result<PathBuf> {
    let mut path = data_dir()?;
    path.push("clipway.sqlite3");
    Ok(path)
}

pub fn daemon_lock_path() -> Result<PathBuf> {
    let mut path = data_dir()?;
    path.push("daemon.lock");
    Ok(path)
}

pub fn gui_socket_path() -> Result<PathBuf> {
    let mut path = data_dir()?;
    path.push("gui.sock");
    Ok(path)
}

fn fallback_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        let mut path = home;
        path.push(".local");
        path.push("share");
        path
    })
}
