use super::paths::{
    app_config_dir, auth_json_path, codex_dir, config_toml_path, providers_config_path,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use toml_edit::{value, DocumentMut, Item, Value as TomlValue};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub provider: String,
    pub active_sessions: usize,
    pub archived_sessions: usize,
    pub codex_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: Option<String>,
    pub auth_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub wire_api: Option<String>,
    pub model: Option<String>,
    pub model_reasoning_effort: Option<String>,
    pub requires_openai_auth: Option<bool>,
    #[serde(default)]
    pub auth_json: Option<Value>,
    #[serde(default)]
    pub config_toml: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelOption {
    pub id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyUsage {
    pub date: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub events: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderUsage {
    pub provider: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub events: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub codex_dir: String,
    pub days: Vec<DailyUsage>,
    pub providers: Vec<ProviderUsage>,
    pub total: TokenUsage,
    pub total_cost_usd: f64,
    pub files_scanned: usize,
    pub usage_events: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadRepairSummary {
    pub rollout_files: usize,
    pub inserted_rows: usize,
    pub updated_rows: usize,
    pub index_entries: usize,
    pub remapped_cwd_rows: usize,
    pub workspace_hint_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderStore {
    providers: Vec<ProviderConfig>,
}

pub fn get_summary_impl() -> Result<Summary> {
    let dir = codex_dir()?;
    Ok(Summary {
        provider: current_provider().unwrap_or_else(|_| "openai".to_string()),
        active_sessions: rollout_count(&dir.join("sessions")),
        archived_sessions: rollout_count(&dir.join("archived_sessions")),
        codex_dir: dir.display().to_string(),
    })
}

pub fn get_usage_summary_impl() -> Result<UsageSummary> {
    let dir = codex_dir()?;
    let rollout_files = rollout_paths(&dir.join("sessions"));
    let mut days = BTreeMap::<String, DailyUsage>::new();
    let mut providers = BTreeMap::<String, ProviderUsage>::new();
    let mut seen_events = HashSet::<String>::new();
    let mut total = TokenUsage::default();
    let mut total_cost_usd = 0.0f64;
    let mut usage_events = 0usize;

    for file in &rollout_files {
        let content = fs::read_to_string(file)?;
        let mut previous_total: Option<TokenUsage> = None;
        let mut current_provider = "unknown".to_string();
        let mut current_model: Option<String> = None;
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(provider) = value
                .pointer("/payload/model_provider")
                .and_then(Value::as_str)
            {
                current_provider = provider.to_string();
            }
            if let Some(model) = model_from_value(&value) {
                current_model = Some(model);
            }
            if value.pointer("/payload/type").and_then(Value::as_str) != Some("token_count") {
                continue;
            }
            let info = value.pointer("/payload/info");
            let last_usage = usage_from_value(info.and_then(|info| info.get("last_token_usage")));
            let current_total =
                usage_from_value(info.and_then(|info| info.get("total_token_usage")));
            let Some(delta) = last_usage.or_else(|| {
                current_total.as_ref().map(|current| {
                    previous_total
                        .as_ref()
                        .map(|last| current.saturating_delta(last))
                        .unwrap_or_else(|| current.clone())
                })
            }) else {
                continue;
            };
            if let Some(current) = current_total {
                previous_total = Some(current);
            }
            let delta = delta.normalized();
            if delta.input_tokens <= 0
                && delta.cached_input_tokens <= 0
                && delta.output_tokens <= 0
                && delta.reasoning_output_tokens <= 0
            {
                continue;
            }
            let date = timestamp_date(&value).unwrap_or_else(|| "unknown".to_string());
            let model = info
                .and_then(model_from_info)
                .or_else(|| current_model.clone())
                .unwrap_or_else(|| "gpt-5".to_string());
            let event_key = usage_event_key(
                value.get("timestamp").and_then(Value::as_str),
                &model,
                &delta,
            );
            if !seen_events.insert(event_key) {
                continue;
            }
            let cost_usd = estimate_usage_cost_usd(&model, &delta);
            usage_events += 1;
            total.add(&delta);
            total_cost_usd += cost_usd;
            let daily = days.entry(date.clone()).or_insert_with(|| DailyUsage {
                date,
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: 0,
                cost_usd: 0.0,
                events: 0,
            });
            daily.input_tokens += delta.input_tokens;
            daily.cached_input_tokens += delta.cached_input_tokens;
            daily.output_tokens += delta.output_tokens;
            daily.reasoning_output_tokens += delta.reasoning_output_tokens;
            daily.total_tokens += delta.total_tokens;
            daily.cost_usd += cost_usd;
            daily.events += 1;
            let provider = providers
                .entry(current_provider.clone())
                .or_insert_with(|| ProviderUsage {
                    provider: current_provider.clone(),
                    input_tokens: 0,
                    cached_input_tokens: 0,
                    output_tokens: 0,
                    reasoning_output_tokens: 0,
                    total_tokens: 0,
                    cost_usd: 0.0,
                    events: 0,
                });
            provider.input_tokens += delta.input_tokens;
            provider.cached_input_tokens += delta.cached_input_tokens;
            provider.output_tokens += delta.output_tokens;
            provider.reasoning_output_tokens += delta.reasoning_output_tokens;
            provider.total_tokens += delta.total_tokens;
            provider.cost_usd += cost_usd;
            provider.events += 1;
        }
    }

    let mut days = days.into_values().collect::<Vec<_>>();
    days.reverse();
    let mut providers = providers.into_values().collect::<Vec<_>>();
    providers.sort_by(|left, right| right.total_tokens.cmp(&left.total_tokens));
    Ok(UsageSummary {
        codex_dir: dir.display().to_string(),
        days,
        providers,
        total,
        total_cost_usd,
        files_scanned: rollout_files.len(),
        usage_events,
    })
}

pub fn list_providers_impl() -> Result<Vec<ProviderConfig>> {
    let current = current_provider().unwrap_or_else(|_| "openai".to_string());
    let mut store = read_provider_store()?;
    ensure_builtin_openai(&mut store.providers);
    if current != "openai"
        && !store
            .providers
            .iter()
            .any(|provider| provider.id == current)
    {
        store.providers.insert(1, default_api_key_provider(current));
    }
    capture_current_live_config(&mut store.providers)?;
    for provider in &mut store.providers {
        if provider.auth_json.is_none() || provider.config_toml.is_none() {
            *provider = with_live_files(provider.clone())?;
        }
    }
    sort_providers(&mut store.providers);
    write_provider_store(&store)?;
    let mut visible = store.providers;
    for provider in &mut visible {
        provider.auth_json = None;
        provider.config_toml = None;
    }
    Ok(visible)
}

impl TokenUsage {
    fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }

    fn saturating_delta(&self, previous: &TokenUsage) -> TokenUsage {
        TokenUsage {
            input_tokens: (self.input_tokens - previous.input_tokens).max(0),
            cached_input_tokens: (self.cached_input_tokens - previous.cached_input_tokens).max(0),
            output_tokens: (self.output_tokens - previous.output_tokens).max(0),
            reasoning_output_tokens: (self.reasoning_output_tokens
                - previous.reasoning_output_tokens)
                .max(0),
            total_tokens: (self.total_tokens - previous.total_tokens).max(0),
        }
    }

    fn normalized(&self) -> TokenUsage {
        let cached_input_tokens = self.cached_input_tokens.min(self.input_tokens).max(0);
        let total_tokens = if self.total_tokens > 0 {
            self.total_tokens
        } else {
            self.input_tokens + self.output_tokens + self.reasoning_output_tokens
        };
        TokenUsage {
            input_tokens: self.input_tokens.max(0),
            cached_input_tokens,
            output_tokens: self.output_tokens.max(0),
            reasoning_output_tokens: self.reasoning_output_tokens.max(0),
            total_tokens: total_tokens.max(0),
        }
    }
}

fn usage_from_value(value: Option<&Value>) -> Option<TokenUsage> {
    let value = value?;
    Some(TokenUsage {
        input_tokens: value
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        cached_input_tokens: value
            .get("cached_input_tokens")
            .or_else(|| value.get("cache_read_input_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        output_tokens: value
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        reasoning_output_tokens: value
            .get("reasoning_output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        total_tokens: value
            .get("total_tokens")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    })
}

fn model_from_value(value: &Value) -> Option<String> {
    value
        .pointer("/payload/info")
        .and_then(model_from_info)
        .or_else(|| value.pointer("/payload/model").and_then(value_string))
        .or_else(|| value.pointer("/payload/model_name").and_then(value_string))
        .or_else(|| {
            value
                .pointer("/payload/metadata/model")
                .and_then(value_string)
        })
}

fn model_from_info(info: &Value) -> Option<String> {
    info.get("model")
        .and_then(value_string)
        .or_else(|| info.get("model_name").and_then(value_string))
        .or_else(|| info.pointer("/metadata/model").and_then(value_string))
}

fn value_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn usage_event_key(timestamp: Option<&str>, model: &str, usage: &TokenUsage) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        timestamp.unwrap_or_default(),
        model,
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.output_tokens,
        usage.reasoning_output_tokens,
        usage.total_tokens
    )
}

fn timestamp_date(value: &Value) -> Option<String> {
    let raw = value.get("timestamp").and_then(Value::as_str)?;
    DateTime::parse_from_rfc3339(raw)
        .map(|time| time.with_timezone(&Local).date_naive().to_string())
        .ok()
        .or_else(|| raw.get(0..10).map(ToOwned::to_owned))
}

struct ModelPricing {
    input_per_million: f64,
    cached_input_per_million: f64,
    output_per_million: f64,
}

fn estimate_usage_cost_usd(model: &str, usage: &TokenUsage) -> f64 {
    let Some(pricing) = pricing_for_model(model) else {
        return 0.0;
    };
    let cached_input = usage.cached_input_tokens.max(0) as f64;
    let fresh_input = (usage.input_tokens - usage.cached_input_tokens).max(0) as f64;
    let output = usage.output_tokens.max(0) as f64;
    ((fresh_input * pricing.input_per_million)
        + (cached_input * pricing.cached_input_per_million)
        + (output * pricing.output_per_million))
        / 1_000_000.0
}

fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.starts_with("gpt-5.5-pro") || normalized.starts_with("gpt-5.4-pro") {
        return Some(ModelPricing {
            input_per_million: 30.0,
            cached_input_per_million: 3.0,
            output_per_million: 180.0,
        });
    }
    if normalized.starts_with("gpt-5.5") {
        return Some(ModelPricing {
            input_per_million: 5.0,
            cached_input_per_million: 0.5,
            output_per_million: 30.0,
        });
    }
    if normalized.starts_with("gpt-5.4-mini") {
        return Some(ModelPricing {
            input_per_million: 0.75,
            cached_input_per_million: 0.075,
            output_per_million: 4.5,
        });
    }
    if normalized.starts_with("gpt-5.4-nano") {
        return Some(ModelPricing {
            input_per_million: 0.2,
            cached_input_per_million: 0.02,
            output_per_million: 1.25,
        });
    }
    if normalized.starts_with("gpt-5.4") {
        return Some(ModelPricing {
            input_per_million: 2.5,
            cached_input_per_million: 0.25,
            output_per_million: 15.0,
        });
    }
    if normalized.starts_with("gpt-5-mini") {
        return Some(ModelPricing {
            input_per_million: 0.25,
            cached_input_per_million: 0.025,
            output_per_million: 2.0,
        });
    }
    if normalized.starts_with("gpt-5-nano") {
        return Some(ModelPricing {
            input_per_million: 0.05,
            cached_input_per_million: 0.005,
            output_per_million: 0.4,
        });
    }
    if normalized.starts_with("gpt-5") {
        return Some(ModelPricing {
            input_per_million: 1.25,
            cached_input_per_million: 0.125,
            output_per_million: 10.0,
        });
    }
    None
}

pub fn get_provider_impl(provider_id: &str) -> Result<ProviderConfig> {
    validate_provider_id(provider_id)?;
    let mut store = read_provider_store()?;
    if !store
        .providers
        .iter()
        .any(|provider| provider.id == provider_id)
    {
        if provider_id == "openai" {
            store
                .providers
                .insert(0, default_openai_provider("openai".to_string()));
        } else {
            return Err(anyhow!("Provider 不存在：{}", provider_id));
        }
    }
    if current_provider().is_ok_and(|current| current == provider_id) {
        capture_current_live_config(&mut store.providers)?;
    }
    for provider in &mut store.providers {
        if provider.id == provider_id
            && (provider.auth_json.is_none() || provider.config_toml.is_none())
        {
            *provider = with_live_files(provider.clone())?;
        }
    }
    sort_providers(&mut store.providers);
    write_provider_store(&store)?;
    let mut provider = store
        .providers
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| anyhow!("Provider 不存在：{}", provider_id))?;
    provider.auth_json = None;
    provider.config_toml = None;
    Ok(provider)
}

pub fn save_provider_impl(provider: ProviderConfig) -> Result<()> {
    validate_provider_id(&provider.id)?;
    let mut store = read_provider_store()?;
    let mut provider = sanitize_provider(provider);
    if let Some(existing) = store.providers.iter().find(|item| item.id == provider.id) {
        provider.auth_json = provider.auth_json.or_else(|| existing.auth_json.clone());
        provider.config_toml = provider
            .config_toml
            .or_else(|| existing.config_toml.clone());
    }
    let provider = with_live_files(provider)?;
    let provider_id = provider.id.clone();
    let is_current = current_provider().is_ok_and(|current| current == provider_id);
    if let Some(existing) = store
        .providers
        .iter_mut()
        .find(|item| item.id == provider_id)
    {
        let next = provider.clone();
        *existing = next;
    } else {
        store.providers.push(provider.clone());
    }
    sort_providers(&mut store.providers);
    write_provider_store(&store)?;
    if is_current {
        apply_provider_to_codex(&provider)?;
    }
    Ok(())
}

pub fn delete_provider_impl(provider_id: &str) -> Result<()> {
    validate_provider_id(provider_id)?;
    if provider_id == "openai" {
        return Err(anyhow!("内建官方 Provider `openai` 不能删除"));
    }
    if current_provider().is_ok_and(|current| current == provider_id) {
        return Err(anyhow!("不能删除当前正在使用的 Provider：{}", provider_id));
    }
    let mut store = read_provider_store()?;
    store
        .providers
        .retain(|provider| provider.id != provider_id);
    write_provider_store(&store)
}

pub fn switch_provider_impl(provider_id: &str) -> Result<()> {
    let mut store = read_provider_store()?;
    capture_current_live_config(&mut store.providers)?;
    let provider = store
        .providers
        .iter()
        .cloned()
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| anyhow!("Provider 不存在：{}", provider_id))?;
    apply_provider_to_codex(&provider)?;
    write_provider_store(&store)?;
    Ok(())
}

pub fn restart_codex_app_impl() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let app_path = find_codex_app_path_macos()
            .ok_or_else(|| anyhow!("未找到 Codex.app，请确认已安装桌面版 Codex"))?;
        let _ = Command::new("osascript")
            .args(["-e", "tell application \"Codex\" to quit"])
            .status();
        wait_for_process_exit("Codex", 12, 500);
        let open_status = Command::new("open")
            .args(["-a", &app_path.display().to_string()])
            .status()?;
        if !open_status.success() {
            return Err(anyhow!("重新打开 Codex 失败"));
        }
        return Ok("已重启 Codex".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let launch_target = find_codex_launch_target_windows()
            .ok_or_else(|| anyhow!("未找到 Codex.exe 或 Codex.lnk，请确认已安装桌面版 Codex"))?;
        let _ = Command::new("taskkill")
            .args(["/IM", "Codex.exe", "/T", "/F"])
            .status();
        wait_for_process_exit("Codex.exe", 12, 500);
        let open_status = Command::new("cmd")
            .args(["/C", "start", "", &launch_target.display().to_string()])
            .status()?;
        if !open_status.success() {
            return Err(anyhow!("重新打开 Codex 失败"));
        }
        return Ok("已重启 Codex".to_string());
    }

    #[allow(unreachable_code)]
    Err(anyhow!("当前平台暂未实现自动重启 Codex"))
}

