use crate::commands::{ApiResult, ApiError};
use crate::core::share::{SharedCard, decode, encode};

#[tauri::command]
pub fn share_encode(card: SharedCard, password: Option<String>) -> ApiResult<String> {
    encode(&card, password.as_deref()).map_err(ApiError::from)
}

#[tauri::command]
pub fn share_decode(payload: String, password: Option<String>) -> ApiResult<SharedCard> {
    decode(&payload, password.as_deref()).map_err(ApiError::from)
}
