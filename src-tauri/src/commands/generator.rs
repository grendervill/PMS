use crate::commands::ApiResult;
use crate::core::generator::{
    LoginOptions, PasswordOptions, generate_login as core_generate_login,
    generate_password as core_generate_password,
};

#[tauri::command]
pub fn generate_password(options: PasswordOptions) -> ApiResult<String> {
    Ok(core_generate_password(&options)?)
}

#[tauri::command]
pub fn generate_login(options: LoginOptions) -> ApiResult<String> {
    Ok(core_generate_login(&options)?)
}
