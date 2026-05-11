use super::paths::{codex_dir, webdav_config_path};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

const SYNC_ROOTS: [&str; 2] = ["sessions", "archived_sessions"];
const SESSION_INDEX: &str = "session_index.jsonl";
const MANIFEST_FILE: &str = "codex_tools_thread_manifest.json";

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SyncManifest {
    version: u8,
    updated_at: String,
    threads: Vec<ManifestThread>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestThread {
    project_name: String,
    thread_id: String,
    relative_path: String,
    archived: bool,
    updated_at: Option<i64>,
    size: Option<u64>,
}

#[derive(Debug, Clone)]
struct ThreadRecord {
    project_name: String,
    thread_id: String,
    relative_path: String,
    local_path: PathBuf,
    archived: bool,
    updated_at: Option<i64>,
    modified_at: Option<std::time::SystemTime>,
    size: Option<u64>,
}

#[derive(Debug, Clone)]
struct RemoteEntry {
    relative_path: String,
    is_directory: bool,
    last_modified: Option<DateTime<Utc>>,
    size: Option<u64>,
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

pub async fn push_threads_impl() -> Result<String> {
    let config = load_webdav_config_impl()?;
    let client = webdav_client(&config)?;
    let base_url = normalized_base_url(&config)?;
    let codex = codex_dir()?;
    let local_threads = local_thread_map(&codex)?;
    if local_threads.is_empty() {
        return Err(anyhow!(
            "未找到可推送的本地线程，请确认 Codex 目录是否正确：{}",
            codex.display()
        ));
    }
    ensure_remote_roots(&client, &base_url).await?;
    let mut remote_manifest = load_remote_manifest(&client, &base_url).await?;
    let remote_entries = remote_file_map(&client, &base_url).await?;
    let remote_by_identity = manifest_identity_map(&remote_manifest);

    let mut uploaded = 0usize;
    let mut skipped = 0usize;
    let mut relocated = 0usize;
    let mut ensured_remote_dirs = HashSet::new();
    for entry in remote_entries.values().filter(|entry| !entry.is_directory) {
        if let Some(parent) = entry.relative_path.rsplit_once('/').map(|(parent, _)| parent) {
            ensured_remote_dirs.insert(parent.to_string());
        }
    }

    for thread in local_threads.values() {
        let identity = identity_key(&thread.project_name, &thread.thread_id);
        let existing_remote_path = remote_by_identity
            .get(&identity)
            .cloned()
            .or_else(|| manifest_path_by_thread_id(&remote_manifest, &thread.thread_id));
        let remote_path = thread.relative_path.clone();
        if existing_remote_path.as_deref().is_some_and(|path| path != remote_path)
            && existing_remote_path
                .as_deref()
                .is_some_and(|path| remote_entries.contains_key(path))
        {
            relocated += 1;
        }

        let should_skip = remote_entries
            .get(&remote_path)
            .and_then(|remote| {
                let local_mtime = thread.modified_at?;
                let remote_mtime = remote.last_modified?;
                Some(remote_mtime.timestamp() >= system_time_secs(local_mtime) && remote.size == thread.size)
            })
            .unwrap_or(false);
        if should_skip {
            skipped += 1;
            continue;
        }

        ensure_remote_directory_cached(
            &client,
            &base_url,
            parent_relative(&remote_path),
            &mut ensured_remote_dirs,
        )
        .await?;
        put_remote_file(&client, &base_url, &remote_path, fs::read(&thread.local_path)?).await?;
        uploaded += 1;
    }

    merge_local_threads_into_manifest(&mut remote_manifest, local_threads.values());
    save_remote_manifest(&client, &base_url, &remote_manifest).await?;
    if codex.join(SESSION_INDEX).exists() {
        put_remote_file(
            &client,
            &base_url,
            SESSION_INDEX,
            fs::read(codex.join(SESSION_INDEX))?,
        )
        .await?;
    }
    Ok(format!(
        "推送完成：上传 {} 个，跳过 {} 个，路径归并 {} 个，线程索引 {} 条",
        uploaded,
        skipped,
        relocated,
        remote_manifest.threads.len(),
    ))
}

pub async fn pull_threads_impl() -> Result<String> {
    let config = load_webdav_config_impl()?;
    let client = webdav_client(&config)?;
    let base_url = normalized_base_url(&config)?;
    let codex = codex_dir()?;
    let local_threads = local_thread_map(&codex)?;
    let manifest = load_remote_manifest(&client, &base_url).await?;
    let remote_entries = remote_file_map(&client, &base_url).await?;

    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut merged_by_identity = 0usize;

    for remote in &manifest.threads {
        let identity = identity_key(&remote.project_name, &remote.thread_id);
        let target_rel = local_threads
            .get(&identity)
            .or_else(|| find_local_by_thread_id(&local_threads, &remote.thread_id))
            .map(|thread| thread.relative_path.clone())
            .unwrap_or_else(|| remote.relative_path.clone());
        if target_rel != remote.relative_path {
            merged_by_identity += 1;
        }

        let local_path = codex.join(path_from_relative(&target_rel));
        let should_skip = if local_path.exists() {
            let local_modified = fs::metadata(&local_path).ok().and_then(|meta| meta.modified().ok());
            remote_entries
                .get(&remote.relative_path)
                .and_then(|entry| Some(entry.last_modified?.timestamp() <= system_time_secs(local_modified?)))
                .unwrap_or(false)
        } else {
            false
        };
        if should_skip {
            skipped += 1;
            continue;
        }

        let data = get_remote_file(&client, &base_url, &remote.relative_path).await?;
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if local_path.exists() {
            backup_local_file(&codex, &local_path)?;
        }
        fs::write(local_path, data)?;
        downloaded += 1;
    }

    Ok(format!(
        "拉取完成：下载 {} 个，跳过 {} 个，按线程 ID 合并 {} 个，远端线程 {} 条",
        downloaded,
        skipped,
        merged_by_identity,
        manifest.threads.len()
    ))
}

fn webdav_client(config: &WebDavConfig) -> Result<Client> {
    if config.base_url.trim().is_empty() {
        return Err(anyhow!("请先填写 WebDAV 服务地址"));
    }
    if config.username.trim().is_empty() {
        return Err(anyhow!("请先填写 WebDAV 用户名"));
    }
    Ok(Client::builder()
        .danger_accept_invalid_certs(!config.verify_tls)
        .build()?)
}

fn normalized_base_url(config: &WebDavConfig) -> Result<String> {
    let trimmed = config.base_url.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("WebDAV 服务地址为空"));
    }
    let mut url = reqwest::Url::parse(trimmed)?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    let path = url.path().trim_matches('/');
    let last = path.rsplit('/').next();
    if matches!(last, Some("sessions" | "archived_sessions")) {
        let parent = path
            .rsplit_once('/')
            .map(|(parent, _)| format!("/{}/", parent))
            .unwrap_or_else(|| "/".to_string());
        url.set_path(&parent);
    }
    Ok(url.to_string())
}

