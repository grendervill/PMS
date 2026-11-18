use std::io::Read;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::commands::fs_scope::Access;
use crate::commands::storage::write_private_atomic;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::attachments::{
    attachment_kind, open_attachment, seal_attachment_in_place, ATTACHMENT_HEADER_LEN,
    MAX_ATTACHMENT_BYTES,
};
use crate::core::models::AttachmentMeta;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedAttachment {
    pub blob_b64: String,
    pub meta: AttachmentMeta,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAttachment {
    pub meta: AttachmentMeta,
}

pub(crate) fn attachment_path(folder: &Path, id: Uuid) -> PathBuf {
    folder.join("attachments").join(format!("{id}.pmsa"))
}

fn max_b64_len() -> usize {
    MAX_ATTACHMENT_BYTES.saturating_mul(4) / 3 + 8
}

/// Decodes base64 straight into the final sealed-blob buffer: the plaintext is
/// placed after the attachment header and encrypted in place, so a 100 MiB file
/// costs one allocation and no extra copies.
fn seal_from_base64(
    vault_key: &[u8; 32],
    vault_id: Uuid,
    attachment_id: Uuid,
    b64: &str,
) -> ApiResult<(Vec<u8>, usize, String)> {
    // Upper bound for the decoded length: base64 packs 3 bytes per 4 chars.
    let upper = b64.len() / 4 * 3 + 3;
    if upper > MAX_ATTACHMENT_BYTES + 3 {
        return Err(ApiError::new(
            "file_too_large",
            "Файл больше 100 МБ — выберите файл меньше",
        ));
    }

    let mut blob = vec![0u8; ATTACHMENT_HEADER_LEN + upper];
    let written = STANDARD
        .decode_slice(b64.as_bytes(), &mut blob[ATTACHMENT_HEADER_LEN..])
        .map_err(|_| ApiError::bad_base64())?;
    if written > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::new(
            "file_too_large",
            "Файл больше 100 МБ — выберите файл меньше",
        ));
    }
    if written == 0 {
        return Err(ApiError::new("empty_file", "Файл пустой"));
    }
    blob.truncate(ATTACHMENT_HEADER_LEN + written);

    // The metadata digest is over the plaintext, so hash it before the in-place
    // encryption overwrites it.
    let digest = hex::encode(Sha256::digest(&blob[ATTACHMENT_HEADER_LEN..]));
    seal_attachment_in_place(vault_key, vault_id, attachment_id, &mut blob)?;
    Ok((blob, written, digest))
}

fn meta_for(id: Uuid, name: String, mime: String, size: usize, digest: String) -> AttachmentMeta {
    let mime = if mime.is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime
    };
    let kind = attachment_kind(&mime);
    AttachmentMeta {
        id,
        name,
        mime,
        size: size as u64,
        sha256: digest,
        kind,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    }
}

fn checked_name(name: &str) -> ApiResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::new("empty_name", "У файла нет имени"));
    }
    Ok(trimmed.to_string())
}

/// Seals an attachment and writes it into `folder/attachments` in one step.
///
/// The sealed bytes never travel to the renderer: before this, an upload did
/// `bytes -> base64 -> IPC -> Rust -> base64 -> IPC -> base64 -> IPC -> Rust`,
/// i.e. six full copies of the file (three of them in JavaScript).
#[tauri::command]
pub async fn attachment_seal_store(
    app: AppHandle,
    session_id: Uuid,
    folder: String,
    name: String,
    mime: String,
    b64: String,
) -> ApiResult<StoredAttachment> {
    let name = checked_name(&name)?;
    if b64.len() > max_b64_len() {
        return Err(ApiError::new(
            "file_too_large",
            "Файл больше 100 МБ — выберите файл меньше",
        ));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Write)?;
        let material = state.sessions.material(session_id)?;

        let id = Uuid::new_v4();
        let (blob, size, digest) =
            seal_from_base64(&material.vault_key, material.header.vault_id, id, &b64)?;

        let path = attachment_path(&folder, id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ApiError::new("storage", error.to_string()))?;
        }
        write_private_atomic(&path, &blob)?;

        Ok(StoredAttachment {
            meta: meta_for(id, name, mime, size, digest),
        })
    })
    .await
    .map_err(|error| ApiError::new("attachment", error.to_string()))?
}

