mod commands;
mod core;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(tauri_plugin_android_fs::init())
        .plugin(tauri_plugin_biometric::init());

    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_inspect,
            commands::vault::keyfile_kind,
            commands::vault::vault_create,
            commands::vault::vault_unlock,
            commands::vault::vault_save,
            commands::vault::vault_rewrap,
            commands::vault::vault_remove_slot,
            commands::vault::vault_lock,
            commands::generator::generate_password,
            commands::generator::generate_login,
            commands::attachments::attachment_seal,
            commands::attachments::attachment_open,
            commands::totp::totp_inspect,
            commands::totp::totp_code,
            commands::ssh::ssh_generate,
            commands::ssh::ssh_formats,
            commands::ssh::ssh_install,
            commands::ssh::ssh_check,
            commands::biometric::biometric_status,
            commands::biometric::biometric_enroll,
            commands::biometric::biometric_disable,
            commands::biometric::biometric_unlock,
            commands::share::share_encode,
            commands::share::share_decode,
            commands::import::import_parse,
            commands::pms_archive::pms_export,
            commands::pms_archive::pms_import,
            commands::app_icon::app_icon_set,
            commands::transfer::transfer_receive_start,
            commands::transfer::transfer_receive_stop,
            commands::transfer::transfer_scan,
            commands::transfer::transfer_send,
            commands::transfer::transfer_install,
            commands::storage::storage_read_file,
            commands::storage::storage_write_atomic,
            commands::storage::storage_exists,
            commands::storage::storage_mkdir,
            commands::storage::storage_stat,
            commands::storage::storage_list_dir,
            commands::storage::storage_remove_file,
            commands::storage::storage_remove_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PMS");
}