fn local_thread_map(codex: &Path) -> Result<HashMap<String, ThreadRecord>> {
    let project_by_thread = load_project_names(codex)?;
    let mut threads = HashMap::new();
    for root in SYNC_ROOTS {
        let root_path = codex.join(root);
        if !root_path.exists() {
            continue;
        }
        for entry in WalkDir::new(root_path).into_iter().filter_map(Result::ok) {
            let path = entry.into_path();
            if !is_rollout_file(&path) {
                continue;
            }
            let Some(thread_id) = thread_id_from_path(&path) else {
                continue;
            };
            let relative_path = relative_path(codex, &path)?;
            let project_name = project_by_thread
                .get(&thread_id)
                .cloned()
                .unwrap_or_default();
            let metadata = fs::metadata(&path).ok();
            let record = ThreadRecord {
                project_name,
                thread_id: thread_id.clone(),
                archived: relative_path.starts_with("archived_sessions/"),
                relative_path,
                local_path: path,
                updated_at: None,
                modified_at: metadata.as_ref().and_then(|meta| meta.modified().ok()),
                size: metadata.map(|meta| meta.len()),
            };
            threads.insert(identity_key(&record.project_name, &record.thread_id), record);
        }
    }
    Ok(threads)
}

fn load_project_names(codex: &Path) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let index_path = codex.join(SESSION_INDEX);
    if !index_path.exists() {
        return Ok(map);
    }
    let content = fs::read_to_string(index_path)?;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let project_name = value
            .get("project_name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| {
                value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .and_then(|cwd| Path::new(cwd).file_name())
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_default();
        map.insert(id.to_string(), project_name);
    }
    Ok(map)
}

