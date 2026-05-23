use super::paths::{
    bridge_qr_image_path, cc_connect_config_path, cc_connect_dir, config_toml_path,
    managed_cc_connect_bin_dir, managed_cc_connect_binary_path, managed_cc_connect_bridge_log_path,
    managed_cc_connect_bridge_pid_path, managed_cc_connect_setup_log_path,
    managed_cc_connect_setup_pid_path,
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

const WECHAT_ENGINE_VERSION: &str = "1.3.2";
static WECHAT_ENGINE_VERSION_CACHE: OnceLock<Option<String>> = OnceLock::new();
static QR_DATA_URL_CACHE: OnceLock<Mutex<Option<(u64, String)>>> = OnceLock::new();

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const EMBEDDED_WECHAT_ENGINE: &[u8] =
    include_bytes!("../../resources/engine/darwin-arm64/wechat-connect-engine");

#[derive(Debug, Clone, Serialize)]
pub struct BridgeProjectStatus {
    pub name: String,
    pub work_dir: String,
    pub agent_type: String,
    pub has_weixin: bool,
    pub has_weixin_session: bool,
    pub has_codex: bool,
    pub allow_from: String,
    pub admin_from: String,
    pub model: String,
    pub permission_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexProjectStatus {
    pub name: String,
    pub work_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub installed: bool,
    pub cc_connect_path: Option<String>,
    pub version: Option<String>,
    pub config_path: String,
    pub config_exists: bool,
    pub daemon_status: String,
    pub qr_image_path: String,
    pub qr_image_exists: bool,
    pub qr_image_data_url: Option<String>,
    pub service_running: bool,
    pub login_running: bool,
    pub suggested_project_name: String,
    pub suggested_snippet: String,
    pub weixin_setup_command: String,
    pub start_command: String,
    pub has_logged_in_wechat_session: bool,
    pub communication_ready: bool,
    pub communication_hint: String,
    pub projects: Vec<BridgeProjectStatus>,
    pub codex_projects: Vec<CodexProjectStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeProjectDraft {
    pub name: String,
    pub work_dir: String,
    pub allow_from: String,
    pub admin_from: String,
    pub model: String,
    pub permission_mode: String,
}

#[derive(Debug, Clone)]
struct WeixinSession {
    token: String,
    account_id: String,
    base_url: String,
}

pub fn get_bridge_status_impl() -> Result<BridgeStatus> {
    let config_path = cc_connect_config_path()?;
    let qr_image_path = bridge_qr_image_path()?;
    let qr_image_exists = qr_image_path.exists();
    let qr_image_data_url = if qr_image_exists {
        Some(read_png_data_url(&qr_image_path)?)
    } else {
        None
    };
    let service_running = read_pid_file(&managed_cc_connect_bridge_pid_path()?)
        .map(|pid| process_alive(pid))
        .unwrap_or(false);
    let setup_pid_path = managed_cc_connect_setup_pid_path()?;
    let setup_process_running = read_pid_file(&setup_pid_path)
        .map(|pid| process_alive(pid))
        .unwrap_or(false);
    let cc_connect_path = find_cc_connect_path();
    let installed = cc_connect_path.is_some();
    let version = if installed {
        read_cc_connect_version_cached(cc_connect_path.as_deref())?
    } else {
        None
    };
    let daemon_status = if installed {
        read_bridge_status_text(cc_connect_path.as_deref())?
    } else {
        "未检测到微信连接服务".to_string()
    };
    let projects = if config_path.exists() {
        read_projects(&config_path)?
    } else {
        Vec::new()
    };
    let has_logged_in_wechat_session = projects.iter().any(|project| project.has_weixin_session);
    if service_running && setup_process_running {
        let _ = fs::remove_file(&setup_pid_path);
    }
    let login_running = setup_process_running && !service_running;
    let (communication_ready, communication_hint) = read_weixin_communication_state(&projects)?;
    let codex_projects = read_codex_projects()?;
    let suggested_project_name = suggest_project_name();
    let suggested_snippet = build_project_snippet(&suggested_project_name);
    let cc_bin = cc_connect_path.clone().unwrap_or_else(|| {
        managed_cc_connect_binary_path()
            .unwrap_or_else(|_| PathBuf::from("cc-connect"))
            .display()
            .to_string()
    });
    let weixin_setup_command = format!(
        "{} weixin setup --project {} --qr-image {} --config {}",
        shell_quote(&cc_bin),
        shell_quote(&suggested_project_name),
        shell_quote(&qr_image_path.display().to_string()),
        shell_quote(&config_path.display().to_string())
    );
    let start_command = format!(
        "{} --config {}",
        shell_quote(&cc_bin),
        shell_quote(&config_path.display().to_string())
    );
    Ok(BridgeStatus {
        installed,
        cc_connect_path,
        version,
        config_path: config_path.display().to_string(),
        config_exists: config_path.exists(),
        daemon_status,
        qr_image_path: qr_image_path.display().to_string(),
        qr_image_exists,
        qr_image_data_url,
        service_running,
        login_running,
        suggested_project_name,
        suggested_snippet,
        weixin_setup_command,
        start_command,
        has_logged_in_wechat_session,
        communication_ready,
        communication_hint,
        projects,
        codex_projects,
    })
}

pub fn open_wechat_setup_terminal_impl(project_name: &str) -> Result<String> {
    let project_name = project_name.trim();
    if project_name.is_empty() {
        return Err(anyhow!("项目名不能为空"));
    }
    ensure_managed_cc_connect_installed_blocking()?;
    let cc_connect_path = managed_cc_connect_binary_path()?;
    let config_path = cc_connect_config_path()?;
    ensure_config_data_dir(&config_path)?;
    let qr_image_path = bridge_qr_image_path()?;
    if let Some(parent) = qr_image_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(&qr_image_path);
    let _ = stop_managed_process(&managed_cc_connect_setup_pid_path()?, "微信登录进程");
    spawn_managed_process(
        &cc_connect_path,
        &[
            "weixin",
            "setup",
            "--project",
            project_name,
            "--qr-image",
            &qr_image_path.display().to_string(),
            "--config",
            &config_path.display().to_string(),
        ],
        &managed_cc_connect_setup_pid_path()?,
        &managed_cc_connect_setup_log_path()?,
    )?;
    Ok(format!(
        "已开始微信登录流程，二维码会保存到 {}",
        qr_image_path.display()
    ))
}

pub fn open_cc_connect_terminal_impl() -> Result<String> {
    ensure_managed_cc_connect_installed_blocking()?;
    let cc_connect_path = managed_cc_connect_binary_path()?;
    let config_path = cc_connect_config_path()?;
    ensure_config_data_dir(&config_path)?;
    let log_path = managed_cc_connect_bridge_log_path()?;
    let pid_path = managed_cc_connect_bridge_pid_path()?;
    if let Some(pid) = read_pid_file(&pid_path) {
        if process_alive(pid) {
            return Ok(format!(
                "微信服务已在运行中，日志输出到 {}",
                log_path.display()
            ));
        }
        let _ = fs::remove_file(&pid_path);
    }
    spawn_managed_process(
        &cc_connect_path,
        &["--force", "--config", &config_path.display().to_string()],
        &pid_path,
        &log_path,
    )?;
    Ok(format!("已启动微信服务，日志输出到 {}", log_path.display()))
}

pub fn install_cc_connect_impl() -> Result<String> {
    ensure_managed_cc_connect_installed_blocking()?;
    let binary_path = managed_cc_connect_binary_path()?;
    Ok(format!("已初始化微信连接引擎：{}", binary_path.display()))
}

pub fn run_daemon_command_impl(action: &str) -> Result<String> {
    let normalized = match action.trim() {
        "install" => "install",
        "uninstall" => "uninstall",
        "start" => "start",
        "stop" => "stop",
        "restart" => "restart",
        "status" => "status",
        "logs" => "logs",
        _ => return Err(anyhow!("不支持的微信服务操作：{}", action)),
    };

    match normalized {
        "install" | "start" => {
            open_cc_connect_terminal_impl()?;
            Ok("已启动微信服务".to_string())
        }
        "stop" | "uninstall" => {
            let stopped =
                stop_managed_process(&managed_cc_connect_bridge_pid_path()?, "微信服务进程")?;
            Ok(if stopped {
                "已停止微信服务".to_string()
            } else {
                "微信服务当前未运行".to_string()
            })
        }
        "restart" => {
            let _ = stop_managed_process(&managed_cc_connect_bridge_pid_path()?, "微信服务进程");
            open_cc_connect_terminal_impl()?;
            Ok("已重启微信服务".to_string())
        }
        "status" => Ok(read_bridge_status_text(find_cc_connect_path().as_deref())?),
        "logs" => {
            let path = managed_cc_connect_bridge_log_path()?;
            if !path.exists() {
                return Ok("暂未生成微信服务日志".to_string());
            }
            let content = fs::read_to_string(path)?;
            Ok(content
                .lines()
                .rev()
                .take(40)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"))
        }
        _ => unreachable!(),
    }
}

pub fn save_bridge_project_impl(draft: BridgeProjectDraft) -> Result<String> {
    let draft = normalize_project_draft(draft)?;
    let config_path = cc_connect_config_path()?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut document = if config_path.exists() {
        fs::read_to_string(&config_path)?.parse::<DocumentMut>()?
    } else {
        let mut doc = String::new();
        doc.push_str("[log]\nlevel = \"info\"\n");
        doc.push_str(
            "\n[display]\nmode = \"yolo\"\nthinking_messages = true\ntool_messages = false\n",
        );
        doc.parse::<DocumentMut>()?
    };
    document["data_dir"] = value(cc_connect_dir()?.display().to_string());
    let existing_session = find_session_for_project(&document, &draft.name)
        .or_else(|| find_any_weixin_session(&document));

    let mut projects = ArrayOfTables::new();
    let mut table = Table::new();
    write_project_table(&mut table, &draft, existing_session);
    projects.push(table);

    document["projects"] = Item::ArrayOfTables(projects);
    fs::write(&config_path, document.to_string())?;
    Ok(format!("已切换微信连接项目：{}", draft.name))
}

pub fn pick_work_dir_impl() -> Result<Option<String>> {
    let selected = rfd::FileDialog::new().pick_folder();
    Ok(selected.map(|path| path.display().to_string()))
}

fn read_projects(path: &Path) -> Result<Vec<BridgeProjectStatus>> {
    let content = fs::read_to_string(path)?;
    let document = content.parse::<DocumentMut>()?;
    let mut projects = Vec::new();
    let Some(project_items) = document.get("projects").and_then(Item::as_array_of_tables) else {
        return Ok(projects);
    };
    for project in project_items.iter() {
        let name = table_string(project, "name").unwrap_or_else(|| "(未命名项目)".to_string());
        let work_dir = project
            .get("agent")
            .and_then(Item::as_table)
            .and_then(|agent| agent.get("options"))
            .and_then(Item::as_table)
            .and_then(|options| table_string(options, "work_dir"))
            .or_else(|| table_string(project, "work_dir"))
            .unwrap_or_default();
        let agent_type = project
            .get("agent")
            .and_then(Item::as_table)
            .and_then(|agent| table_string(agent, "type"))
            .unwrap_or_default();
        let has_codex = agent_type == "codex";
        let has_weixin = project
            .get("platforms")
            .and_then(Item::as_array_of_tables)
            .is_some_and(|platforms| {
                platforms
                    .iter()
                    .any(|platform| table_string(platform, "type").as_deref() == Some("weixin"))
            });
        let (allow_from, admin_from, has_weixin_session) = read_weixin_fields(project);
        let model = project
            .get("agent")
            .and_then(Item::as_table)
            .and_then(|agent| {
                table_string(agent, "model").or_else(|| {
                    agent
                        .get("options")
                        .and_then(Item::as_table)
                        .and_then(|options| table_string(options, "model"))
                })
            })
            .unwrap_or_default();
        let permission_mode = project
            .get("agent")
            .and_then(Item::as_table)
            .and_then(|agent| agent.get("options"))
            .and_then(Item::as_table)
            .and_then(|options| table_string(options, "mode"))
            .map(|mode| normalize_permission_mode(&mode))
            .unwrap_or_else(|| "plan".to_string());
        projects.push(BridgeProjectStatus {
            name,
            work_dir,
            agent_type,
            has_weixin,
            has_weixin_session,
            has_codex,
            allow_from,
            admin_from,
            model,
            permission_mode,
        });
    }
    Ok(projects)
}

fn ensure_config_data_dir(config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }
    let mut document = fs::read_to_string(config_path)?.parse::<DocumentMut>()?;
    let data_dir = cc_connect_dir()?.display().to_string();
    if document
        .get("data_dir")
        .and_then(Item::as_value)
        .and_then(|value| value.as_str())
        == Some(data_dir.as_str())
    {
        return Ok(());
    }
    document["data_dir"] = value(data_dir);
    fs::write(config_path, document.to_string())?;
    Ok(())
}

fn read_codex_projects() -> Result<Vec<CodexProjectStatus>> {
    let mut ordered_paths = Vec::new();
    let mut seen = BTreeSet::new();

    for work_dir in read_codex_projects_from_global_state()? {
        if seen.insert(work_dir.clone()) {
            ordered_paths.push(work_dir);
        }
    }
    for work_dir in read_codex_projects_from_config()? {
        if seen.insert(work_dir.clone()) {
            ordered_paths.push(work_dir);
        }
    }

    Ok(ordered_paths
        .into_iter()
        .map(|work_dir| CodexProjectStatus {
            name: project_display_name(&work_dir),
            work_dir,
        })
        .collect())
}

fn read_codex_projects_from_global_state() -> Result<Vec<String>> {
    let path = config_toml_path()?
        .parent()
        .ok_or_else(|| anyhow!("无法定位 Codex 配置目录"))?
        .join(".codex-global-state.json");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&content)?;
    let mut items = Vec::new();

    if let Some(order) = value.get("project-order").and_then(Value::as_array) {
        for item in order {
            if let Some(path) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                items.push(path.to_string());
            }
        }
    }
    if let Some(saved) = value
        .get("electron-saved-workspace-roots")
        .and_then(Value::as_array)
    {
        for item in saved {
            if let Some(path) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                items.push(path.to_string());
            }
        }
    }

    Ok(items)
}

