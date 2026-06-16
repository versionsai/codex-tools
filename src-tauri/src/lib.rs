mod services;

use services::{
    bridge::{
        get_bridge_status_impl, install_cc_connect_impl, open_cc_connect_terminal_impl,
        open_wechat_setup_terminal_impl, pick_work_dir_impl, run_daemon_command_impl,
        save_bridge_project_impl,
    },
    codex::{
        delete_provider_impl, fetch_provider_models_impl, get_provider_impl, get_summary_impl,
        get_usage_summary_impl, list_providers_impl, repair_thread_visibility_index_impl,
        restart_codex_app_impl, save_provider_impl, switch_provider_impl,
        unify_thread_provider_impl,
    },
    webdav::{
        load_webdav_config_impl, pull_threads_impl, push_threads_impl, save_webdav_config_impl,
        WebDavConfig,
    },
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, WindowEvent};

const FRONTEND_REFRESH_EVENT: &str = "codex-tools-refresh";

#[tauri::command]
async fn get_summary() -> Result<services::codex::Summary, String> {
    tauri::async_runtime::spawn_blocking(get_summary_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn get_usage_summary() -> Result<services::codex::UsageSummary, String> {
    tauri::async_runtime::spawn_blocking(get_usage_summary_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn get_bridge_status() -> Result<services::bridge::BridgeStatus, String> {
    tauri::async_runtime::spawn_blocking(get_bridge_status_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn open_wechat_setup_terminal(project_name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || open_wechat_setup_terminal_impl(&project_name))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn open_cc_connect_terminal() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(open_cc_connect_terminal_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn install_cc_connect() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(install_cc_connect_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn run_bridge_daemon_command(action: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_daemon_command_impl(&action))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn save_bridge_project(
    project: services::bridge::BridgeProjectDraft,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || save_bridge_project_impl(project))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn pick_work_dir() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(pick_work_dir_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn list_providers() -> Result<Vec<services::codex::ProviderConfig>, String> {
    tauri::async_runtime::spawn_blocking(list_providers_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn get_provider(provider_id: String) -> Result<services::codex::ProviderConfig, String> {
    tauri::async_runtime::spawn_blocking(move || get_provider_impl(&provider_id))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn save_provider(provider: services::codex::ProviderConfig) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || save_provider_impl(provider))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
async fn delete_provider(provider_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || delete_provider_impl(&provider_id))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
async fn switch_provider(provider_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || switch_provider_impl(&provider_id))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    refresh_tray_menu_for_app().map_err(|err| err.to_string())
}

#[tauri::command]
async fn restart_codex_app() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(restart_codex_app_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
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
async fn unify_thread_provider() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(unify_thread_provider_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn repair_thread_visibility_index() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(repair_thread_visibility_index_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn load_webdav_config() -> Result<WebDavConfig, String> {
    tauri::async_runtime::spawn_blocking(load_webdav_config_impl)
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn save_webdav_config(config: WebDavConfig) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || save_webdav_config_impl(config))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn pull_threads() -> Result<String, String> {
    pull_threads_impl().await.map_err(|err| err.to_string())
}

#[tauri::command]
async fn push_threads() -> Result<String, String> {
    push_threads_impl().await.map_err(|err| err.to_string())
}

fn refresh_tray_menu_for_app() -> tauri::Result<()> {
    Ok(())
}

fn notify_frontend_refresh<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(FRONTEND_REFRESH_EVENT, ());
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_skip_taskbar(false);
                let _ = window.set_focus();
            }
            APP_HANDLE.set(app.handle().clone()).ok();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_summary,
            get_usage_summary,
            get_bridge_status,
            open_wechat_setup_terminal,
            open_cc_connect_terminal,
            install_cc_connect,
            run_bridge_daemon_command,
            save_bridge_project,
            pick_work_dir,
            list_providers,
            get_provider,
            save_provider,
            delete_provider,
            switch_provider,
            restart_codex_app,
            fetch_provider_models,
            unify_thread_provider,
            repair_thread_visibility_index,
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
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            notify_frontend_refresh(app);
        }
        _ => {}
    });
}

static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
