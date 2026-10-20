mod commands;
mod core;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_inspect,
            commands::vault::vault_create,
            commands::vault::vault_unlock,
            commands::vault::vault_save,
            commands::vault::vault_rewrap,
            commands::vault::vault_remove_slot,
            commands::vault::vault_lock,
            commands::generator::generate_password,
            commands::generator::generate_login,
            commands::storage::storage_read_file,
            commands::storage::storage_write_atomic,
            commands::storage::storage_exists,
            commands::storage::storage_mkdir,
            commands::storage::storage_stat,
            commands::storage::storage_list_dir,
            commands::storage::storage_remove_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PMS");
}