fn read_codex_projects_from_config() -> Result<Vec<String>> {
    let path = config_toml_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let document = content.parse::<DocumentMut>()?;
    let Some(projects) = document.get("projects").and_then(Item::as_table_like) else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for (key, _) in projects.iter() {
        let work_dir = key.trim().to_string();
        if !work_dir.is_empty() {
            items.push(work_dir);
        }
    }
    Ok(items)
}

fn project_display_name(work_dir: &str) -> String {
    Path::new(work_dir)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(work_dir)
        .to_string()
}

fn table_string(table: &Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(Item::as_value)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn read_weixin_fields(project: &Table) -> (String, String, bool) {
    let Some(platforms) = project.get("platforms").and_then(Item::as_array_of_tables) else {
        return (String::new(), String::new(), false);
    };
    for platform in platforms.iter() {
        if table_string(platform, "type").as_deref() == Some("weixin") {
            let options = platform.get("options").and_then(Item::as_table);
            let has_session = options.is_some_and(|table| {
                table_string(table, "token").is_some()
                    && table_string(table, "account_id").is_some()
                    && table_string(table, "base_url").is_some()
            });
            return (
                table_string(platform, "allow_from")
                    .or_else(|| options.and_then(|table| table_string(table, "allow_from")))
                    .unwrap_or_default(),
                table_string(platform, "admin_from")
                    .or_else(|| options.and_then(|table| table_string(table, "admin_from")))
                    .unwrap_or_default(),
                has_session,
            );
        }
    }
    (String::new(), String::new(), false)
}

fn find_cc_connect_path() -> Option<String> {
    let managed = managed_cc_connect_binary_path().ok()?;
    if managed.exists() {
        return Some(managed.display().to_string());
    }
    None
}

fn read_cc_connect_version(path: Option<&str>) -> Result<Option<String>> {
    let binary = path.ok_or_else(|| anyhow!("未检测到微信连接服务"))?;
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .with_context(|| format!("执行 {binary} --version 失败"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned))
}

fn read_cc_connect_version_cached(path: Option<&str>) -> Result<Option<String>> {
    if let Some(cached) = WECHAT_ENGINE_VERSION_CACHE.get() {
        return Ok(cached.clone());
    }
    let version = read_cc_connect_version(path)?;
    let _ = WECHAT_ENGINE_VERSION_CACHE.set(version.clone());
    Ok(version)
}

fn read_bridge_status_text(path: Option<&str>) -> Result<String> {
    let Some(binary) = path else {
        return Ok("未安装微信连接服务".to_string());
    };
    let bridge_running = read_pid_file(&managed_cc_connect_bridge_pid_path()?)
        .map(|pid| process_alive(pid))
        .unwrap_or(false);
    let setup_running = read_pid_file(&managed_cc_connect_setup_pid_path()?)
        .map(|pid| process_alive(pid))
        .unwrap_or(false);
    let version = read_cc_connect_version(Some(binary))?.unwrap_or_else(|| "unknown".to_string());
    let qr_exists = bridge_qr_image_path()?.exists();
    let bridge_log = managed_cc_connect_bridge_log_path()?;
    let bridge_log_hint = if bridge_log.exists() {
        format!("服务日志：{}", bridge_log.display())
    } else {
        "服务日志：尚未生成".to_string()
    };
    Ok(format!(
        "服务程序：{}\n版本：{}\n微信服务：{}\n登录流程：{}\n二维码：{}\n{}",
        binary,
        version,
        if bridge_running {
            "运行中"
        } else {
            "未运行"
        },
        if setup_running {
            "运行中"
        } else {
            "未运行"
        },
        if qr_exists { "已生成" } else { "未生成" },
        bridge_log_hint
    ))
}

fn suggest_project_name() -> String {
    "codex-tools-wechat".to_string()
}

fn build_project_snippet(project_name: &str) -> String {
    format!(
        r#"[[projects]]
name = "{project_name}"

[projects.agent]
type = "codex"

[projects.agent.options]
work_dir = "/path/to/your/project"

[[projects.platforms]]
type = "weixin"

[projects.platforms.options]
allow_from = "*"

# 建议后续补上：
# admin_from = "your_weixin_user_id"
# model = "gpt-5.5"
"#
    )
}

fn normalize_project_draft(draft: BridgeProjectDraft) -> Result<BridgeProjectDraft> {
    let name = draft.name.trim().to_string();
    let work_dir = draft.work_dir.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("项目名不能为空"));
    }
    if work_dir.is_empty() {
        return Err(anyhow!("work_dir 不能为空"));
    }
    Ok(BridgeProjectDraft {
        name,
        work_dir,
        allow_from: nonempty_or_default(draft.allow_from, "*"),
        admin_from: draft.admin_from.trim().to_string(),
        model: draft.model.trim().to_string(),
        permission_mode: normalize_permission_mode(&draft.permission_mode),
    })
}

