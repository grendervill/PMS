use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::crypto::UnlockSecretInput;
use crate::core::models::{CipherSuite, KdfSlotKind, VaultData};
use crate::core::vault::{
    CreateVaultConfig, create_vault, remove_slot, rewrap_vault, save_vault, unlock_vault,
};
use crate::core::vault_file::parse_vault_file;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UnlockSecretDto {
    Passphrase { value: String },
    Keyfile {
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
                let bytes = STANDARD
                    .decode(bytes_b64)
                    .map_err(|_| ApiError::bad_base64())?;
                Ok(UnlockSecretInput::Keyfile { bytes, passphrase })
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveVaultResponse {
    pub file_b64: String,
}

fn decode_file(file_b64: &str) -> ApiResult<Vec<u8>> {
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
pub fn vault_inspect(file_b64: String) -> ApiResult<VaultPublicInfo> {
    let bytes = decode_file(&file_b64)?;
    let file = parse_vault_file(&bytes)?;

    Ok(VaultPublicInfo {
        vault_id: file.header.vault_id,
        format: file.header.format,
        created_at: file.header.created_at,
        cipher: file.header.cipher,
        slot_kinds: file.header.kdf_slots.iter().map(|slot| slot.kind).collect(),
        file_hash: file_hash(&bytes),
    })
}

#[tauri::command]
pub fn vault_create(
    config: CreateVaultConfig,
    secret: UnlockSecretDto,
    state: State<'_, AppState>,
) -> ApiResult<CreateVaultResponse> {
    if config.name.trim().is_empty() {
        return Err(ApiError::new("empty_name", "Название хранилища не может быть пустым"));
    }

    let secret = UnlockSecretInput::try_from(secret)?;
    let (file, session, data) = create_vault(&config, &secret)?;
    let session_id = session.id;
    state.sessions.put(session);

    Ok(CreateVaultResponse {
        session_id,
        file_b64: encode_file(&file),
        data,
    })
}

#[tauri::command]
pub fn vault_unlock(
    file_b64: String,
    secret: UnlockSecretDto,
    state: State<'_, AppState>,
) -> ApiResult<UnlockVaultResponse> {
    let bytes = decode_file(&file_b64)?;
    let secret = UnlockSecretInput::try_from(secret)?;
    let (session, data) = unlock_vault(&bytes, &secret)?;
    let session_id = session.id;
    state.sessions.put(session);

    Ok(UnlockVaultResponse { session_id, data })
}

#[tauri::command]
pub fn vault_save(
    session_id: Uuid,
    data: VaultData,
    state: State<'_, AppState>,
) -> ApiResult<SaveVaultResponse> {
    let file = state
        .sessions
        .with_mut(session_id, |session| save_vault(session, &data))?;

    Ok(SaveVaultResponse {
        file_b64: encode_file(&file),
    })
}

#[tauri::command]
pub fn vault_rewrap(
    session_id: Uuid,
    secret: UnlockSecretDto,
    keep_existing: bool,
    state: State<'_, AppState>,
) -> ApiResult<SaveVaultResponse> {
    let secret = UnlockSecretInput::try_from(secret)?;
    let file = state
        .sessions
        .with_mut(session_id, |session| {
            rewrap_vault(session, &secret, keep_existing)
        })?;

    Ok(SaveVaultResponse {
        file_b64: encode_file(&file),
    })
}

#[tauri::command]
pub fn vault_remove_slot(
    session_id: Uuid,
    kind: KdfSlotKind,
    state: State<'_, AppState>,
) -> ApiResult<SaveVaultResponse> {
    let file = state
        .sessions
        .with_mut(session_id, |session| remove_slot(session, kind))?;

    Ok(SaveVaultResponse {
        file_b64: encode_file(&file),
    })
}

#[tauri::command]
pub fn vault_lock(session_id: Uuid, state: State<'_, AppState>) -> ApiResult<()> {
    state.sessions.lock(session_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
