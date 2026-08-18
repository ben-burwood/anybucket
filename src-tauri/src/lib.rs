mod commands;
mod secret_store;

use tauri::Manager;
use tokio::sync::Mutex;

use anybucket_core::connections::ConnectionStore;
use anybucket_core::state::AppState;
use secret_store::KeyringStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Connection metadata lives under the OS app-config dir; secrets are
            // kept in the keychain (see connections module).
            let config_dir = app.path().app_config_dir()?;
            let store = ConnectionStore::load(&config_dir, Box::new(KeyringStore))?;
            app.manage(Mutex::new(AppState::new(store)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::get_active_connection,
            commands::save_connection,
            commands::delete_connection,
            commands::set_active_connection,
            commands::test_connection,
            commands::list_buckets,
            commands::list_objects,
            commands::head_object,
            commands::presign_get,
            commands::object_uris,
            commands::download_object,
            commands::object_exists,
            commands::expand_upload_paths,
            commands::upload_object,
            commands::create_folder,
            commands::delete_objects,
            commands::transfer_objects,
            commands::scan_bucket_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
