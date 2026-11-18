use crate::commands::ApiResult;
use crate::core::generator::{
    entropy_bits as core_entropy_bits, generate_login as core_generate_login,
    generate_passphrase as core_generate_passphrase, generate_password as core_generate_password,
    EntropyInput, LoginOptions, PassphraseOptions, PasswordOptions,
};

#[tauri::command]
pub fn generate_password(options: PasswordOptions) -> ApiResult<String> {
    Ok(core_generate_password(&options)?)
}

#[tauri::command]
pub fn generate_login(options: LoginOptions) -> ApiResult<String> {
    Ok(core_generate_login(&options)?)
}

#[tauri::command]
pub fn generate_passphrase(options: PassphraseOptions) -> ApiResult<String> {
    Ok(core_generate_passphrase(&options)?)
}

#[tauri::command]
pub fn generator_entropy(input: EntropyInput) -> ApiResult<f64> {
    Ok(core_entropy_bits(&input)?)
}