fn merge_local_threads_into_manifest<'a>(
    manifest: &mut SyncManifest,
    local_threads: impl Iterator<Item = &'a ThreadRecord>,
) {
    let mut map: HashMap<String, ManifestThread> = manifest
        .threads
        .drain(..)
        .map(|thread| (identity_key(&thread.project_name, &thread.thread_id), thread))
        .collect();
    for thread in local_threads {
        map.retain(|_, existing| existing.thread_id != thread.thread_id);
        map.insert(
            identity_key(&thread.project_name, &thread.thread_id),
            ManifestThread {
                project_name: thread.project_name.clone(),
                thread_id: thread.thread_id.clone(),
                relative_path: thread.relative_path.clone(),
                archived: thread.archived,
                updated_at: thread.updated_at,
                size: thread.size,
            },
        );
    }
    let mut threads = map.into_values().collect::<Vec<_>>();
    threads.sort_by(|left, right| {
        left.project_name
            .cmp(&right.project_name)
            .then_with(|| left.thread_id.cmp(&right.thread_id))
    });
    manifest.version = 1;
    manifest.updated_at = Utc::now().to_rfc3339();
    manifest.threads = threads;
}

fn manifest_identity_map(manifest: &SyncManifest) -> HashMap<String, String> {
    manifest
        .threads
        .iter()
        .map(|thread| {
            (
                identity_key(&thread.project_name, &thread.thread_id),
                thread.relative_path.clone(),
            )
        })
        .collect()
}

fn manifest_path_by_thread_id(manifest: &SyncManifest, thread_id: &str) -> Option<String> {
    manifest
        .threads
        .iter()
        .find(|thread| thread.thread_id == thread_id)
        .map(|thread| thread.relative_path.clone())
}

fn find_local_by_thread_id<'a>(
    local_threads: &'a HashMap<String, ThreadRecord>,
    thread_id: &str,
) -> Option<&'a ThreadRecord> {
    local_threads
        .values()
        .find(|thread| thread.thread_id == thread_id)
}

async fn load_remote_manifest(client: &Client, base_url: &str) -> Result<SyncManifest> {
    match get_remote_file(client, base_url, MANIFEST_FILE).await {
        Ok(data) => Ok(serde_json::from_slice(&data)?),
        Err(_) => {
            let entries = remote_file_map(client, base_url).await?;
            let mut manifest = SyncManifest {
                version: 1,
                updated_at: Utc::now().to_rfc3339(),
                threads: entries
                    .values()
                    .filter(|entry| !entry.is_directory)
                    .filter_map(|entry| {
                        let thread_id = thread_id_from_relative(&entry.relative_path)?;
                        Some(ManifestThread {
                            project_name: String::new(),
                            thread_id,
                            archived: entry.relative_path.starts_with("archived_sessions/"),
                            relative_path: entry.relative_path.clone(),
                            updated_at: entry.last_modified.map(|time| time.timestamp()),
                            size: entry.size,
                        })
                    })
                    .collect(),
            };
            manifest.threads.sort_by(|left, right| left.thread_id.cmp(&right.thread_id));
            Ok(manifest)
        }
    }
}

async fn save_remote_manifest(client: &Client, base_url: &str, manifest: &SyncManifest) -> Result<()> {
    put_remote_file(
        client,
        base_url,
        MANIFEST_FILE,
        serde_json::to_vec_pretty(manifest)?,
    )
    .await
}

async fn remote_file_map(client: &Client, base_url: &str) -> Result<HashMap<String, RemoteEntry>> {
    let mut entries = HashMap::new();
    for root in SYNC_ROOTS {
        match list_remote_tree(client, base_url, root).await {
            Ok(tree) => {
                for entry in tree.into_iter().filter(|entry| !entry.is_directory) {
                    entries.insert(entry.relative_path.clone(), entry);
                }
            }
            Err(error) if error.to_string().contains("HTTP 404") => {}
            Err(error) => return Err(error),
        }
    }
    if let Ok(entry) = stat_remote_file(client, base_url, SESSION_INDEX).await {
        entries.insert(entry.relative_path.clone(), entry);
    }
    Ok(entries)
}

async fn ensure_remote_roots(client: &Client, base_url: &str) -> Result<()> {
    for root in SYNC_ROOTS {
        ensure_remote_directory(client, base_url, root).await?;
    }
    Ok(())
}

async fn list_remote_tree(client: &Client, base_url: &str, relative_root: &str) -> Result<Vec<RemoteEntry>> {
    let mut results = Vec::new();
    let mut stack = vec![relative_root.to_string()];
    while let Some(current) = stack.pop() {
        let xml = propfind(client, base_url, &(current.clone() + "/"), "1").await?;
        for entry in parse_multistatus(base_url, &xml, &current)? {
            if entry.is_directory {
                stack.push(entry.relative_path.clone());
            }
            results.push(entry);
        }
    }
    Ok(results)
}

async fn stat_remote_file(client: &Client, base_url: &str, relative_path: &str) -> Result<RemoteEntry> {
    let xml = propfind(client, base_url, relative_path, "0").await?;
    parse_multistatus(base_url, &xml, "")?
        .into_iter()
        .find(|entry| entry.relative_path == relative_path)
        .ok_or_else(|| anyhow!("远端文件不存在：{}", relative_path))
}