/// Base64 variant kept for the Android storage backend, which writes the blob
/// through the SAF plugin and therefore needs the bytes in JavaScript.
#[tauri::command]
pub async fn attachment_seal(
    app: AppHandle,
    session_id: Uuid,
    name: String,
    mime: String,
    b64: String,
) -> ApiResult<SealedAttachment> {
    let name = checked_name(&name)?;
    if b64.len() > max_b64_len() {
        return Err(ApiError::new(
            "file_too_large",
            "Файл больше 100 МБ — выберите файл меньше",
        ));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let material = state.sessions.material(session_id)?;
        let id = Uuid::new_v4();
        let (blob, size, digest) =
            seal_from_base64(&material.vault_key, material.header.vault_id, id, &b64)?;
        Ok(SealedAttachment {
            meta: meta_for(id, name, mime, size, digest),
            blob_b64: STANDARD.encode(blob),
        })
    })
    .await
    .map_err(|error| ApiError::new("attachment", error.to_string()))?
}

/// Reads and decrypts an attachment straight from the vault folder and returns
/// the plaintext as an `ArrayBuffer`, without any base64 hop.
#[tauri::command]
pub async fn attachment_open_file(
    app: AppHandle,
    session_id: Uuid,
    attachment_id: Uuid,
    folder: String,
) -> ApiResult<tauri::ipc::Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let folder = state.fs_scope.resolve(&folder, Access::Read)?;
        let material = state.sessions.material(session_id)?;
        let path = attachment_path(&folder, attachment_id);
        let limit = (MAX_ATTACHMENT_BYTES + ATTACHMENT_HEADER_LEN + 16) as u64;
        // Check the size before reading: a 4 GiB file in `attachments/` must not
        // be pulled into memory just to be refused.
        let size = std::fs::metadata(&path)
            .map_err(|error| ApiError::new("storage", error.to_string()))?
            .len();
        if size > limit {
            return Err(ApiError::new("file_too_large", "Файл слишком большой"));
        }
        let mut blob = Vec::with_capacity(size as usize);
        std::fs::File::open(&path)
            .map_err(|error| ApiError::new("storage", error.to_string()))?
            .take(limit)
            .read_to_end(&mut blob)
            .map_err(|error| ApiError::new("storage", error.to_string()))?;
        if blob.len() as u64 != size {
            return Err(ApiError::new("storage", "Файл изменился при чтении"));
        }
        let plaintext = open_attachment(
            &material.vault_key,
            material.header.vault_id,
            attachment_id,
            &blob,
        )?;
        Ok(tauri::ipc::Response::new(plaintext))
    })
    .await
    .map_err(|error| ApiError::new("attachment", error.to_string()))?
}

/// Base64 variant kept for the Android storage backend.
#[tauri::command]
pub async fn attachment_open(
    app: AppHandle,
    session_id: Uuid,
    attachment_id: Uuid,
    blob_b64: String,
) -> ApiResult<tauri::ipc::Response> {
    if blob_b64.len() > max_b64_len() {
        return Err(ApiError::new("file_too_large", "Файл слишком большой"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let blob = STANDARD
            .decode(&blob_b64)
            .map_err(|_| ApiError::bad_base64())?;
        let material = state.sessions.material(session_id)?;
        let plaintext = open_attachment(
            &material.vault_key,
            material.header.vault_id,
            attachment_id,
            &blob,
        )?;
        Ok(tauri::ipc::Response::new(plaintext))
    })
    .await
    .map_err(|error| ApiError::new("attachment", error.to_string()))?
}
