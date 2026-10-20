use crate::commands::ApiResult;
use crate::core::models::TotpConfig;
use crate::core::totp::{TotpCode, code, inspect};

#[tauri::command]
pub fn totp_inspect(input: String) -> ApiResult<TotpConfig> {
    Ok(inspect(&input)?)
}

#[tauri::command]
pub fn totp_code(config: TotpConfig, at: Option<u64>) -> ApiResult<TotpCode> {
    Ok(code(&config, at)?)
}