#[cfg(target_os = "macos")]
fn find_codex_app_path_macos() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/Applications/Codex.app"),
        dirs::home_dir()?.join("Applications").join("Codex.app"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn wait_for_process_exit(process_name: &str, max_attempts: usize, sleep_ms: u64) {
    for _ in 0..max_attempts {
        if !is_process_running(process_name) {
            return;
        }
        thread::sleep(Duration::from_millis(sleep_ms));
    }
}

fn is_process_running(process_name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "tell application \"System Events\" to count (every process whose name is \"{}\")",
                    process_name
                ),
            ])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim() != "0")
            .or_else(|_| {
                Command::new("pgrep")
                    .args(["-x", process_name])
                    .status()
                    .map(|status| status.success())
            })
            .unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {}", process_name)])
            .output()
            .map(|output| {
                let body = String::from_utf8_lossy(&output.stdout);
                body.lines().any(|line| line.starts_with(process_name))
            })
            .unwrap_or(false)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

#[cfg(target_os = "windows")]
fn find_codex_launch_target_windows() -> Option<PathBuf> {
    find_codex_exe_path_windows().or_else(find_codex_shortcut_path_windows)
}

#[cfg(target_os = "windows")]
fn find_codex_exe_path_windows() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Codex")
                .join("Codex.exe"),
        );
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(&program_files)
                .join("Codex")
                .join("Codex.exe"),
        );
        candidates.push(
            PathBuf::from(program_files)
                .join("Programs")
                .join("Codex")
                .join("Codex.exe"),
        );
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Codex")
                .join("Codex.exe"),
        );
    }
    candidates.into_iter().find(|path| path.exists())
}