async fn propfind(client: &Client, base_url: &str, relative_path: &str, depth: &str) -> Result<String> {
    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:resourcetype />
    <d:getcontentlength />
    <d:getlastmodified />
  </d:prop>
</d:propfind>"#;
    let response = authed_request(client, Method::from_bytes(b"PROPFIND")?, base_url, relative_path)?
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_string())
        .send()
        .await?;
    let status = response.status();
    if !matches!(status, StatusCode::MULTI_STATUS | StatusCode::OK) {
        return Err(anyhow!("WebDAV PROPFIND 失败：HTTP {}", status.as_u16()));
    }
    Ok(response.text().await?)
}

async fn ensure_remote_directory(client: &Client, base_url: &str, relative_dir: &str) -> Result<()> {
    let normalized = relative_dir.trim_matches('/');
    if normalized.is_empty() || normalized == "." {
        return Ok(());
    }
    let mut current = String::new();
    for part in normalized.split('/') {
        current = if current.is_empty() {
            part.to_string()
        } else {
            format!("{}/{}", current, part)
        };
        let response = authed_request(client, Method::from_bytes(b"MKCOL")?, base_url, &(current.clone() + "/"))?
            .send()
            .await?;
        if !matches!(
            response.status(),
            StatusCode::CREATED | StatusCode::METHOD_NOT_ALLOWED | StatusCode::MOVED_PERMANENTLY | StatusCode::OK
        ) {
            return Err(anyhow!("创建远端目录失败：{} HTTP {}", current, response.status().as_u16()));
        }
    }
    Ok(())
}

async fn ensure_remote_directory_cached(
    client: &Client,
    base_url: &str,
    relative_dir: &str,
    ensured: &mut HashSet<String>,
) -> Result<()> {
    let normalized = relative_dir.trim_matches('/');
    if normalized.is_empty() || normalized == "." || ensured.contains(normalized) {
        return Ok(());
    }

    let mut current = String::new();
    let mut missing = Vec::new();
    for part in normalized.split('/') {
        current = if current.is_empty() {
            part.to_string()
        } else {
            format!("{}/{}", current, part)
        };
        if !ensured.contains(&current) {
            missing.push(current.clone());
        }
    }

    for dir in missing {
        ensure_remote_directory(client, base_url, &dir).await?;
        ensured.insert(dir);
    }
    ensured.insert(normalized.to_string());
    Ok(())
}

async fn put_remote_file(client: &Client, base_url: &str, relative_path: &str, data: Vec<u8>) -> Result<()> {
    let response = authed_request(client, Method::PUT, base_url, relative_path)?
        .body(data)
        .send()
        .await?;
    if !matches!(response.status(), StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT) {
        return Err(anyhow!("上传远端文件失败：{} HTTP {}", relative_path, response.status().as_u16()));
    }
    Ok(())
}

async fn get_remote_file(client: &Client, base_url: &str, relative_path: &str) -> Result<Vec<u8>> {
    let response = authed_request(client, Method::GET, base_url, relative_path)?
        .send()
        .await?;
    let status = response.status();
    if status != StatusCode::OK {
        return Err(anyhow!("下载远端文件失败：{} HTTP {}", relative_path, status.as_u16()));
    }
    Ok(response.bytes().await?.to_vec())
}

fn authed_request(
    client: &Client,
    method: Method,
    base_url: &str,
    relative_path: &str,
) -> Result<reqwest::RequestBuilder> {
    let config = load_webdav_config_impl()?;
    Ok(client
        .request(method, join_url(base_url, relative_path)?)
        .basic_auth(config.username, Some(config.password)))
}

fn parse_multistatus(base_url: &str, xml: &str, target_relative: &str) -> Result<Vec<RemoteEntry>> {
    let value: Value = quick_xml_to_json(xml)?;
    let base_path = reqwest::Url::parse(base_url)?
        .path()
        .trim_matches('/')
        .to_string();
    let mut entries = Vec::new();
    collect_responses(&value, &mut |response| {
        if let Some(entry) = remote_entry_from_response(response, &base_path, target_relative) {
            entries.push(entry);
        }
    });
    Ok(entries)
}

