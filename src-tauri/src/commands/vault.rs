use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::commands::fs_scope::Access;
use crate::commands::secure_screen;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::crypto::{keyfile_kind as classify_keyfile, UnlockSecretInput, MAX_KEYFILE_BYTES};
use crate::core::error::VaultError;
use crate::core::models::{CipherSuite, KdfSlotKind, VaultData};
use crate::core::vault::{
    create_vault, remove_slot, rewrap_vault, seal_document, unlock_vault, CreateVaultConfig,
};
use crate::core::vault_file::parse_vault_file;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UnlockSecretDto {
    Passphrase {
        value: String,
    },
    Keyfile {
        #[serde(rename = "bytesB64")]
        bytes_b64: String,
        passphrase: Option<String>,
    },
    PassphraseKeyfile {
        value: String,
        #[serde(rename = "bytesB64")]
        bytes_b64: String,
        passphrase: Option<String>,
    },
}

impl TryFrom<UnlockSecretDto> for UnlockSecretInput {
    type Error = ApiError;

    fn try_from(value: UnlockSecretDto) -> Result<Self, Self::Error> {
        match value {
            UnlockSecretDto::Passphrase { value } => Ok(UnlockSecretInput::Passphrase { value }),
            UnlockSecretDto::Keyfile {
                bytes_b64,
                passphrase,
            } => {
                let bytes = decode_keyfile(&bytes_b64)?;
                Ok(UnlockSecretInput::Keyfile { bytes, passphrase })
            }
            UnlockSecretDto::PassphraseKeyfile {
                value,
                bytes_b64,
                passphrase,
            } => {
                let bytes = decode_keyfile(&bytes_b64)?;
                Ok(UnlockSecretInput::PassphraseKeyfile {
                    passphrase: value,
                    bytes,
                    keyfile_passphrase: passphrase,
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultPublicInfo {
    pub vault_id: Uuid,
    pub format: u16,
    pub created_at: String,
    pub cipher: CipherSuite,
    pub slot_kinds: Vec<KdfSlotKind>,
    pub file_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultResponse {
    pub session_id: Uuid,
    pub file_b64: String,
    pub data: VaultData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockVaultResponse {
    pub session_id: Uuid,
    pub data: VaultData,
}

/// Keyfiles are hashed whole, so the size is capped before decoding: a 500 MiB
/// "keyfile" must not cost 500 MiB of memory on every unlock attempt.
fn decode_keyfile(bytes_b64: &str) -> ApiResult<Vec<u8>> {
    if bytes_b64.len() > MAX_KEYFILE_BYTES.saturating_mul(4) / 3 + 8 {
        return Err(ApiError::new(
            "keyfile_too_large",
            "Файл-ключ больше 1 МБ — выберите файл меньше",
        ));
    }
    STANDARD
        .decode(bytes_b64)
        .map_err(|_| ApiError::bad_base64())
}

/// Upper bound for a vault file on the wire (payload + header + framing).
const MAX_VAULT_FILE_BYTES: usize = 512 * 1024 * 1024;

/// Reads `folder/pms.vault` and returns its `vault_id` and sealed payload.
///
/// `rewrap` and `remove_slot` only change the header, so they need the payload
/// but not the key; reading it from disk keeps it out of the session for the
/// whole lifetime of the vault.
fn read_sealed_payload(folder: &std::path::Path) -> ApiResult<(Uuid, Vec<u8>)> {
    let path = folder.join("pms.vault");
    let bytes = std::fs::read(&path).map_err(|error| {
        ApiError::new(
            "storage",
            format!("Не удалось прочитать хранилище: {error}"),
        )
    })?;
    let parsed = parse_vault_file(&bytes)?;
    Ok((parsed.header.vault_id, parsed.payload))
}

fn decode_file(file_b64: &str) -> ApiResult<Vec<u8>> {
    if file_b64.len() > MAX_VAULT_FILE_BYTES.saturating_mul(4) / 3 + 8 {
        return Err(ApiError::new(
            "file_too_large",
            "Файл хранилища слишком большой",
        ));
    }
    STANDARD
        .decode(file_b64)
        .map_err(|_| ApiError::bad_base64())
}

fn encode_file(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn file_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[tauri::command]
pub fn keyfile_kind(file_b64: String) -> ApiResult<String> {
    let bytes = decode_file(&file_b64)?;
    classify_keyfile(&bytes)
        .map(str::to_string)
        .map_err(ApiError::from)
}

/// Tells the scope that `folder` holds a vault, so it is remembered across
/// restarts (a folder without `pms.vault` never is).
#[tauri::command]
pub fn vault_remember_folder(folder: String, state: State<'_, AppState>) -> ApiResult<()> {
    let path = state.fs_scope.resolve(&folder, Access::Read)?;
    state.fs_scope.remember(&path);
    Ok(())
}

#[tauri::command]
pub fn vault_inspect(file_b64: String) -> ApiResult<VaultPublicInfo> {
    inspect_bytes(&decode_file(&file_b64)?)
}

fn inspect_bytes(bytes: &[u8]) -> ApiResult<VaultPublicInfo> {
    let file = parse_vault_file(bytes)?;

    Ok(VaultPublicInfo {
        vault_id: file.header.vault_id,
        format: file.header.format,
        created_at: file.header.created_at,
        cipher: file.header.cipher,
        slot_kinds: file.header.kdf_slots.iter().map(|slot| slot.kind).collect(),
        file_hash: file_hash(bytes),
    })
}

/// Header that carries the unlock secret for the raw-bytes commands: their body
/// is the vault file itself, so the secret cannot travel as an argument.
///
/// It is a JSON `UnlockSecretDto`, because a header value is a string and the
/// secret has three shapes. Never log it: it holds the passphrase.
const SECRET_HEADER: &str = "x-pms-secret";

fn bad_secret() -> ApiError {
    ApiError::new("bad_request", "Некорректный секрет разблокировки")
}

/// Reads and parses `x-pms-secret`. Missing, non-ASCII and malformed values are
/// all `bad_request` — the renderer never retries with base64 on this code.
fn secret_from_headers(headers: &tauri::http::HeaderMap) -> ApiResult<UnlockSecretDto> {
    let value = headers
        .get(SECRET_HEADER)
        .ok_or_else(|| ApiError::new("bad_request", "Не передан секрет разблокировки"))?;
    let text = value.to_str().map_err(|_| bad_secret())?;
    serde_json::from_str(text).map_err(|_| bad_secret())
}

/// The raw body of a command invocation.
///
/// Android cannot send raw bodies (`InvokeBody::Raw` never arrives there), so
/// the renderer keeps the base64 commands on that platform; this error is what
/// it would see if it ever called a raw command anyway.
fn raw_body(request: &tauri::ipc::Request<'_>) -> ApiResult<Vec<u8>> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => {
            if bytes.len() > MAX_VAULT_FILE_BYTES {
                return Err(ApiError::new(
                    "file_too_large",
                    "Файл хранилища слишком большой",
                ));
            }
            Ok(bytes.clone())
        }
        _ => Err(ApiError::new(
            "raw_unavailable",
            "Сырые байты не поддерживаются на этой платформе",
        )),
    }
}

#[tauri::command]
pub async fn vault_create(
    app: AppHandle,
    config: CreateVaultConfig,
    secret: UnlockSecretDto,
) -> ApiResult<CreateVaultResponse> {
    if config.name.trim().is_empty() {
        return Err(ApiError::new(
            "empty_name",
            "Название хранилища не может быть пустым",
        ));
    }

    let secret = UnlockSecretInput::try_from(secret)?;
    // Argon2id at the "paranoid" profile takes a second or more: it must never
    // run on the IPC thread, which is the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (file, session, data) = create_vault(&config, &secret)?;
        let session_id = session.id;
        secure_screen::apply(&app, data.settings.secure_screen);
        state.sessions.put(session);

        Ok(CreateVaultResponse {
            session_id,
            file_b64: encode_file(&file),
            data,
        })
    })
    .await
    .map_err(|error| ApiError::new("vault_create", error.to_string()))?
}

#[tauri::command]
pub async fn vault_unlock(
    app: AppHandle,
    file_b64: String,
    secret: UnlockSecretDto,
) -> ApiResult<UnlockVaultResponse> {
    let bytes = decode_file(&file_b64)?;
    unlock_bytes(app, bytes, secret).await
}

/// Desktop unlock: the file arrives as raw bytes (`invoke(cmd, bytes, { headers
/// })`) and the secret in the `x-pms-secret` header, so the whole base64 round
/// trip in the renderer disappears. Android keeps [`vault_unlock`].
#[tauri::command]
pub async fn vault_unlock_raw(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> ApiResult<UnlockVaultResponse> {
    let bytes = raw_body(&request)?;
    let secret = secret_from_headers(request.headers())?;
    unlock_bytes(app, bytes, secret).await
}

/// The shared unlock path of the base64 and the raw command.
async fn unlock_bytes(
    app: AppHandle,
    bytes: Vec<u8>,
    secret: UnlockSecretDto,
) -> ApiResult<UnlockVaultResponse> {
    let secret = UnlockSecretInput::try_from(secret)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // The vault id is in the plaintext header, so the cool-down can be
        // checked before the expensive key derivation runs.
        let vault_id = parse_vault_file(&bytes)?.header.vault_id;
        state.attempts.check(vault_id)?;

        match unlock_vault(&bytes, &secret) {
            Ok((session, data)) => {
                state.attempts.record_success(vault_id);
                let session_id = session.id;
                secure_screen::apply(&app, data.settings.secure_screen);
                state.sessions.put(session);
                Ok(UnlockVaultResponse { session_id, data })
            }
            Err(error) => {
                if matches!(error, crate::core::error::VaultError::InvalidCredentials) {
                    state.attempts.record_failure(vault_id);
                }
                Err(error.into())
            }
        }
    })
    .await
    .map_err(|error| ApiError::new("vault_unlock", error.to_string()))?
}

#[tauri::command]
pub async fn vault_inspect_raw(request: tauri::ipc::Request<'_>) -> ApiResult<VaultPublicInfo> {
    inspect_bytes(&raw_body(&request)?)
}

/// Saves the vault and returns the sealed file as raw bytes (`ArrayBuffer` in
/// the renderer) instead of base64: the base64 round trip alone cost ~70 ms per
/// 4 MiB, on every save.
#[tauri::command]
pub async fn vault_save(
    app: AppHandle,
    session_id: Uuid,
    data: VaultData,
) -> ApiResult<tauri::ipc::Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // The key is copied out of the session, so the lock is not held while
        // the data is serialized and encrypted.
        let material = state.sessions.material(session_id)?;
        let (file, plaintext_len) = seal_document(
            &material.header,
            &material.vault_key,
            &data,
            material.plaintext_hint,
        )?;
        state
            .sessions
            .set_plaintext_len(session_id, plaintext_len, material.generation)?;
        secure_screen::apply(&app, data.settings.secure_screen);
        Ok(tauri::ipc::Response::new(file))
    })
    .await
    .map_err(|error| ApiError::new("vault_save", error.to_string()))?
}

/// Saves the vault straight to `folder/pms.vault`.
///
/// This is the path the desktop app uses: the sealed bytes never travel to the
/// renderer, so a save costs no base64 round trip (~140 ms per 4 MiB) and no
/// temporary copy in JavaScript.
#[tauri::command]
pub async fn vault_save_to(
    app: AppHandle,
    session_id: Uuid,
    folder: String,
    data: VaultData,
) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Write)?;
        let material = state.sessions.material(session_id)?;
        let (file, plaintext_len) = seal_document(
            &material.header,
            &material.vault_key,
            &data,
            material.plaintext_hint,
        )?;
        let path = folder.join("pms.vault");
        crate::commands::storage::write_private_atomic(&path, &file)?;
        state
            .sessions
            .set_plaintext_len(session_id, plaintext_len, material.generation)?;
        secure_screen::apply(&app, data.settings.secure_screen);
        Ok(())
    })
    .await
    .map_err(|error| ApiError::new("vault_save", error.to_string()))?
}