#[cfg(target_os = "windows")]
fn find_codex_shortcut_path_windows() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        candidates.push(
            PathBuf::from(&user_profile)
                .join("Desktop")
                .join("Codex.lnk"),
        );
    }
    if let Some(public_profile) = std::env::var_os("PUBLIC") {
        candidates.push(
            PathBuf::from(public_profile)
                .join("Desktop")
                .join("Codex.lnk"),
        );
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Codex.lnk"),
        );
    }
    if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
        candidates.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Codex.lnk"),
        );
    }
    candidates.into_iter().find(|path| path.exists())
}

pub async fn fetch_provider_models_impl(provider: ProviderConfig) -> Result<Vec<ModelOption>> {
    let base_url = provider
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Base URL 不能为空"))?;
    let endpoint = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint).header("accept", "application/json");
    if let Some(api_key) = provider
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await?;
    let status = response.status();
    let payload = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "获取模型失败：HTTP {} {}",
            status.as_u16(),
            payload
        ));
    }
    let value: Value = serde_json::from_str(&payload)?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("模型接口响应中没有 data 数组"))?;
    let mut models = data
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .filter(|id| !id.trim().is_empty())
        .map(|id| ModelOption { id: id.to_string() })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

fn apply_provider_to_codex(provider: &ProviderConfig) -> Result<()> {
    let provider = if provider.auth_json.is_some() && provider.config_toml.is_some() {
        provider.clone()
    } else {
        with_live_files(provider.clone())?
    };
    fs::write(
        config_toml_path()?,
        provider
            .config_toml
            .ok_or_else(|| anyhow!("Provider 缺少 config.toml 快照"))?,
    )?;
    fs::write(
        auth_json_path()?,
        serde_json::to_string_pretty(
            &provider
                .auth_json
                .ok_or_else(|| anyhow!("Provider 缺少 auth.json 快照"))?,
        )?,
    )?;
    Ok(())
}

