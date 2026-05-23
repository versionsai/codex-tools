use anyhow::{anyhow, Result};
use std::{env, path::PathBuf};

pub fn codex_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(expand_home(PathBuf::from(path)));
    }

    Ok(get_home_dir().join(".codex"))
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

pub fn cc_connect_dir() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("wechatbot"))
}

pub fn cc_connect_config_path() -> Result<PathBuf> {
    Ok(cc_connect_dir()?.join("config.toml"))
}

pub fn bridge_qr_image_path() -> Result<PathBuf> {
    Ok(cc_connect_dir()?.join("qr").join("weixin-qr.png"))
}

pub fn managed_cc_connect_dir() -> Result<PathBuf> {
    Ok(cc_connect_dir()?.join("runtime"))
}

pub fn managed_cc_connect_bin_dir() -> Result<PathBuf> {
    Ok(managed_cc_connect_dir()?.join("bin"))
}

pub fn managed_cc_connect_binary_path() -> Result<PathBuf> {
    let binary = if cfg!(target_os = "windows") {
        "cc-connect.exe"
    } else {
        "cc-connect"
    };
    Ok(managed_cc_connect_bin_dir()?.join(binary))
}

pub fn managed_cc_connect_bridge_pid_path() -> Result<PathBuf> {
    Ok(managed_cc_connect_dir()?.join("bridge.pid"))
}

pub fn managed_cc_connect_setup_pid_path() -> Result<PathBuf> {
    Ok(managed_cc_connect_dir()?.join("setup.pid"))
}

pub fn managed_cc_connect_bridge_log_path() -> Result<PathBuf> {
    Ok(managed_cc_connect_dir()?.join("bridge.log"))
}

pub fn managed_cc_connect_setup_log_path() -> Result<PathBuf> {
    Ok(managed_cc_connect_dir()?.join("setup.log"))
}

fn get_home_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home;
    }

    if let Some(user_profile) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return PathBuf::from(user_profile);
    }

    if let (Some(drive), Some(path)) = (env::var_os("HOMEDRIVE"), env::var_os("HOMEPATH")) {
        let mut combined = PathBuf::from(drive);
        combined.push(PathBuf::from(path));
        return combined;
    }

    PathBuf::from(".")
}

fn expand_home(path: PathBuf) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path;
    };
    if raw == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path
}