fn normalize_permission_mode(mode: &str) -> String {
    match mode.trim() {
        "plan" | "acceptEdits" | "bypassPermissions" => mode.trim().to_string(),
        "edit" | "accept-edits" | "accept_edits" => "acceptEdits".to_string(),
        "yolo" | "bypass-permissions" | "bypass_permissions" => "bypassPermissions".to_string(),
        _ => "plan".to_string(),
    }
}

fn nonempty_or_default(value_text: String, fallback: &str) -> String {
    let trimmed = value_text.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn write_project_table(
    project: &mut Table,
    draft: &BridgeProjectDraft,
    session: Option<WeixinSession>,
) {
    project["name"] = value(draft.name.clone());

    let mut agent = Table::new();
    agent["type"] = value("codex");
    let mut agent_options = Table::new();
    agent_options["work_dir"] = value(draft.work_dir.clone());
    if !draft.model.is_empty() {
        agent_options["model"] = value(draft.model.clone());
    }
    agent_options["mode"] = value(draft.permission_mode.clone());
    agent["options"] = Item::Table(agent_options);
    project["agent"] = Item::Table(agent);

    let mut weixin = Table::new();
    weixin["type"] = value("weixin");
    let mut weixin_options = Table::new();
    weixin_options["allow_from"] = value(draft.allow_from.clone());
    if !draft.admin_from.is_empty() {
        weixin_options["admin_from"] = value(draft.admin_from.clone());
    }
    if let Some(session) = session {
        weixin_options["token"] = value(session.token);
        weixin_options["account_id"] = value(session.account_id);
        weixin_options["base_url"] = value(session.base_url);
    }
    weixin["options"] = Item::Table(weixin_options);

    let mut platforms = ArrayOfTables::new();
    platforms.push(weixin);
    project["platforms"] = Item::ArrayOfTables(platforms);
}

fn find_any_weixin_session(document: &DocumentMut) -> Option<WeixinSession> {
    let projects = document
        .get("projects")
        .and_then(Item::as_array_of_tables)?;
    for project in projects.iter() {
        if let Some(session) = find_weixin_session(project) {
            return Some(session);
        }
    }
    None
}

fn find_session_for_project(document: &DocumentMut, project_name: &str) -> Option<WeixinSession> {
    let projects = document
        .get("projects")
        .and_then(Item::as_array_of_tables)?;
    for project in projects.iter() {
        if table_string(project, "name").as_deref() == Some(project_name) {
            return find_weixin_session(project);
        }
    }
    None
}

fn find_weixin_session(project: &Table) -> Option<WeixinSession> {
    let platforms = project
        .get("platforms")
        .and_then(Item::as_array_of_tables)?;
    for platform in platforms.iter() {
        if table_string(platform, "type").as_deref() != Some("weixin") {
            continue;
        }
        let options = platform.get("options").and_then(Item::as_table)?;
        let token = table_string(options, "token")?;
        let account_id = table_string(options, "account_id")?;
        let base_url = table_string(options, "base_url")?;
        return Some(WeixinSession {
            token,
            account_id,
            base_url,
        });
    }
    None
}

fn read_weixin_communication_state(projects: &[BridgeProjectStatus]) -> Result<(bool, String)> {
    let Some(project) = projects.iter().find(|project| project.has_weixin_session) else {
        return Ok((false, "等待微信扫码登录".to_string()));
    };
    let Some(account_id) = read_project_account_id(project)? else {
        return Ok((false, "等待微信登录凭据写入".to_string()));
    };
    let context_tokens_path = super::paths::cc_connect_dir()?
        .join("weixin")
        .join(&project.name)
        .join(&account_id)
        .join("context_tokens.json");
    if !context_tokens_path.exists() {
        return Ok((false, "等待你在微信里发送第一条消息".to_string()));
    }
    let content = fs::read_to_string(&context_tokens_path)
        .with_context(|| format!("读取微信会话状态失败：{}", context_tokens_path.display()))?;
    let value: Value = serde_json::from_str(&content).unwrap_or(Value::Null);
    let ready = value.as_object().is_some_and(|object| !object.is_empty());
    Ok(if ready {
        (true, "已建立微信通信会话".to_string())
    } else {
        (false, "等待你在微信里发送第一条消息".to_string())
    })
}

fn read_project_account_id(project: &BridgeProjectStatus) -> Result<Option<String>> {
    let config_path = cc_connect_config_path()?;
    if !config_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(config_path)?;
    let document = content.parse::<DocumentMut>()?;
    let Some(project_items) = document.get("projects").and_then(Item::as_array_of_tables) else {
        return Ok(None);
    };
    for item in project_items.iter() {
        if table_string(item, "name").as_deref() != Some(project.name.as_str()) {
            continue;
        }
        if let Some(session) = find_weixin_session(item) {
            return Ok(Some(session.account_id));
        }
    }
    Ok(None)
}

fn ensure_managed_cc_connect_installed_blocking() -> Result<()> {
    let binary_path = managed_cc_connect_binary_path()?;
    if binary_path.exists() {
        return Ok(());
    }
    let bin_dir = managed_cc_connect_bin_dir()?;
    fs::create_dir_all(&bin_dir)?;
    install_embedded_wechat_engine(&binary_path)?;

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&binary_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary_path, perms)?;
        let _ = Command::new("xattr")
            .args([
                "-d",
                "com.apple.quarantine",
                &binary_path.display().to_string(),
            ])
            .status();
    }

    Ok(())
}

