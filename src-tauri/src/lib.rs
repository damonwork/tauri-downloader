mod browser_bridge;
mod commands;
mod downloads;

use downloads::manager::DownloadManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let manager =
                tauri::async_runtime::block_on(DownloadManager::load(app.handle().clone()))?;
            let integration =
                tauri::async_runtime::block_on(browser_bridge::start(manager.clone()));
            manager.clone().start_scheduler();
            app.manage(manager);
            app.manage(integration);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_browser_integration,
            commands::reveal_download,
            commands::add_download,
            commands::control_download,
            commands::replace_download_source,
            commands::update_settings,
            commands::save_proxy,
            commands::remove_proxy,
            commands::check_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Fluxor");
}