fn quick_xml_to_json(xml: &str) -> Result<Value> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<(String, Value)> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                stack.push((name, Value::Object(Default::default())));
            }
            Ok(quick_xml::events::Event::Empty(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                attach_xml_value(&mut stack, name, Value::String(String::new()));
            }
            Ok(quick_xml::events::Event::Text(event)) => {
                if let Some((_, value)) = stack.last_mut() {
                    *value = Value::String(String::from_utf8_lossy(event.as_ref()).to_string());
                }
            }
            Ok(quick_xml::events::Event::End(_)) => {
                if let Some((name, value)) = stack.pop() {
                    if stack.is_empty() {
                        return Ok(value);
                    }
                    attach_xml_value(&mut stack, name, value);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(anyhow!("解析 WebDAV XML 失败：{}", error)),
            _ => {}
        }
    }
    Ok(Value::Null)
}

fn attach_xml_value(stack: &mut [(String, Value)], name: String, value: Value) {
    if let Some((_, Value::Object(parent))) = stack.last_mut() {
        let key = name
            .rsplit_once(':')
            .map(|(_, local)| local.to_string())
            .unwrap_or(name);
        match parent.get_mut(&key) {
            Some(Value::Array(items)) => items.push(value),
            Some(existing) => {
                let old = existing.take();
                *existing = Value::Array(vec![old, value]);
            }
            None => {
                parent.insert(key, value);
            }
        }
    }
}

fn collect_responses(value: &Value, callback: &mut impl FnMut(&Value)) {
    match value {
        Value::Object(map) => {
            if let Some(response) = map.get("response") {
                match response {
                    Value::Array(items) => items.iter().for_each(&mut *callback),
                    item => callback(item),
                }
            }
            for child in map.values() {
                collect_responses(child, callback);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_responses(item, callback);
            }
        }
        _ => {}
    }
}

fn remote_entry_from_response(response: &Value, base_path: &str, target_relative: &str) -> Option<RemoteEntry> {
    let href = response.get("href").and_then(Value::as_str)?;
    let href_path = reqwest::Url::parse(href)
        .map(|url| url.path().to_string())
        .unwrap_or_else(|_| href.to_string());
    let decoded = percent_decode(&href_path).trim_matches('/').to_string();
    let mut relative = decoded.clone();
    if !base_path.is_empty() && decoded.starts_with(base_path) {
        relative = decoded[base_path.len()..].trim_matches('/').to_string();
    }
    if relative.is_empty() || relative == target_relative.trim_matches('/') {
        return None;
    }
    let propstat = response.get("propstat")?;
    let prop = first_value(propstat).get("prop")?;
    let is_directory = prop
        .get("resourcetype")
        .is_some_and(|resource| resource.to_string().contains("collection"));
    let size = prop
        .get("getcontentlength")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok());
    let last_modified = prop
        .get("getlastmodified")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|time| time.with_timezone(&Utc));
    Some(RemoteEntry {
        relative_path: relative,
        is_directory,
        last_modified,
        size,
    })
}

fn first_value(value: &Value) -> &Value {
    value.as_array().and_then(|items| items.first()).unwrap_or(value)
}

fn join_url(base_url: &str, relative_path: &str) -> Result<String> {
    let base = reqwest::Url::parse(base_url)?;
    let trailing_slash = relative_path.ends_with('/');
    let encoded = relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/");
    let encoded = if trailing_slash && !encoded.is_empty() {
        format!("{}/", encoded)
    } else {
        encoded
    };
    Ok(base.join(&encoded)?.to_string())
}

fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .to_string()
}

fn identity_key(project_name: &str, thread_id: &str) -> String {
    format!("{}::{}", project_name.trim(), thread_id.trim())
}

fn is_rollout_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
}

fn thread_id_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(thread_id_from_filename)
}

fn thread_id_from_relative(relative_path: &str) -> Option<String> {
    Path::new(relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(thread_id_from_filename)
}

fn thread_id_from_filename(filename: &str) -> Option<String> {
    let stem = filename.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    if stem.len() >= 36 {
        return Some(stem[stem.len() - 36..].to_string());
    }
    Some(stem.to_string())
}

fn relative_path(root: &Path, file: &Path) -> Result<String> {
    Ok(file
        .strip_prefix(root)?
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn path_from_relative(relative_path: &str) -> PathBuf {
    relative_path.split('/').collect()
}

fn parent_relative(relative_path: &str) -> &str {
    relative_path.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("")
}

fn system_time_secs(time: std::time::SystemTime) -> i64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn backup_local_file(codex: &Path, local_path: &Path) -> Result<()> {
    let relative = relative_path(codex, local_path)?;
    let backup = codex
        .join("sync_backups")
        .join("pull-overwrite")
        .join(Utc::now().format("%Y%m%d-%H%M%S").to_string())
        .join(path_from_relative(&relative));
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(local_path, backup)?;
    Ok(())
}
