use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::fs_scope::Access;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::generator::{
    clear_custom_wordlist as core_clear_custom_wordlist,
    custom_wordlist_len as core_custom_wordlist_len, entropy_bits as core_entropy_bits,
    generate_login as core_generate_login, generate_passphrase as core_generate_passphrase,
    generate_password as core_generate_password, generate_pin as core_generate_pin,
    generate_pronounceable as core_generate_pronounceable,
    parse_custom_wordlist as core_parse_custom_wordlist,
    store_custom_wordlist as core_store_custom_wordlist, EntropyInput, LoginOptions,
    PassphraseOptions, PasswordOptions, PinOptions, PronounceableOptions,
    DICEWARE_WORDS as core_diceware_words, MAX_CUSTOM_WORDLIST_BYTES as core_max_wordlist_bytes,
    MIN_CUSTOM_WORDS as core_min_custom_words,
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
pub fn generate_pin(options: PinOptions) -> ApiResult<String> {
    Ok(core_generate_pin(&options)?)
}

#[tauri::command]
pub fn generate_pronounceable(options: PronounceableOptions) -> ApiResult<String> {
    Ok(core_generate_pronounceable(&options)?)
}

#[tauri::command]
pub fn generator_entropy(input: EntropyInput) -> ApiResult<f64> {
    Ok(core_entropy_bits(&input)?)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordlistInfo {
    /// Words in the imported list, if one is present.
    pub custom_words: Option<usize>,
    /// Words in the built-in list (EFF large).
    pub builtin_words: usize,
    pub min_custom_words: usize,
}

fn wordlist_info() -> WordlistInfo {
    WordlistInfo {
        custom_words: core_custom_wordlist_len(),
        builtin_words: core_diceware_words,
        min_custom_words: core_min_custom_words,
    }
}

#[tauri::command]
pub fn generator_wordlist() -> ApiResult<WordlistInfo> {
    Ok(wordlist_info())
}

/// Imports a word list from a file the user picked in the native dialog.
#[tauri::command]
pub async fn generator_import_wordlist(app: AppHandle, path: String) -> ApiResult<WordlistInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let path = state.fs_scope.resolve(&path, Access::Read)?;
        let metadata = std::fs::metadata(&path)
            .map_err(|error| ApiError::new("storage", error.to_string()))?;
        if metadata.len() > core_max_wordlist_bytes {
            return Err(ApiError::new(
                "file_too_large",
                "Словник больше 2 МБ — выберите файл меньше",
            ));
        }
        let bytes = std::fs::read(&path).map_err(|error| ApiError::new("storage", error.to_string()))?;
        let words = core_parse_custom_wordlist(&bytes).map_err(|error| match error {
            crate::core::error::VaultError::EmptyCharset => ApiError::new(
                "wordlist_too_small",
                "В словнике слишком мало слов: нужно не меньше 512",
            ),
            crate::core::error::VaultError::FileTooLarge => ApiError::new(
                "wordlist_too_large",
                "В словнике слишком много слов: максимум 50 000",
            ),
            _ => ApiError::new(
                "wordlist_invalid",
                "Словник должен содержать по одному слову в строке: буквы, цифры, дефис или апостроф, 2–24 символа",
            ),
        })?;
        core_store_custom_wordlist(&words)?;
        Ok(wordlist_info())
    })
    .await
    .map_err(|error| ApiError::new("wordlist", error.to_string()))?
}

#[tauri::command]
pub fn generator_clear_wordlist() -> ApiResult<WordlistInfo> {
    core_clear_custom_wordlist()?;
    Ok(wordlist_info())
}
