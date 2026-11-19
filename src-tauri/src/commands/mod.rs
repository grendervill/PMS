pub mod app_icon;
pub mod attachments;
pub mod attempts;
pub mod biometric;
pub mod breach;
pub mod clipboard;
pub mod dialogs;
pub mod favicon;
pub mod fs_scope;
pub mod generator;
pub mod import;
pub mod pms_archive;
pub mod remote;
pub mod secure_screen;
pub mod share;
pub mod ssh;
pub mod storage;
pub mod totp;
pub mod transfer;
pub mod vault;

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;

use crate::commands::attempts::UnlockGuard;
use crate::commands::fs_scope::FsScope;
use crate::commands::remote::RemoteRegistry;
use crate::commands::transfer::TransferManager;
use crate::core::error::VaultError;
use crate::core::session::SessionManager;

#[derive(Default)]
pub struct AppState {
    pub sessions: SessionManager,
    pub transfer: TransferManager,
    /// Paths the user granted through a native dialog; every command that takes
    /// a path from the renderer has to resolve it here first.
    pub fs_scope: FsScope,
    /// Cool-down after repeated failed unlocks.
    pub attempts: UnlockGuard,
    /// Активные сессии серверного режима: токены живут только здесь.
    pub remote: RemoteRegistry,
    /// Семя identity-ключа, прочитанное биометрией: до блокировки позволяет
    /// поднять серверную сессию без пароль-фразы.
    pub biometric_seed: std::sync::Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>,
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Remembers the app data directory once the app is set up. Commands that run
/// on worker threads (and therefore have no `AppHandle`) need it for their own
/// bookkeeping files.
pub fn set_data_dir(path: PathBuf) {
    let _ = DATA_DIR.set(path);
}

pub fn app_data_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn bad_base64() -> Self {
        Self::new("bad_base64", "Некорректные данные файла")
    }
}

impl From<VaultError> for ApiError {
    fn from(error: VaultError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
