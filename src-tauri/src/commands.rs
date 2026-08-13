use std::sync::Arc;

use tauri::State;

use crate::browser_bridge::BrowserIntegration;
use crate::diagnostics::{DiagnosticSnapshot, Diagnostics};
use crate::downloads::{
    error::AppError,
    manager::DownloadManager,
    model::{
        AppSettings, AppSnapshot, CreateDownloadInput, DownloadAction, DownloadItem,
        DownloadSource, ProxyProfile,
    },
};

#[tauri::command]
pub async fn get_snapshot(
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<AppSnapshot, AppError> {
    Ok(manager.snapshot().await)
}

#[tauri::command]
pub fn get_browser_integration(integration: State<'_, BrowserIntegration>) -> BrowserIntegration {
    integration.inner().clone()
}

#[tauri::command]
pub async fn get_diagnostic_logs(
    diagnostics: State<'_, Arc<Diagnostics>>,
) -> Result<DiagnosticSnapshot, String> {
    Ok(diagnostics.snapshot().await)
}

#[tauri::command]
pub async fn clear_diagnostic_logs(diagnostics: State<'_, Arc<Diagnostics>>) -> Result<(), String> {
    diagnostics.clear().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reveal_download(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), AppError> {
    manager.reveal_download(&id).await
}

#[tauri::command]
pub async fn add_download(
    manager: State<'_, Arc<DownloadManager>>,
    input: CreateDownloadInput,
) -> Result<DownloadItem, AppError> {
    manager.add(input).await
}

#[tauri::command]
pub async fn control_download(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
    action: DownloadAction,
) -> Result<(), AppError> {
    manager.control(&id, action).await
}

#[tauri::command]
pub async fn replace_download_source(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
    source: DownloadSource,
) -> Result<(), AppError> {
    manager.replace_source(&id, source).await
}

#[tauri::command]
pub async fn update_settings(
    manager: State<'_, Arc<DownloadManager>>,
    settings: AppSettings,
) -> Result<(), AppError> {
    manager.update_settings(settings).await
}

#[tauri::command]
pub async fn save_proxy(
    manager: State<'_, Arc<DownloadManager>>,
    proxy: ProxyProfile,
) -> Result<(), AppError> {
    manager.save_proxy(proxy).await
}

#[tauri::command]
pub async fn remove_proxy(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), AppError> {
    manager.remove_proxy(&id).await
}

#[tauri::command]
pub async fn check_proxy(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), AppError> {
    manager.check_proxy(&id).await
}