pub fn unify_thread_provider_impl() -> Result<String> {
    let dir = codex_dir()?;
    let provider = current_provider()?;
    let rollout_files = sync_rollout_paths(&dir);
    let mut rollout_changed = 0usize;
    for file in &rollout_files {
        let original = fs::read_to_string(file)?;
        let (updated, changed) = replace_provider_in_jsonl(&original, &provider);
        if changed {
            fs::write(file, updated)?;
            rollout_changed += 1;
        }
    }
    let repair = repair_thread_visibility_index_for_dir(&dir, &provider)?;
    Ok(format!(
        "合并完成：扫描 {} 个，修改 {} 个，补齐线程行 {} 条，更新线程行 {} 条，项目名映射 {} 条，项目归属 {} 条，索引 {} 条",
        rollout_files.len(),
        rollout_changed,
        repair.inserted_rows,
        repair.updated_rows,
        repair.remapped_cwd_rows,
        repair.workspace_hint_rows,
        repair.index_entries
    ))
}

pub fn repair_thread_visibility_index_impl() -> Result<String> {
    let dir = codex_dir()?;
    let provider = current_provider()?;
    let repair = repair_thread_visibility_index_for_dir(&dir, &provider)?;
    Ok(format!(
        "修复完成：扫描 {} 个，补齐线程行 {} 条，更新线程行 {} 条，项目名映射 {} 条，项目归属 {} 条，索引 {} 条",
        repair.rollout_files,
        repair.inserted_rows,
        repair.updated_rows,
        repair.remapped_cwd_rows,
        repair.workspace_hint_rows,
        repair.index_entries
    ))
}

pub fn repair_thread_visibility_index_for_current_provider() -> Result<ThreadRepairSummary> {
    let dir = codex_dir()?;
    let provider = current_provider()?;
    repair_thread_visibility_index_for_dir(&dir, &provider)
}

fn read_config_document() -> Result<DocumentMut> {
    let path = config_toml_path()?;
    let content = fs::read_to_string(path)?;
    Ok(content.parse::<DocumentMut>()?)
}

fn current_provider() -> Result<String> {
    let document = read_config_document()?;
    current_provider_from_document(&document).ok_or_else(|| anyhow!("无法读取当前 model_provider"))
}

fn current_provider_from_document(document: &DocumentMut) -> Option<String> {
    document
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned)
}

fn set_optional_document_string(document: &mut DocumentMut, key: &str, input: Option<String>) {
    if let Some(value_text) = input
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
    {
        document[key] = value(value_text);
    }
}

fn default_openai_provider(id: String) -> ProviderConfig {
    ProviderConfig {
        id,
        name: Some("Codex 默认 Provider".to_string()),
        auth_type: Some("chatgpt".to_string()),
        base_url: Some("https://api.openai.com/v1".to_string()),
        api_key: None,
        wire_api: Some("responses".to_string()),
        model: Some("gpt-5.4".to_string()),
        model_reasoning_effort: Some("medium".to_string()),
        requires_openai_auth: Some(false),
        auth_json: None,
        config_toml: None,
    }
}

fn read_provider_store() -> Result<ProviderStore> {
    let path = providers_config_path()?;
    let mut store = if !path.exists() {
        ProviderStore {
            providers: vec![default_openai_provider("openai".to_string())],
        }
    } else {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| ProviderStore {
            providers: vec![default_openai_provider("openai".to_string())],
        })
    };
    ensure_builtin_openai(&mut store.providers);
    sort_providers(&mut store.providers);
    Ok(store)
}

fn write_provider_store(store: &ProviderStore) -> Result<()> {
    fs::create_dir_all(app_config_dir()?)?;
    fs::write(
        providers_config_path()?,
        serde_json::to_string_pretty(store)?,
    )?;
    Ok(())
}

fn sanitize_provider(provider: ProviderConfig) -> ProviderConfig {
    if provider.id == "openai" {
        let mut builtin = default_openai_provider("openai".to_string());
        builtin.model = provider.model.or(builtin.model);
        builtin.model_reasoning_effort = provider
            .model_reasoning_effort
            .or(builtin.model_reasoning_effort);
        builtin.auth_json = provider.auth_json;
        builtin.config_toml = provider.config_toml;
        return builtin;
    }
    ProviderConfig {
        id: provider.id,
        name: Some(provider.name.unwrap_or_default()),
        auth_type: Some("api_key".to_string()),
        base_url: provider.base_url,
        api_key: provider.api_key,
        wire_api: provider.wire_api.or(Some("responses".to_string())),
        model: provider.model,
        model_reasoning_effort: provider.model_reasoning_effort,
        requires_openai_auth: Some(true),
        auth_json: provider.auth_json,
        config_toml: provider.config_toml,
    }
}

fn default_api_key_provider(id: String) -> ProviderConfig {
    ProviderConfig {
        id,
        name: None,
        auth_type: Some("api_key".to_string()),
        base_url: None,
        api_key: None,
        wire_api: Some("responses".to_string()),
        model: Some("gpt-5.4".to_string()),
        model_reasoning_effort: Some("high".to_string()),
        requires_openai_auth: Some(true),
        auth_json: None,
        config_toml: None,
    }
}

fn ensure_builtin_openai(providers: &mut Vec<ProviderConfig>) {
    let Some(index) = providers
        .iter()
        .position(|provider| provider.id == "openai")
    else {
        providers.insert(0, default_openai_provider("openai".to_string()));
        return;
    };

    let saved = providers.remove(index);
    let mut builtin = default_openai_provider("openai".to_string());
    builtin.model = saved.model.or(builtin.model);
    builtin.model_reasoning_effort = saved
        .model_reasoning_effort
        .or(builtin.model_reasoning_effort);
    builtin.auth_json = saved.auth_json;
    builtin.config_toml = saved.config_toml;
    providers.insert(0, builtin);
}

fn with_live_files(mut provider: ProviderConfig) -> Result<ProviderConfig> {
    provider.auth_json = Some(match provider.auth_type.as_deref().unwrap_or("api_key") {
        "chatgpt" => provider
            .auth_json
            .clone()
            .filter(is_chatgpt_auth_json)
            .or_else(read_current_chatgpt_auth_json)
            .unwrap_or_else(|| {
                serde_json::json!({
                    "auth_mode": "chatgpt",
                    "OPENAI_API_KEY": null
                })
            }),
        _ => serde_json::json!({
            "OPENAI_API_KEY": provider.api_key.as_deref().unwrap_or_default()
        }),
    });
    provider.config_toml = Some(match provider.auth_type.as_deref().unwrap_or("api_key") {
        "chatgpt" => build_official_config(&provider)?,
        _ => build_api_key_config(&provider)?,
    });
    Ok(provider)
}

