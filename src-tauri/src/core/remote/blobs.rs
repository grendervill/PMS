//! Запечатанные вложения: загрузка, список, скачивание, удаление и квота.

use std::path::{Path, PathBuf};

use pms_proto::dto::{BlobInfo, Deleted, Usage};
use pms_proto::identity::{sign_account_delete, DeviceId, Identity};
use pms_proto::PROTOCOL_VERSION;

use super::client::{
    error_from_response, unreachable, RemoteClient, HEADER_DIGEST, HEADER_PROTOCOL,
    HEADER_SIGNATURE,
};
use super::temp::TempFile;
use super::RemoteError;

/// UUID в нижнем hex, как требует сервер.
///
/// Без проверки `blob_id` попадает в путь через `Url::join`, а тот схлопывает
/// `..`: `blob_id = "../../vault"` превратил бы загрузку вложения в перезапись
/// сейфа без `If-Match`.
pub fn is_blob_id(value: &str) -> bool {
    let mut digits = 0;
    for byte in value.bytes() {
        match byte {
            b'-' => {}
            b'0'..=b'9' | b'a'..=b'f' => digits += 1,
            _ => return false,
        }
    }
    digits == 32 && value.len() <= 36
}

fn check_blob_id(blob_id: &str) -> Result<(), RemoteError> {
    if is_blob_id(blob_id) {
        Ok(())
    } else {
        Err(RemoteError::BadRequest(
            "blob_id должен быть UUID в нижнем регистре".to_string(),
        ))
    }
}

impl RemoteClient {
    pub async fn list_blobs(&self, token: &str) -> Result<Vec<BlobInfo>, RemoteError> {
        let response = self
            .http()
            .get(self.endpoint("/v1/blobs")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;

        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        response.json().await.map_err(unreachable)
    }

    /// Отправляет блоб из файла. Повторная отправка того же содержимого
    /// бесплатна: сервер узнаёт BLAKE3 и ничего не пишет.
    pub async fn put_blob(
        &self,
        token: &str,
        blob_id: &str,
        file: &Path,
    ) -> Result<BlobInfo, RemoteError> {
        check_blob_id(blob_id)?;
        let digest = digest_of_file(file).await?;
        let metadata = tokio::fs::metadata(file).await.map_err(storage)?;
        let content = tokio::fs::File::open(file).await.map_err(storage)?;
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(content));

        let response = self
            .http()
            .put(self.endpoint(&format!("/v1/blobs/{blob_id}"))?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .header(HEADER_DIGEST, digest)
            .header(reqwest::header::CONTENT_LENGTH, metadata.len())
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(unreachable)?;

        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        response.json().await.map_err(unreachable)
    }

    /// Скачивает блоб во временный файл и переименовывает только целиком.
    pub async fn get_blob(
        &self,
        token: &str,
        blob_id: &str,
        dest: &Path,
    ) -> Result<BlobInfo, RemoteError> {
        check_blob_id(blob_id)?;
        let response = self
            .http()
            .get(self.endpoint(&format!("/v1/blobs/{blob_id}"))?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }

        let size = response.content_length().unwrap_or(0);
        let digest = response
            .headers()
            .get(HEADER_DIGEST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();

        // Суффикс, а не замена расширения: для `foo.part` иначе временный путь
        // совпал бы с целевым и guard удалил бы только что скачанный файл.
        let temp = TempFile::at(PathBuf::from(format!("{}.part", dest.display())));
        let mut file = tokio::fs::File::create(temp.path())
            .await
            .map_err(storage)?;
        let mut stream = response.bytes_stream();
        let mut written = 0u64;
        while let Some(chunk) = futures_util::TryStreamExt::try_next(&mut stream)
            .await
            .map_err(unreachable)?
        {
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk).await.map_err(storage)?;
            written += chunk.len() as u64;
        }
        use tokio::io::AsyncWriteExt;
        file.flush().await.map_err(storage)?;
        file.sync_all().await.map_err(storage)?;
        drop(file);

        if size != 0 && written != size {
            return Err(RemoteError::Protocol(format!(
                "сервер обещал {size} байт, пришло {written}"
            )));
        }
        tokio::fs::rename(temp.path(), dest)
            .await
            .map_err(storage)?;

        Ok(BlobInfo {
            blob_id: blob_id.to_string(),
            size: written,
            digest,
            created_at: String::new(),
            last_used_at: String::new(),
        })
    }

    pub async fn delete_blob(&self, token: &str, blob_id: &str) -> Result<bool, RemoteError> {
        check_blob_id(blob_id)?;
        let response = self
            .http()
            .delete(self.endpoint(&format!("/v1/blobs/{blob_id}"))?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;

        match response.status() {
            status if status.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => Err(error_from_response(response).await),
        }
    }

    pub async fn usage(&self, token: &str) -> Result<Usage, RemoteError> {
        let response = self
            .http()
            .get(self.endpoint("/v1/usage")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        response.json().await.map_err(unreachable)
    }

    /// Удаляет аккаунт на сервере. Локальная копия остаётся у пользователя.
    pub async fn delete_account(
        &self,
        token: &str,
        identity: &Identity,
        device: DeviceId,
    ) -> Result<(), RemoteError> {
        let account = pms_proto::identity::AccountId::from_public_key(&identity.public);
        let signature = sign_account_delete(identity, &account, &device);

        let response = self
            .http()
            .delete(self.endpoint("/v1/account")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .header(HEADER_SIGNATURE, hex::encode(signature))
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        let _: Deleted = response.json().await.map_err(unreachable)?;
        Ok(())
    }
}

pub async fn digest_of_file(path: &Path) -> Result<String, RemoteError> {
    use tokio::io::AsyncReadExt;

    const CHUNK: usize = 64 * 1024;
    let mut file = tokio::fs::File::open(path).await.map_err(storage)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer).await.map_err(storage)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn storage(error: std::io::Error) -> RemoteError {
    RemoteError::Storage(error.to_string())
}
