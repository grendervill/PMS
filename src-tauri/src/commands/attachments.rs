use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::attachments::{
    MAX_ATTACHMENT_BYTES, attachment_kind, open_attachment, seal_attachment,
};
use crate::core::models::{AttachmentKind, AttachmentMeta};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedAttachment {
    pub blob_b64: String,
    pub meta: AttachmentMeta,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedAttachment {
    pub b64: String,
}

#[tauri::command]
pub fn attachment_seal(
    session_id: Uuid,
    name: String,
    mime: String,
    b64: String,
    state: State<'_, AppState>,
) -> ApiResult<SealedAttachment> {
    if name.trim().is_empty() {
        return Err(ApiError::new("empty_name", "У файла нет имени"));
    }
    let bytes = STANDARD
        .decode(&b64)
        .map_err(|_| ApiError::bad_base64())?;
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::new(
            "file_too_large",
            "Файл больше 100 МБ — выберите файл меньше",
        ));
    }

    let id = Uuid::new_v4();
    let kind: AttachmentKind = attachment_kind(&mime);
    let size = bytes.len() as u64;
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

    let blob = state.sessions.with(session_id, |session| {
        seal_attachment(
            session.vault_key(),
            session.vault_id(),
            id,
            &bytes,
        )
    })?;

    Ok(SealedAttachment {
        blob_b64: STANDARD.encode(blob),
        meta: AttachmentMeta {
            id,
            name: name.trim().to_string(),
            mime: if mime.is_empty() {
                "application/octet-stream".to_string()
            } else {
                mime
            },
            size,
            sha256,
            kind,
            created_at,
        },
    })
}

#[tauri::command]
pub fn attachment_open(
    session_id: Uuid,
    attachment_id: Uuid,
    blob_b64: String,
    state: State<'_, AppState>,
) -> ApiResult<OpenedAttachment> {
    let blob = STANDARD
        .decode(&blob_b64)
        .map_err(|_| ApiError::bad_base64())?;
    let plaintext = state.sessions.with(session_id, |session| {
        open_attachment(
            session.vault_key(),
            session.vault_id(),
            attachment_id,
            &blob,
        )
    })?;

    Ok(OpenedAttachment {
        b64: STANDARD.encode(plaintext),
    })
}