fn install_embedded_wechat_engine(binary_path: &Path) -> Result<()> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        fs::write(binary_path, EMBEDDED_WECHAT_ENGINE)
            .with_context(|| format!("写入内置微信连接引擎失败：{}", binary_path.display()))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow!(
        "当前平台暂未内置微信连接引擎，请先补充对应平台资源。版本：{}",
        WECHAT_ENGINE_VERSION
    ))
}

fn spawn_managed_process(
    binary: &Path,
    args: &[&str],
    pid_path: &Path,
    log_path: &Path,
) -> Result<()> {
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let stderr = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let mut command = Command::new(binary);
    apply_tool_path_env(&mut command);
    let child = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("启动 {} 失败", binary.display()))?;
    fs::write(pid_path, child.id().to_string())?;
    Ok(())
}

fn apply_tool_path_env(command: &mut Command) {
    command.env("PATH", build_tool_path_env());
}

fn build_tool_path_env() -> String {
    let mut paths = Vec::<PathBuf>::new();
    if let Some(current_path) = env::var_os("PATH") {
        paths.extend(env::split_paths(&current_path));
    }

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local").join("bin"));
        paths.push(home.join(".npm-global").join("bin"));
        paths.push(home.join(".cargo").join("bin"));
        let nvm_versions = home.join(".nvm").join("versions").join("node");
        if let Ok(entries) = fs::read_dir(nvm_versions) {
            for entry in entries.flatten() {
                paths.push(entry.path().join("bin"));
            }
        }
    }

    paths.push(PathBuf::from("/opt/homebrew/bin"));
    paths.push(PathBuf::from("/usr/local/bin"));
    paths.push(PathBuf::from("/usr/bin"));
    paths.push(PathBuf::from("/bin"));
    paths.push(PathBuf::from("/usr/sbin"));
    paths.push(PathBuf::from("/sbin"));

    let mut seen = BTreeSet::new();
    let deduped = paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect::<Vec<_>>();
    env::join_paths(deduped)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn stop_managed_process(pid_path: &Path, _label: &str) -> Result<bool> {
    let Some(pid) = read_pid_file(pid_path) else {
        return Ok(false);
    };
    if !process_alive(pid) {
        let _ = fs::remove_file(pid_path);
        return Ok(false);
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .context("停止进程失败")?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .context("停止进程失败")?;
        for _ in 0..20 {
            if !process_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(150));
        }
        if process_alive(pid) {
            let _ = Command::new("kill")
                .arg("-KILL")
                .arg(pid.to_string())
                .status();
        }
    }
    let _ = fs::remove_file(pid_path);
    Ok(true)
}

fn read_pid_file(path: &Path) -> Option<u32> {
    let content = fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok()
}

fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        return Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false);
    }
    #[cfg(not(target_os = "windows"))]
    {
        return Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn read_png_data_url(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)
        .with_context(|| format!("读取二维码元数据失败：{}", path.display()))?
        .modified()
        .with_context(|| format!("读取二维码修改时间失败：{}", path.display()))?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cache = QR_DATA_URL_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_modified, cached_value)) = guard.as_ref() {
            if *cached_modified == modified {
                return Ok(cached_value.clone());
            }
        }
    }
    let bytes = fs::read(path).with_context(|| format!("读取二维码失败：{}", path.display()))?;
    let data_url = format!("data:image/png;base64,{}", STANDARD.encode(bytes));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((modified, data_url.clone()));
    }
    Ok(data_url)
}
