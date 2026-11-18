//! Сейф по проводу: скачивание и отправка потоком через файл.
//!
//! Байты никогда не собираются в `Vec` целиком: сейф бывает в сотни мегабайт,
//! а блоб — ещё больше.

use std::path::{Path, PathBuf};

use futures_util::TryStreamExt;
use pms_proto::dto::VaultMeta;
use pms_proto::PROTOCOL_VERSION;

use super::client::{
    error_from_response, unreachable, RemoteClient, HEADER_DIGEST, HEADER_PROTOCOL,
    HEADER_VAULT_VERSION,
};
use super::temp::TempFile;
use super::RemoteError;

const VAULT_FILE: &str = "pms.vault";

impl RemoteClient {
    /// Скачивает сейф в `dest_dir/pms.vault`, возвращая его метаданные.
    ///
    /// Пишем во временный файл и переименовываем: оборванная загрузка не должна
    /// оставить обрезанный сейф на месте настоящего.
    pub async fn pull_vault(&self, token: &str, dest_dir: &Path) -> Result<VaultMeta, RemoteError> {
        let response = self
            .http()
            .get(self.endpoint("/v1/vault")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;

        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }

        let meta = meta_from_headers(&response)?;
        tokio::fs::create_dir_all(dest_dir).await.map_err(storage)?;

        let temp = TempFile::at(dest_dir.join(format!("{VAULT_FILE}.part")));
        let mut file = tokio::fs::File::create(temp.path())
            .await
            .map_err(storage)?;

        let mut stream = response.bytes_stream();
        let mut written = 0u64;
        while let Some(chunk) = stream.try_next().await.map_err(unreachable)? {
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk).await.map_err(storage)?;
            written += chunk.len() as u64;
        }
        use tokio::io::AsyncWriteExt;
        file.flush().await.map_err(storage)?;
        file.sync_all().await.map_err(storage)?;
        drop(file);

        if meta.size != 0 && written != meta.size {
            return Err(RemoteError::Protocol(format!(
                "сервер обещал {} байт, пришло {written}",
                meta.size
            )));
        }

        tokio::fs::rename(temp.path(), dest_dir.join(VAULT_FILE))
            .await
            .map_err(storage)?;
        Ok(meta)
    }

    /// Отправляет сейф из файла. `expected = None` — осознанная перезапись.
    pub async fn push_vault(
        &self,
        token: &str,
        expected: Option<u64>,
        file: &Path,
    ) -> Result<VaultMeta, RemoteError> {
        let metadata = tokio::fs::metadata(file).await.map_err(storage)?;
        let content = tokio::fs::File::open(file).await.map_err(storage)?;
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(content));

        let if_match = match expected {
            Some(version) => version.to_string(),
            None => "*".to_string(),
        };

        let response = self
            .http()
            .put(self.endpoint("/v1/vault")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .header(reqwest::header::IF_MATCH, if_match)
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
}

fn meta_from_headers(response: &reqwest::Response) -> Result<VaultMeta, RemoteError> {
    let header = |name: &str| -> Result<String, RemoteError> {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| RemoteError::Protocol(format!("сервер не прислал {name}")))
    };

    Ok(VaultMeta {
        version: header(HEADER_VAULT_VERSION)?
            .parse()
            .map_err(|_| RemoteError::Protocol("версия сейфа не число".to_string()))?,
        size: response.content_length().unwrap_or(0),
        digest: header(HEADER_DIGEST)?,
        updated_at: String::new(),
    })
}

pub fn destination(dest_dir: &Path) -> PathBuf {
    dest_dir.join(VAULT_FILE)
}

fn storage(error: std::io::Error) -> RemoteError {
    RemoteError::Storage(error.to_string())
}
