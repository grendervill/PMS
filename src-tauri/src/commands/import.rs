use base64::Engine as _;

use crate::commands::{ApiError, ApiResult};
use crate::core::import::{ImportBundle, parse};

#[tauri::command]
pub fn import_parse(
    file_b64: String,
    password: Option<String>,
    keyfile_b64: Option<String>,
) -> ApiResult<ImportBundle> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(file_b64)
        .map_err(|_| ApiError::bad_base64())?;
    let keyfile = keyfile_b64
        .map(|value| {
            base64::engine::general_purpose::STANDARD
                .decode(value)
                .map_err(|_| ApiError::bad_base64())
        })
        .transpose()?;
    parse(&bytes, password.as_deref(), keyfile.as_deref()).map_err(ApiError::from)
}
