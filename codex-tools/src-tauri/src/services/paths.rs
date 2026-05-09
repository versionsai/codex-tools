use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub fn codex_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法定位用户主目录"))?;
    Ok(home.join(".codex"))
}

pub fn config_toml_path() -> Result<PathBuf> {
    Ok(codex_dir()?.join("config.toml"))
}

pub fn auth_json_path() -> Result<PathBuf> {
    Ok(codex_dir()?.join("auth.json"))
}

pub fn app_config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow!("无法定位应用配置目录"))?;
    Ok(base.join("codex-tools"))
}

pub fn providers_config_path() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("providers.json"))
}

pub fn webdav_config_path() -> Result<PathBuf> {
    Ok(codex_dir()?.join("webdav_sync_config.json"))
}
