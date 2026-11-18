//! Сейф: один запечатанный файл на аккаунт, монотонная версия, атомарная запись.
//!
//! Тело запроса никогда не буферизуется целиком: байты идут из сокета в
//! временный файл кусками по 64 КиБ, а BLAKE3 считается на лету.

use std::path::Path;

use pms_proto::dto::VaultMeta;
use pms_proto::identity::AccountId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::paths;
use super::{db_error, io_error, now, AccountRecord, Store, StoreError};

/// Чтение кусками по 64 КиБ без аллокации на каждый запрос.
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedVersion {
    /// Принять любую версию: регистрация и осознанное «оставить моё».
    Any,
    /// Только эта версия; иначе `Conflict` (устарела) или `PreconditionFailed`.
    Version(u64),
}

impl Store {
    pub fn vault_meta(&self, account: AccountId) -> Result<Option<VaultMeta>, StoreError> {
        let record = self.account(account)?;
        Ok(record.and_then(|record| meta_of(&record)))
    }

    /// Принимает сейф потоком и возвращает новые метаданные.
    ///
    /// Порядок важен и покрыт тестами: блокировка аккаунта → сверка версии →
    /// проверка квоты → поток в `vault.bin.tmp` с подсчётом BLAKE3 → `fsync` →
    /// `rename` → обновление индекса. При любой ошибке временный файл удаляется.
    pub async fn put_vault<R: AsyncRead + Unpin>(
        &self,
        account: AccountId,
        expected: ExpectedVersion,
        mut body: R,
        len: u64,
    ) -> Result<VaultMeta, StoreError> {
        let lock = self.account_lock(account);
        let _guard = lock.lock().await;

        let record = self
            .account(account)?
            .ok_or_else(|| StoreError::Corrupt("аккаунт исчез".to_string()))?;

        let current = record.vault_version;
        match expected {
            ExpectedVersion::Any => {}
            ExpectedVersion::Version(version) if version == current => {}
            ExpectedVersion::Version(version) if version < current => {
                return Err(StoreError::Conflict(meta_of(&record).unwrap_or_else(
                    || VaultMeta {
                        version: current,
                        size: record.vault_size,
                        digest: record.vault_digest.clone(),
                        updated_at: record.updated_at.clone(),
                    },
                )));
            }
            ExpectedVersion::Version(_) => return Err(StoreError::PreconditionFailed),
        }

        let after_write = record
            .used_bytes
            .saturating_sub(record.vault_size)
            .saturating_add(len);
        if after_write > record.quota_bytes {
            return Err(StoreError::QuotaExceeded {
                used: record.used_bytes,
                quota: record.quota_bytes,
            });
        }

        let temp_path = paths::vault_tmp_path(self.root(), account);
        let final_path = paths::vault_path(self.root(), account);

        let digest = write_stream(&temp_path, &mut body, len)
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&temp_path);
            })?;

        tokio::fs::rename(&temp_path, &final_path)
            .await
            .map_err(|error| {
                let _ = std::fs::remove_file(&temp_path);
                io_error(error)
            })?;

        let version = current + 1;
        let updated_at = now();
        let used = record
            .used_bytes
            .saturating_sub(record.vault_size)
            .saturating_add(len);
        self.db()?
            .execute(
                "UPDATE accounts SET vault_version = ?2, vault_digest = ?3, vault_size = ?4, \
                 used_bytes = ?5, updated_at = ?6 WHERE account_id = ?1",
                rusqlite::params![
                    account.to_string(),
                    version as i64,
                    digest,
                    len as i64,
                    used as i64,
                    updated_at
                ],
            )
            .map_err(db_error)?;

        Ok(VaultMeta {
            version,
            size: len,
            digest,
            updated_at,
        })
    }

    /// Открывает файл сейфа для потоковой отдачи. Содержимое в память не читается.
    pub async fn open_vault(
        &self,
        account: AccountId,
    ) -> Result<Option<(VaultMeta, tokio::fs::File)>, StoreError> {
        let Some(record) = self.account(account)? else {
            return Ok(None);
        };
        let Some(meta) = meta_of(&record) else {
            return Ok(None);
        };
        let file = tokio::fs::File::open(paths::vault_path(self.root(), account))
            .await
            .map_err(io_error)?;
        Ok(Some((meta, file)))
    }
}

fn meta_of(record: &AccountRecord) -> Option<VaultMeta> {
    (record.vault_version > 0).then(|| VaultMeta {
        version: record.vault_version,
        size: record.vault_size,
        digest: record.vault_digest.clone(),
        updated_at: record.updated_at.clone(),
    })
}

/// Пишет ровно `len` байт из `body` в `path`, возвращая BLAKE3 в hex.
async fn write_stream<R: AsyncRead + Unpin>(
    path: &Path,
    body: &mut R,
    len: u64,
) -> Result<String, StoreError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(path).await.map_err(io_error)?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; CHUNK];
    let mut remaining = len;

    while remaining > 0 {
        let want = CHUNK.min(remaining as usize);
        let read = body.read(&mut buffer[..want]).await.map_err(io_error)?;
        if read == 0 {
            return Err(StoreError::Truncated {
                expected: len,
                got: len - remaining,
            });
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).await.map_err(io_error)?;
        remaining -= read as u64;
    }

    file.flush().await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    Ok(hasher.finalize().to_hex().to_string())
}
