use super::paths::webdav_config_path;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub verify_tls: bool,
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            username: String::new(),
            password: String::new(),
            verify_tls: true,
        }
    }
}

pub fn load_webdav_config_impl() -> Result<WebDavConfig> {
    let path = webdav_config_path()?;
    if !path.exists() {
        return Ok(WebDavConfig::default());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_webdav_config_impl(config: WebDavConfig) -> Result<()> {
    let path = webdav_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&config)? + "\n")?;
    Ok(())
}