fn capture_current_live_config(providers: &mut [ProviderConfig]) -> Result<()> {
    let current = current_provider().unwrap_or_else(|_| "openai".to_string());
    if let Some(provider) = providers.iter_mut().find(|provider| provider.id == current) {
        if let Ok(config) = fs::read_to_string(config_toml_path()?) {
            if let Ok(document) = config.parse::<DocumentMut>() {
                sync_provider_from_document(provider, &document);
            }
            provider.config_toml = Some(config);
        }
        if let Some(auth) = read_current_auth_json() {
            sync_provider_from_auth(provider, &auth);
            provider.auth_json = Some(auth);
        }
    }
    Ok(())
}

fn sync_provider_from_document(provider: &mut ProviderConfig, document: &DocumentMut) {
    provider.model = document_string(document, "model").or_else(|| provider.model.clone());
    provider.model_reasoning_effort = document_string(document, "model_reasoning_effort")
        .or_else(|| provider.model_reasoning_effort.clone());
    if provider.id == "openai" {
        provider.auth_type = Some("chatgpt".to_string());
        provider.requires_openai_auth = Some(false);
        return;
    }
    provider.auth_type = Some("api_key".to_string());
    provider.requires_openai_auth = Some(true);
    if let Some(table) = document
        .get("model_providers")
        .and_then(Item::as_table)
        .and_then(|table| table.get(provider.id.as_str()))
        .and_then(Item::as_table)
    {
        provider.base_url = table
            .get("base_url")
            .and_then(Item::as_value)
            .and_then(TomlValue::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| provider.base_url.clone());
        provider.wire_api = table
            .get("wire_api")
            .and_then(Item::as_value)
            .and_then(TomlValue::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| provider.wire_api.clone());
        provider.requires_openai_auth = table
            .get("requires_openai_auth")
            .and_then(Item::as_value)
            .and_then(TomlValue::as_bool)
            .or(provider.requires_openai_auth);
    }
}

fn sync_provider_from_auth(provider: &mut ProviderConfig, auth: &Value) {
    if provider.id == "openai" || is_chatgpt_auth_json(auth) {
        provider.auth_type = Some("chatgpt".to_string());
        provider.requires_openai_auth = Some(false);
        provider.api_key = None;
        return;
    }
    provider.auth_type = Some("api_key".to_string());
    provider.requires_openai_auth = Some(true);
    provider.api_key = auth
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| provider.api_key.clone());
}

fn document_string(document: &DocumentMut, key: &str) -> Option<String> {
    document
        .get(key)
        .and_then(Item::as_value)
        .and_then(TomlValue::as_str)
        .map(ToOwned::to_owned)
}

fn read_current_auth_json() -> Option<Value> {
    fs::read_to_string(auth_json_path().ok()?)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn read_current_chatgpt_auth_json() -> Option<Value> {
    let value = read_current_auth_json()?;
    if is_chatgpt_auth_json(&value) {
        Some(value)
    } else {
        None
    }
}

fn is_chatgpt_auth_json(value: &Value) -> bool {
    let is_chatgpt = value
        .get("auth_mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode == "chatgpt");
    let has_tokens = value.get("tokens").is_some();
    is_chatgpt && has_tokens
}

fn build_official_config(provider: &ProviderConfig) -> Result<String> {
    let source = provider
        .config_toml
        .clone()
        .or_else(read_current_official_config)
        .unwrap_or_else(default_official_config);
    let mut document = source.parse::<DocumentMut>().unwrap_or_else(|_| {
        default_official_config()
            .parse::<DocumentMut>()
            .expect("valid default official config")
    });
    document.as_table_mut().remove("model_providers");
    document["model_provider"] = value("openai");
    set_optional_document_string(&mut document, "model", provider.model.clone());
    set_optional_document_string(
        &mut document,
        "model_reasoning_effort",
        provider.model_reasoning_effort.clone(),
    );
    Ok(document.to_string())
}

fn build_api_key_config(provider: &ProviderConfig) -> Result<String> {
    let id = provider.id.trim();
    let model = provider.model.as_deref().unwrap_or("gpt-5.4").trim();
    let reasoning = provider
        .model_reasoning_effort
        .as_deref()
        .unwrap_or("high")
        .trim();
    let wire_api = provider.wire_api.as_deref().unwrap_or("responses").trim();
    let mut output = String::new();
    output.push_str(&format!("model_provider = \"{}\"\n", toml_escape(id)));
    output.push_str(&format!(
        "model = \"{}\"\n",
        toml_escape(if model.is_empty() { "gpt-5.4" } else { model })
    ));
    output.push_str(&format!(
        "model_reasoning_effort = \"{}\"\n",
        toml_escape(if reasoning.is_empty() {
            "high"
        } else {
            reasoning
        })
    ));
    output.push_str("disable_response_storage = true\n\n");
    output.push_str("[model_providers]\n");
    output.push_str(&format!("[model_providers.{}]\n", id));
    output.push_str(&format!("name = \"{}\"\n", toml_escape(id)));
    output.push_str(&format!(
        "wire_api = \"{}\"\n",
        toml_escape(if wire_api.is_empty() {
            "responses"
        } else {
            wire_api
        })
    ));
    output.push_str("requires_openai_auth = true\n");
    if let Some(base_url) = provider
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        output.push_str(&format!("base_url = \"{}\"\n", toml_escape(base_url)));
    }
    output.push_str("\n[features]\nmulti_agent = true\n\n");
    output.push_str("[plugins]\n");
    output.push_str("[plugins.\"github@openai-curated\"]\nenabled = true\n");
    Ok(output)
}

fn read_current_official_config() -> Option<String> {
    let content = fs::read_to_string(config_toml_path().ok()?).ok()?;
    let document = content.parse::<DocumentMut>().ok()?;
    let provider = current_provider_from_document(&document)?;
    if provider == "openai" {
        Some(content)
    } else {
        None
    }
}

fn default_official_config() -> String {
    [
        "model_provider = \"openai\"",
        "model = \"gpt-5.4\"",
        "model_reasoning_effort = \"medium\"",
        "",
        "[features]",
        "multi_agent = true",
        "",
        "[plugins]",
        "[plugins.\"github@openai-curated\"]",
        "enabled = true",
        "",
    ]
    .join("\n")
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn sort_providers(providers: &mut [ProviderConfig]) {
    providers.sort_by(|left, right| {
        let left_rank = if left.id == "openai" { 0 } else { 1 };
        let right_rank = if right.id == "openai" { 0 } else { 1 };
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.id.to_lowercase().cmp(&right.id.to_lowercase()))
    });
}

fn validate_provider_id(provider_id: &str) -> Result<()> {
    let trimmed = provider_id.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Provider ID 不能为空"));
    }
    let valid = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'));
    if !valid {
        return Err(anyhow!("Provider ID 只能包含字母、数字、点、横线或下划线"));
    }
    Ok(())
}

fn rollout_count(root: &Path) -> usize {
    rollout_paths(root).len()
}

