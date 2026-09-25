mod audit;
mod commands;
mod discovery;
mod model;
mod report;
mod safety;
mod state;
mod storage;
mod stream;

use tauri::Manager;

use state::ScanState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ScanState::default())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = storage::init(&dir.join("camera-scanner.db"))?;
            let whitelist = storage::whitelist_load(&conn)?;

            let state = app.state::<ScanState>();
            *state.whitelist.lock().unwrap() = whitelist;
            *state.db.lock().unwrap() = Some(conn);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_start,
            commands::scan_cancel,
            commands::device_list,
            commands::history_devices,
            commands::whitelist_list,
            commands::whitelist_add,
            commands::whitelist_remove,
            commands::results_export,
            commands::audit_device,
            commands::stream_open,
            commands::stream_close,
            commands::stream_list,
            commands::consent_status,
            commands::consent_accept,
            commands::audit_log_list,
            commands::report_generate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