#[tauri::command]
pub async fn vault_rewrap(
    app: AppHandle,
    session_id: Uuid,
    folder: String,
    secret: UnlockSecretDto,
    keep_existing: bool,
) -> ApiResult<tauri::ipc::Response> {
    let secret = UnlockSecretInput::try_from(secret)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Read)?;
        // The sealed payload is read from the file instead of being kept in
        // memory for the whole session.
        let (file_vault_id, payload) = read_sealed_payload(&folder)?;
        let file = state.sessions.with_mut(session_id, |session| {
            if session.vault_id() != file_vault_id {
                return Err(VaultError::VaultIdMismatch);
            }
            rewrap_vault(session, &payload, &secret, keep_existing)
        })?;
        Ok(tauri::ipc::Response::new(file))
    })
    .await
    .map_err(|error| ApiError::new("vault_rewrap", error.to_string()))?
}

#[tauri::command]
pub async fn vault_remove_slot(
    app: AppHandle,
    session_id: Uuid,
    folder: String,
    kind: KdfSlotKind,
) -> ApiResult<tauri::ipc::Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Read)?;
        let (file_vault_id, payload) = read_sealed_payload(&folder)?;
        let file = state.sessions.with_mut(session_id, |session| {
            if session.vault_id() != file_vault_id {
                return Err(VaultError::VaultIdMismatch);
            }
            remove_slot(session, &payload, kind)
        })?;
        Ok(tauri::ipc::Response::new(file))
    })
    .await
    .map_err(|error| ApiError::new("vault_remove_slot", error.to_string()))?
}