fn sync_rollout_paths(codex: &Path) -> Vec<std::path::PathBuf> {
    ["sessions", "archived_sessions"]
        .iter()
        .flat_map(|root| rollout_paths(&codex.join(root)))
        .collect()
}

fn rollout_paths(root: &Path) -> Vec<std::path::PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        })
        .collect()
}

fn replace_provider_in_jsonl(content: &str, provider: &str) -> (String, bool) {
    let mut changed = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            lines.push(line.to_string());
            continue;
        };
        let is_meta = value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "session_meta");
        if !is_meta {
            lines.push(line.to_string());
            continue;
        }
        if let Some(payload) = value.get_mut("payload").and_then(Value::as_object_mut) {
            if payload
                .get("model_provider")
                .and_then(Value::as_str)
                .is_none_or(|current| current != provider)
            {
                payload.insert(
                    "model_provider".to_string(),
                    Value::String(provider.to_string()),
                );
                changed = true;
            }
        }
        lines.push(serde_json::to_string(&value).unwrap_or_else(|_| line.to_string()));
    }
    (lines.join("\n") + "\n", changed)
}

#[derive(Debug, Clone)]
struct ThreadMeta {
    id: String,
    rollout_path: PathBuf,
    archived: bool,
    created_at: i64,
    updated_at: i64,
    created_at_ms: i64,
    updated_at_ms: i64,
    source: String,
    model_provider: String,
    cwd: String,
    title: String,
    sandbox_policy: String,
    approval_mode: String,
    cli_version: String,
    first_user_message: String,
    git_sha: Option<String>,
    git_branch: Option<String>,
    git_origin_url: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    thread_source: Option<String>,
    preview: String,
    cwd_was_remapped: bool,
}

#[derive(Debug, Clone)]
struct ThreadColumn {
    name: String,
    kind: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key: bool,
}

fn repair_thread_visibility_index_for_dir(
    codex: &Path,
    provider: &str,
) -> Result<ThreadRepairSummary> {
    let rollout_files = sync_rollout_paths(codex);
    let project_mapper = ProjectNameMapper::from_codex(codex);
    let mut metas = Vec::new();
    for path in &rollout_files {
        if let Some(meta) = read_thread_meta(codex, path, provider, &project_mapper)? {
            metas.push(meta);
        }
    }
    let remapped_cwd_rows = metas.iter().filter(|meta| meta.cwd_was_remapped).count();
    let (inserted_rows, updated_rows) = upsert_thread_rows(codex, &metas)?;
    let workspace_hint_rows = repair_thread_workspace_root_hints(codex, &metas, &project_mapper)?;
    let index_entries = rebuild_index(codex, provider)?;
    Ok(ThreadRepairSummary {
        rollout_files: rollout_files.len(),
        inserted_rows,
        updated_rows,
        index_entries,
        remapped_cwd_rows,
        workspace_hint_rows,
    })
}

