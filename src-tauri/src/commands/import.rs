use base64::Engine as _;
use tauri::ipc::Response;

use crate::commands::{ApiError, ApiResult};
use crate::core::import::parse;

/// Parsed import is returned as raw JSON bytes instead of a serialized struct:
/// the parser borrows strings straight from the decoded file, so keeping the
/// bundle alive (and serializing it in one pass) avoids building tens of
/// thousands of intermediate `String`s.
#[tauri::command]
pub async fn import_parse(
    file_b64: String,
    password: Option<String>,
    keyfile_b64: Option<String>,
) -> ApiResult<Response> {
    tauri::async_runtime::spawn_blocking(move || import_parse_blocking(file_b64, password, keyfile_b64))
        .await
        .map_err(|error| ApiError::new("import_parse", error.to_string()))?
}

const MAX_IMPORT_BYTES: usize = 500 * 1024 * 1024;
const MAX_KEYFILE_BYTES: usize = 4 * 1024 * 1024;

fn decode_limited(value: &str, limit: usize, message: &str) -> ApiResult<Vec<u8>> {
    // Reject by the *encoded* length before allocating: a 4 GB base64 argument
    // must not cost 3 GB of memory only to be refused.
    if value.len() > limit.saturating_mul(4) / 3 + 8 {
        return Err(ApiError::new("import_too_large", message));
    }
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| ApiError::bad_base64())
}

fn import_parse_blocking(
    file_b64: String,
    password: Option<String>,
    keyfile_b64: Option<String>,
) -> ApiResult<Response> {
    let bytes = decode_limited(&file_b64, MAX_IMPORT_BYTES, "Файл импорта больше 500 МБ")?;
    let keyfile = keyfile_b64
        .map(|value| decode_limited(&value, MAX_KEYFILE_BYTES, "Файл-ключ слишком большой"))
        .transpose()?;

    let bundle = parse(&bytes, password.as_deref(), keyfile.as_deref()).map_err(ApiError::from)?;
    // The JSON is roughly the size of the exported file; cap the pre-allocation
    // so a huge archive cannot reserve half a gigabyte up front.
    let capacity = bytes.len().clamp(8 * 1024, 32 * 1024 * 1024);
    let mut payload = Vec::with_capacity(capacity);
    serde_json::to_writer(&mut payload, &bundle)
        .map_err(|_| ApiError::new("import_corrupt", "Не удалось прочитать файл импорта"))?;
    Ok(Response::new(payload))
}