#[tauri::command]
pub fn vault_lock(session_id: Uuid, state: State<'_, AppState>) -> ApiResult<()> {
    state.sessions.lock(session_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::http::{HeaderMap, HeaderValue};

    #[test]
    fn passphrase_dto_converts() {
        let dto = UnlockSecretDto::Passphrase {
            value: "secret".to_string(),
        };
        assert!(matches!(
            UnlockSecretInput::try_from(dto).unwrap(),
            UnlockSecretInput::Passphrase { .. }
        ));
    }

    #[test]
    fn keyfile_dto_decodes_base64() {
        let dto = UnlockSecretDto::Keyfile {
            bytes_b64: STANDARD.encode(b"keyfile"),
            passphrase: Some("pass".to_string()),
        };
        let converted = UnlockSecretInput::try_from(dto).unwrap();

        match converted {
            UnlockSecretInput::Keyfile { bytes, passphrase } => {
                assert_eq!(bytes, b"keyfile");
                assert_eq!(passphrase.as_deref(), Some("pass"));
            }
            _ => panic!("expected keyfile"),
        }
    }

    #[test]
    fn keyfile_dto_accepts_the_frontend_wire_format() {
        let value = serde_json::json!({
            "kind": "keyfile",
            "bytesB64": "a2V5ZmlsZQ==",
            "passphrase": "ssh-pass"
        });
        let dto: UnlockSecretDto = serde_json::from_value(value).unwrap();
        let converted = UnlockSecretInput::try_from(dto).unwrap();

        match converted {
            UnlockSecretInput::Keyfile { bytes, passphrase } => {
                assert_eq!(bytes, b"keyfile");
                assert_eq!(passphrase.as_deref(), Some("ssh-pass"));
            }
            _ => panic!("expected keyfile"),
        }
    }

    #[test]
    fn invalid_base64_is_rejected() {
        let dto = UnlockSecretDto::Keyfile {
            bytes_b64: "not base64!".to_string(),
            passphrase: None,
        };
        assert_eq!(
            UnlockSecretInput::try_from(dto).unwrap_err().code,
            "bad_base64"
        );
    }

    fn headers_with_secret(value: &[u8]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(SECRET_HEADER, HeaderValue::from_bytes(value).unwrap());
        headers
    }

    #[test]
    fn secret_from_headers_parses_the_json_dto() {
        let headers = headers_with_secret(br#"{"kind":"passphrase","value":"correct horse"}"#);
        match secret_from_headers(&headers).unwrap() {
            UnlockSecretDto::Passphrase { value } => assert_eq!(value, "correct horse"),
            other => panic!("unexpected secret {other:?}"),
        }
    }

    #[test]
    fn secret_from_headers_rejects_missing_and_malformed_values() {
        let missing = secret_from_headers(&HeaderMap::new()).unwrap_err();
        assert_eq!(missing.code, "bad_request");

        let garbage = secret_from_headers(&headers_with_secret(b"not json")).unwrap_err();
        assert_eq!(garbage.code, "bad_request");

        let wrong_shape =
            secret_from_headers(&headers_with_secret(br#"{"kind":"nope"}"#)).unwrap_err();
        assert_eq!(wrong_shape.code, "bad_request");

        // A non-ASCII byte cannot be turned into a `&str`.
        let binary = secret_from_headers(&headers_with_secret(&[0xff, 0xfe])).unwrap_err();
        assert_eq!(binary.code, "bad_request");
    }
}