fn upsert_thread_rows(codex: &Path, metas: &[ThreadMeta]) -> Result<(usize, usize)> {
    let sqlite_path = codex.join("state_5.sqlite");
    if !sqlite_path.exists() {
        return Ok((0, 0));
    }
    let conn = Connection::open(sqlite_path)?;
    if !table_exists(&conn, "threads")? {
        return Ok((0, 0));
    }
    let columns = thread_columns(&conn)?;
    let column_names = columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<HashSet<_>>();
    if !column_names.contains("id") {
        return Ok((0, 0));
    }

    let existing_ids = existing_thread_ids(&conn)?;
    let mut inserted = 0usize;
    let mut updated = 0usize;
    for meta in metas {
        if existing_ids.contains(&meta.id) {
            updated += update_existing_thread_row(&conn, meta, &column_names)?;
        } else if insert_thread_row(&conn, meta, &columns)? {
            inserted += 1;
        }
    }
    Ok((inserted, updated))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn thread_columns(conn: &Connection) -> Result<Vec<ThreadColumn>> {
    let mut statement = conn.prepare("PRAGMA table_info(threads)")?;
    let columns = statement
        .query_map([], |row| {
            Ok(ThreadColumn {
                name: row.get(1)?,
                kind: row.get(2)?,
                not_null: row.get::<_, i64>(3)? != 0,
                default_value: row.get(4)?,
                primary_key: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

fn existing_thread_ids(conn: &Connection) -> Result<HashSet<String>> {
    let mut statement = conn.prepare("SELECT id FROM threads")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(ids)
}

fn update_existing_thread_row(
    conn: &Connection,
    meta: &ThreadMeta,
    column_names: &HashSet<&str>,
) -> Result<usize> {
    let mut assignments = Vec::new();
    let mut values = Vec::new();
    let updates = [
        (
            "rollout_path",
            SqlValue::Text(meta.rollout_path.display().to_string()),
        ),
        (
            "model_provider",
            SqlValue::Text(meta.model_provider.clone()),
        ),
        (
            "archived",
            SqlValue::Integer(if meta.archived { 1 } else { 0 }),
        ),
    ];
    for (name, value) in updates {
        if column_names.contains(name) {
            assignments.push(format!("{} = ?", quote_identifier(name)));
            values.push(value);
        }
    }
    if column_names.contains("cwd") && !meta.cwd.trim().is_empty() {
        assignments.push(format!("{} = ?", quote_identifier("cwd")));
        values.push(SqlValue::Text(meta.cwd.clone()));
    }
    for (name, value) in optional_update_values(meta) {
        if column_names.contains(name) {
            assignments.push(format!(
                "{} = COALESCE(NULLIF({}, ''), ?)",
                quote_identifier(name),
                quote_identifier(name)
            ));
            values.push(value);
        }
    }
    if assignments.is_empty() {
        return Ok(0);
    }
    values.push(SqlValue::Text(meta.id.clone()));
    let sql = format!("UPDATE threads SET {} WHERE id = ?", assignments.join(", "));
    Ok(conn.execute(&sql, params_from_iter(values))?)
}

fn optional_update_values(meta: &ThreadMeta) -> Vec<(&'static str, SqlValue)> {
    vec![
        ("source", SqlValue::Text(meta.source.clone())),
        ("thread_source", nullable_text(meta.thread_source.clone())),
        ("git_sha", nullable_text(meta.git_sha.clone())),
        ("git_branch", nullable_text(meta.git_branch.clone())),
        ("git_origin_url", nullable_text(meta.git_origin_url.clone())),
        ("model", nullable_text(meta.model.clone())),
        (
            "reasoning_effort",
            nullable_text(meta.reasoning_effort.clone()),
        ),
    ]
}

fn insert_thread_row(
    conn: &Connection,
    meta: &ThreadMeta,
    columns: &[ThreadColumn],
) -> Result<bool> {
    let mut names = Vec::new();
    let mut placeholders = Vec::new();
    let mut values = Vec::new();
    for column in columns {
        let Some(value) = thread_value_for_column(meta, column) else {
            continue;
        };
        names.push(quote_identifier(&column.name));
        placeholders.push("?".to_string());
        values.push(value);
    }
    if names.is_empty() {
        return Ok(false);
    }
    let sql = format!(
        "INSERT OR IGNORE INTO threads ({}) VALUES ({})",
        names.join(", "),
        placeholders.join(", ")
    );
    Ok(conn.execute(&sql, params_from_iter(values))? > 0)
}

fn thread_value_for_column(meta: &ThreadMeta, column: &ThreadColumn) -> Option<SqlValue> {
    let value = match column.name.as_str() {
        "id" => SqlValue::Text(meta.id.clone()),
        "rollout_path" => SqlValue::Text(meta.rollout_path.display().to_string()),
        "created_at" => SqlValue::Integer(meta.created_at),
        "updated_at" => SqlValue::Integer(meta.updated_at),
        "source" => SqlValue::Text(meta.source.clone()),
        "model_provider" => SqlValue::Text(meta.model_provider.clone()),
        "cwd" => SqlValue::Text(meta.cwd.clone()),
        "title" => SqlValue::Text(meta.title.clone()),
        "sandbox_policy" => SqlValue::Text(meta.sandbox_policy.clone()),
        "approval_mode" => SqlValue::Text(meta.approval_mode.clone()),
        "tokens_used" => SqlValue::Integer(0),
        "has_user_event" => SqlValue::Integer(if meta.first_user_message.is_empty() {
            0
        } else {
            1
        }),
        "archived" => SqlValue::Integer(if meta.archived { 1 } else { 0 }),
        "archived_at" => SqlValue::Null,
        "git_sha" => nullable_text(meta.git_sha.clone()),
        "git_branch" => nullable_text(meta.git_branch.clone()),
        "git_origin_url" => nullable_text(meta.git_origin_url.clone()),
        "cli_version" => SqlValue::Text(meta.cli_version.clone()),
        "first_user_message" => SqlValue::Text(meta.first_user_message.clone()),
        "agent_nickname" => SqlValue::Null,
        "agent_role" => SqlValue::Null,
        "memory_mode" => SqlValue::Text("enabled".to_string()),
        "model" => nullable_text(meta.model.clone()),
        "reasoning_effort" => nullable_text(meta.reasoning_effort.clone()),
        "agent_path" => SqlValue::Null,
        "created_at_ms" => SqlValue::Integer(meta.created_at_ms),
        "updated_at_ms" => SqlValue::Integer(meta.updated_at_ms),
        "thread_source" => nullable_text(meta.thread_source.clone()),
        "preview" => SqlValue::Text(meta.preview.clone()),
        _ => {
            if !column.not_null || column.default_value.is_some() || column.primary_key {
                return None;
            }
            fallback_sql_value_for_column(&column.kind)
        }
    };
    Some(value)
}

fn fallback_sql_value_for_column(kind: &str) -> SqlValue {
    let upper = kind.to_ascii_uppercase();
    if upper.contains("INT") {
        SqlValue::Integer(0)
    } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
        SqlValue::Real(0.0)
    } else {
        SqlValue::Text(String::new())
    }
}

fn nullable_text(value: Option<String>) -> SqlValue {
    value
        .filter(|value| !value.trim().is_empty())
        .map(SqlValue::Text)
        .unwrap_or(SqlValue::Null)
}

fn repair_thread_workspace_root_hints(
    codex: &Path,
    metas: &[ThreadMeta],
    project_mapper: &ProjectNameMapper,
) -> Result<usize> {
    let global_state_path = codex.join(".codex-global-state.json");
    if !global_state_path.exists() {
        return Ok(0);
    }
    let content = fs::read_to_string(&global_state_path)?;
    let mut value: Value = serde_json::from_str(&content)?;
    let Some(root) = value.as_object_mut() else {
        return Ok(0);
    };
    let hints_value = root
        .entry("thread-workspace-root-hints".to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(hints) = hints_value.as_object_mut() else {
        return Ok(0);
    };

    let mut changed = 0usize;
    for meta in metas {
        if meta.cwd.trim().is_empty() || !project_mapper.is_known_project_root(&meta.cwd) {
            continue;
        }
        if hints.get(&meta.id).and_then(Value::as_str) == Some(meta.cwd.as_str()) {
            continue;
        }
        hints.insert(meta.id.clone(), Value::String(meta.cwd.clone()));
        changed += 1;
    }
    if changed == 0 {
        return Ok(0);
    }

    backup_global_state(codex, &global_state_path)?;
    let tmp_path = global_state_path.with_extension(format!("json.tmp-{}", now_ms()));
    fs::write(&tmp_path, serde_json::to_string_pretty(&value)? + "\n")?;
    fs::rename(tmp_path, global_state_path)?;
    Ok(changed)
}

fn backup_global_state(codex: &Path, global_state_path: &Path) -> Result<()> {
    let backup_dir = codex.join("backups_state");
    fs::create_dir_all(&backup_dir)?;
    let backup_path = backup_dir.join(format!(
        ".codex-global-state.json.codex-tools-thread-hints-{}",
        now_ms()
    ));
    fs::copy(global_state_path, backup_path)?;
    Ok(())
}

fn read_thread_meta(
    codex: &Path,
    path: &Path,
    provider: &str,
    project_mapper: &ProjectNameMapper,
) -> Result<Option<ThreadMeta>> {
    let content = fs::read_to_string(path)?;
    let mut session_meta: Option<Value> = None;
    let mut first_user_message = String::new();
    let mut preview = String::new();
    let mut last_timestamp: Option<String> = None;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) {
            last_timestamp = Some(timestamp.to_string());
        }
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            session_meta = Some(value);
            continue;
        }
        if first_user_message.is_empty() {
            if let Some(message) = user_message_from_event(&value) {
                first_user_message = message;
            }
        }
        if preview.is_empty() {
            preview = first_user_message.clone();
        }
    }
    let Some(meta) = session_meta else {
        return Ok(None);
    };
    let payload = meta.get("payload").unwrap_or(&Value::Null);
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| thread_id_from_rollout_path(path))
        .unwrap_or_default();
    if id.is_empty() {
        return Ok(None);
    }
    let timestamp = payload
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| meta.get("timestamp").and_then(Value::as_str));
    let created_at_ms = timestamp.and_then(parse_time_ms).unwrap_or_else(now_ms);
    let updated_at_ms = last_timestamp
        .as_deref()
        .and_then(parse_time_ms)
        .unwrap_or(created_at_ms);
    if first_user_message.is_empty() {
        first_user_message =
            value_string(payload.get("title").unwrap_or(&Value::Null)).unwrap_or_default();
    }
    if preview.is_empty() {
        preview = first_user_message.clone();
    }
    let title = value_string(payload.get("title").unwrap_or(&Value::Null))
        .or_else(|| non_empty_text(&first_user_message))
        .unwrap_or_else(|| "Untitled".to_string());
    let raw_cwd = value_string(payload.get("cwd").unwrap_or(&Value::Null)).unwrap_or_default();
    let cwd = project_mapper.map_cwd(&raw_cwd);
    let cwd_was_remapped = normalize_cwd(raw_cwd) != cwd;
    Ok(Some(ThreadMeta {
        id,
        rollout_path: path.to_path_buf(),
        archived: path
            .strip_prefix(codex)
            .ok()
            .is_some_and(|relative| relative.starts_with("archived_sessions")),
        created_at: created_at_ms / 1000,
        updated_at: updated_at_ms / 1000,
        created_at_ms,
        updated_at_ms,
        source: value_string(payload.get("source").unwrap_or(&Value::Null))
            .unwrap_or_else(|| "codex".to_string()),
        model_provider: provider.to_string(),
        cwd,
        title,
        sandbox_policy: payload
            .get("sandbox_policy")
            .cloned()
            .or_else(|| payload.get("sandbox").cloned())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "{}".to_string()),
        approval_mode: value_string(payload.get("approval_mode").unwrap_or(&Value::Null))
            .unwrap_or_else(|| "on-request".to_string()),
        cli_version: value_string(payload.get("cli_version").unwrap_or(&Value::Null))
            .unwrap_or_default(),
        first_user_message: truncate_text(first_user_message, 4096),
        git_sha: payload.pointer("/git/commit_hash").and_then(value_string),
        git_branch: payload.pointer("/git/branch").and_then(value_string),
        git_origin_url: payload
            .pointer("/git/repository_url")
            .and_then(value_string),
        model: value_string(payload.get("model").unwrap_or(&Value::Null)),
        reasoning_effort: value_string(
            payload
                .get("model_reasoning_effort")
                .unwrap_or(&Value::Null),
        )
        .or_else(|| value_string(payload.get("reasoning_effort").unwrap_or(&Value::Null))),
        thread_source: value_string(payload.get("thread_source").unwrap_or(&Value::Null)),
        preview: truncate_text(preview, 4096),
        cwd_was_remapped,
    }))
}

