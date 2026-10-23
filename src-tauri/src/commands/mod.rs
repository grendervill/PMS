pub mod app_icon;
pub mod attachments;
pub mod favicon;
pub mod biometric;
pub mod generator;
pub mod import;
pub mod pms_archive;
pub mod share;
pub mod storage;
pub mod ssh;
pub mod totp;
pub mod transfer;
pub mod vault;

use serde::Serialize;

use crate::core::error::VaultError;
use crate::core::session::SessionManager;
use crate::commands::transfer::TransferManager;

#[derive(Default)]
pub struct AppState {
    pub sessions: SessionManager,
    pub transfer: TransferManager,
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
