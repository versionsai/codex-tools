mod services;

use services::{
    codex::{
        delete_provider_impl, fetch_provider_models_impl, get_provider_impl, get_summary_impl,
        list_providers_impl, save_provider_impl, switch_provider_impl, unify_thread_provider_impl,
    },
    webdav::{load_webdav_config_impl, save_webdav_config_impl, WebDavConfig},
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuEvent},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager, RunEvent, Runtime, WindowEvent,
};

const TRAY_ID: &str = "codex-tools-tray";
const TRAY_OPEN: &str = "tray:open";
const TRAY_REFRESH: &str = "tray:refresh";
const TRAY_QUIT: &str = "tray:quit";
const TRAY_PROVIDER_PREFIX: &str = "tray:provider:";

#[tauri::command]
fn get_summary() -> Result<services::codex::Summary, String> {
    get_summary_impl().map_err(|err| err.to_string())
}

#[tauri::command]
fn list_providers() -> Result<Vec<services::codex::ProviderConfig>, String> {
    list_providers_impl().map_err(|err| err.to_string())
}

#[tauri::command]
fn get_provider(provider_id: String) -> Result<services::codex::ProviderConfig, String> {
    get_provider_impl(&provider_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn save_provider(provider: services::codex::ProviderConfig) -> Result<(), String> {
    save_provider_impl(provider).map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
fn delete_provider(provider_id: String) -> Result<(), String> {
    delete_provider_impl(&provider_id).map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
fn switch_provider(provider_id: String) -> Result<(), String> {
    switch_provider_impl(&provider_id).map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
async fn fetch_provider_models(
    provider: services::codex::ProviderConfig,
) -> Result<Vec<services::codex::ModelOption>, String> {
    fetch_provider_models_impl(provider)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn unify_thread_provider() -> Result<String, String> {
    unify_thread_provider_impl().map_err(|err| err.to_string())
}

#[tauri::command]
fn load_webdav_config() -> Result<WebDavConfig, String> {
    load_webdav_config_impl().map_err(|err| err.to_string())
}

#[tauri::command]
fn save_webdav_config(config: WebDavConfig) -> Result<(), String> {
    save_webdav_config_impl(config).map_err(|err| err.to_string())
}

#[tauri::command]
fn pull_threads() -> Result<String, String> {
    Ok("拉取线程接口已预留，下一步迁移 WebDAV 下载逻辑".to_string())
}

#[tauri::command]
fn push_threads() -> Result<String, String> {
    Ok("推送线程接口已预留，下一步迁移 WebDAV 上传逻辑".to_string())
}

fn refresh_tray_menu_for_app() -> tauri::Result<()> {
    let Some(app) = APP_HANDLE.get() else {
        return Ok(());
    };
    refresh_tray_menu(app)
}

fn build_tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let current = get_summary_impl()
        .map(|summary| summary.provider)
        .unwrap_or_else(|_| "unknown".to_string());
    let providers = list_providers_impl().unwrap_or_default();
    let mut builder = MenuBuilder::new(app)
        .text("tray:title", format!("当前 Provider：{}", current))
        .separator();
    for provider in providers {
        let checked = provider.id == current;
        let label = if checked {
            format!("✓ {}", provider.id)
        } else {
            format!("  {}", provider.id)
        };
        builder = builder.text(format!("{}{}", TRAY_PROVIDER_PREFIX, provider.id), label);
    }
    builder
        .separator()
        .text(TRAY_OPEN, "打开 Codex Tools")
        .text(TRAY_REFRESH, "刷新 Provider 列表")
        .separator()
        .text(TRAY_QUIT, "退出")
        .build()
}

fn refresh_tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let menu = build_tray_menu(app)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

fn handle_tray_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().0.as_str();
    if id == TRAY_OPEN {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
    if id == TRAY_REFRESH {
        let _ = refresh_tray_menu(app);
        return;
    }
    if id == TRAY_QUIT {
        SHOULD_QUIT.store(true, Ordering::SeqCst);
        app.exit(0);
        return;
    }
    if let Some(provider_id) = id.strip_prefix(TRAY_PROVIDER_PREFIX) {
        if switch_provider_impl(provider_id).is_ok() {
            let _ = unify_thread_provider_impl();
        }
        let _ = refresh_tray_menu(app);
    }
}

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<TrayIcon<R>> {
    let handle = app.handle().clone();
    let menu = build_tray_menu(&handle)?;
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Codex Tools")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_tray_menu_event)
        .build(app)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let _ = setup_tray(app)?;
            APP_HANDLE.set(app.handle().clone()).ok();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_summary,
            list_providers,
            get_provider,
            save_provider,
            delete_provider,
            switch_provider,
            fetch_provider_models,
            unify_thread_provider,
            load_webdav_config,
            save_webdav_config,
            pull_threads,
            push_threads
        ])
        .build(tauri::generate_context!())
        .expect("error while building Codex Tools");
    app.run(|app, event| match event {
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        RunEvent::ExitRequested { api, .. } => {
            if !SHOULD_QUIT.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
        _ => {}
    });
}

static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
static SHOULD_QUIT: AtomicBool = AtomicBool::new(false);
