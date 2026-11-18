mod commands;
mod core;

use tauri::Manager;

/// Stable surface for the external benchmark/tests in `src-tauri/tests`.
///
/// `core` is a private module on purpose; this re-export exposes only what the
/// performance suite needs to measure the real data paths without widening the
/// crate's public API.
#[doc(hidden)]
pub mod testing {
    pub use crate::core::attachments::{
        open_attachment, seal_attachment, seal_attachment_in_place, ATTACHMENT_HEADER_LEN,
        MAX_ATTACHMENT_BYTES,
    };
    pub use crate::core::crypto::{
        open_payload, seal_document_after, seal_payload, KdfProfile, UnlockSecretInput,
    };
    pub use crate::core::generator::{
        generate_passphrase, generate_password, LoginOptions, PassphraseOptions, PasswordOptions,
    };
    pub use crate::core::import;
    pub use crate::core::models::{
        AttachmentKind, AttachmentMeta, CipherSuite, Draft, DraftKind, Entry, EntryKind, SshKey,
        TotpAlgorithm, VaultData, VaultHeader, VaultSettings,
    };
    pub use crate::core::pms_archive::{export as archive_export, import as archive_import};
    pub use crate::core::share;
    pub use crate::core::totp;
    pub use crate::core::vault::{
        create_vault, save_vault, seal_document, unlock_vault, CreateVaultConfig, VaultSession,
    };
    pub use crate::core::vault_file::{
        parse_vault_file, serialize_vault_file, serialize_vault_parts,
    };
}

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_autostart::Builder::new().build());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(tauri_plugin_android_fs::init())
        .plugin(tauri_plugin_biometric::init());

    builder
        .manage(AppState::default())
        .setup(|app| {
            // Load the filesystem grants the user approved earlier and remember
            // where to store new ones. Without this the vault folder would have
            // to be re-picked on every launch.
            let handle = app.handle();
            let state: tauri::State<'_, AppState> = handle.state();
            // Only the cache directory is granted (archives are staged there).
            // The app data directory is *not* granted: it holds the grant store
            // and the biometric blob, both written by Rust directly.
            let cache = handle.path().app_cache_dir().ok();
            if let Some(dir) = cache.as_deref() {
                let _ = std::fs::create_dir_all(dir);
            }
            let store = handle
                .path()
                .app_config_dir()
                .ok()
                .map(|dir| dir.join("fs-scope.json"));
            let always = cache.into_iter().collect();
            state.fs_scope.attach(store, always);
            if let Ok(dir) = handle.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&dir);
                commands::set_data_dir(dir.clone());
                state.attempts.attach(Some(dir.join("unlock-state.json")));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_inspect,
            commands::vault::vault_remember_folder,
            commands::vault::keyfile_kind,
            commands::vault::vault_create,
            commands::vault::vault_unlock,
            commands::vault::vault_save,
            commands::vault::vault_save_to,
            commands::vault::vault_rewrap,
            commands::vault::vault_remove_slot,
            commands::vault::vault_lock,
            commands::generator::generate_password,
            commands::generator::generate_login,
            commands::generator::generate_passphrase,
            commands::generator::generate_pin,
            commands::generator::generate_pronounceable,
            commands::generator::generator_entropy,
            commands::generator::generator_wordlist,
            commands::generator::generator_import_wordlist,
            commands::generator::generator_clear_wordlist,
            commands::clipboard::clipboard_copy_secret,
            commands::dialogs::dialog_pick_folder,
            commands::dialogs::dialog_pick_file,
            commands::dialogs::dialog_pick_save_file,
            commands::attachments::attachment_seal,
            commands::attachments::attachment_open,
            commands::attachments::attachment_seal_store,
            commands::attachments::attachment_open_file,
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
            commands::favicon::favicon_fetch,
            commands::breach::password_breach_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PMS");
}