fn user_message_from_event(value: &Value) -> Option<String> {
    if value.pointer("/payload/type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    if value.pointer("/payload/role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let content = value.pointer("/payload/content")?;
    if let Some(text) = content.as_str() {
        return non_empty_text(text);
    }
    let parts = content.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .or_else(|| part.get("content"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("\n");
    non_empty_text(&text)
}

fn non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn truncate_text(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    value.chars().take(max_chars).collect()
}

fn normalize_cwd(value: String) -> String {
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

#[derive(Debug, Clone, Default)]
struct ProjectNameMapper {
    unique_roots_by_project_name: HashMap<String, String>,
    known_project_roots: HashSet<String>,
}

impl ProjectNameMapper {
    fn from_codex(codex: &Path) -> Self {
        let global_state_path = codex.join(".codex-global-state.json");
        let Ok(content) = fs::read_to_string(global_state_path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            return Self::default();
        };

        let mut roots = Vec::new();
        for key in [
            "project-order",
            "electron-saved-workspace-roots",
            "active-workspace-roots",
        ] {
            let Some(values) = value.get(key).and_then(Value::as_array) else {
                continue;
            };
            roots.extend(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|root| !root.trim().is_empty())
                    .map(ToOwned::to_owned),
            );
        }

        let mut roots_by_name = HashMap::<String, HashSet<String>>::new();
        let mut known_project_roots = HashSet::new();
        for root in roots {
            let Some(project_name) = project_name_from_path(&root) else {
                continue;
            };
            known_project_roots.insert(root.clone());
            roots_by_name
                .entry(project_name_key(&project_name))
                .or_default()
                .insert(root);
        }

        let unique_roots_by_project_name = roots_by_name
            .into_iter()
            .filter_map(|(project_name, roots)| {
                if roots.len() == 1 {
                    roots.into_iter().next().map(|root| (project_name, root))
                } else {
                    None
                }
            })
            .collect();
        Self {
            unique_roots_by_project_name,
            known_project_roots,
        }
    }

    fn map_cwd(&self, raw_cwd: &str) -> String {
        let normalized = normalize_cwd(raw_cwd.to_string());
        let Some(project_name) = project_name_from_path(&normalized) else {
            return normalized;
        };
        self.unique_roots_by_project_name
            .get(&project_name_key(&project_name))
            .cloned()
            .unwrap_or(normalized)
    }

    fn is_known_project_root(&self, cwd: &str) -> bool {
        self.known_project_roots.contains(cwd)
    }
}

fn project_name_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn project_name_from_path(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let trimmed = normalized.trim().trim_end_matches('/');
    let name = trimmed
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or(trimmed)
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_time_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn thread_id_from_rollout_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?;
    let stem = filename.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    if stem.len() >= 36 {
        Some(stem[stem.len() - 36..].to_string())
    } else {
        Some(stem.to_string())
    }
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn rebuild_index(codex: &Path, provider: &str) -> Result<usize> {
    let sqlite_path = codex.join("state_5.sqlite");
    if !sqlite_path.exists() {
        return Ok(0);
    }
    let conn = Connection::open(sqlite_path)?;
    let mut statement = conn.prepare(
        "SELECT id, title, updated_at, updated_at_ms, cwd, git_origin_url, git_branch
         FROM threads
         ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC, id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let updated_at: i64 = row.get(2)?;
        let updated_at_ms: Option<i64> = row.get(3)?;
        let cwd: Option<String> = row.get(4)?;
        let git_origin_url: Option<String> = row.get(5)?;
        let git_branch: Option<String> = row.get(6)?;
        let project_root = cwd.clone().unwrap_or_default().to_lowercase();
        let project_name = cwd
            .as_deref()
            .and_then(project_name_from_path)
            .unwrap_or_default()
            .to_string();
        Ok(serde_json::json!({
            "id": id,
            "thread_name": title,
            "updated_at": updated_at_ms.unwrap_or(updated_at * 1000),
            "cwd": cwd,
            "model_provider": provider,
            "git_origin_url": git_origin_url,
            "git_branch": git_branch,
            "project_root": project_root,
            "project_name": project_name,
            "project_key": project_root
        }))
    })?;

    let mut output = Vec::new();
    for row in rows {
        output.push(serde_json::to_string(&row?)?);
    }
    fs::write(codex.join("session_index.jsonl"), output.join("\n") + "\n")?;
    Ok(output.len())
}
