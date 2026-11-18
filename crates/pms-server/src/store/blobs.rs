//! Запечатанные вложения. Клиент присылает уже зашифрованные байты и BLAKE3,
//! сервер их проверяет, кладёт файлом и ведёт учёт места.

use std::path::Path;

use pms_proto::dto::{BlobInfo, Usage};
use pms_proto::identity::AccountId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::paths::{self, is_blob_id};
use super::{db_error, io_error, now, AccountRecord, Store, StoreError};

const CHUNK: usize = 64 * 1024;

impl Store {
    pub async fn put_blob<R: AsyncRead + Unpin>(
        &self,
        account: AccountId,
        blob_id: &str,
        digest: &str,
        mut body: R,
        len: u64,
    ) -> Result<BlobInfo, StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        if digest.len() != 64 {
            return Err(StoreError::BadDigest);
        }

        let lock = self.account_lock(account);
        let _guard = lock.lock().await;

        let record = self
            .account(account)?
            .ok_or_else(|| StoreError::Corrupt("аккаунт исчез".to_string()))?;
        let existing = self.blob_info(account, blob_id)?;

        // Тот же блоб с тем же содержимым: не пишем, но отмечаем использование.
        if let Some(info) = &existing {
            if info.digest == digest && info.size == len && len > 0 {
                self.bump_blob_use(account, blob_id)?;
                return self
                    .blob_info(account, blob_id)?
                    .ok_or_else(|| StoreError::Corrupt("блоб исчез".to_string()));
            }
        }

        self.check_quota(&record, existing.as_ref().map(|info| info.size), len)?;

        let temp_path = blob_temp_path(self.root(), account, blob_id);
        let final_path = paths::blob_path(self.root(), account, blob_id);

        let written = write_stream(&temp_path, &mut body, len)
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&temp_path);
            })?;
        if written != digest {
            let _ = std::fs::remove_file(&temp_path);
            return Err(StoreError::DigestMismatch);
        }

        tokio::fs::rename(&temp_path, &final_path)
            .await
            .map_err(|error| {
                let _ = std::fs::remove_file(&temp_path);
                io_error(error)
            })?;

        let created_at = existing
            .as_ref()
            .map(|info| info.created_at.clone())
            .unwrap_or_else(now);
        let last_used_at = now();
        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO blobs (account_id, blob_id, size, digest, created_at, last_used_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(account_id, blob_id) DO UPDATE SET size = ?3, digest = ?4, \
                 last_used_at = ?6",
                rusqlite::params![
                    account.to_string(),
                    blob_id,
                    len as i64,
                    digest,
                    created_at,
                    last_used_at
                ],
            )
            .map_err(db_error)?;
        let used = record
            .used_bytes
            .saturating_sub(existing.map(|info| info.size).unwrap_or(0))
            .saturating_add(len);
        transaction
            .execute(
                "UPDATE accounts SET used_bytes = ?2 WHERE account_id = ?1",
                rusqlite::params![account.to_string(), used as i64],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        drop(connection);

        Ok(BlobInfo {
            blob_id: blob_id.to_string(),
            size: len,
            digest: digest.to_string(),
            created_at,
            last_used_at,
        })
    }

    pub async fn get_blob(
        &self,
        account: AccountId,
        blob_id: &str,
    ) -> Result<Option<(BlobInfo, tokio::fs::File)>, StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        let Some(info) = self.blob_info(account, blob_id)? else {
            return Ok(None);
        };
        let file = tokio::fs::File::open(paths::blob_path(self.root(), account, blob_id))
            .await
            .map_err(io_error)?;
        Ok(Some((info, file)))
    }

    pub fn blob_info(
        &self,
        account: AccountId,
        blob_id: &str,
    ) -> Result<Option<BlobInfo>, StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        let connection = self.db()?;
        let mut statement = connection
            .prepare(
                "SELECT blob_id, size, digest, created_at, last_used_at FROM blobs \
                 WHERE account_id = ?1 AND blob_id = ?2",
            )
            .map_err(db_error)?;
        let mut rows = statement
            .query(rusqlite::params![account.to_string(), blob_id])
            .map_err(db_error)?;
        match rows.next().map_err(db_error)? {
            Some(row) => Ok(Some(read_blob(row).map_err(db_error)?)),
            None => Ok(None),
        }
    }

    pub fn list_blobs(&self, account: AccountId) -> Result<Vec<BlobInfo>, StoreError> {
        let connection = self.db()?;
        let mut statement = connection
            .prepare(
                "SELECT blob_id, size, digest, created_at, last_used_at FROM blobs \
                 WHERE account_id = ?1 ORDER BY created_at",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map(rusqlite::params![account.to_string()], read_blob)
            .map_err(db_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    /// Удаляет блоб и возвращает признак того, что файл был.
    pub fn delete_blob(&self, account: AccountId, blob_id: &str) -> Result<bool, StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        let Some(info) = self.blob_info(account, blob_id)? else {
            return Ok(false);
        };

        let _ = std::fs::remove_file(paths::blob_path(self.root(), account, blob_id));

        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM blobs WHERE account_id = ?1 AND blob_id = ?2",
                rusqlite::params![account.to_string(), blob_id],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "UPDATE accounts SET used_bytes = MAX(0, used_bytes - ?2) WHERE account_id = ?1",
                rusqlite::params![account.to_string(), info.size as i64],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(true)
    }

    pub fn bump_blob_use(&self, account: AccountId, blob_id: &str) -> Result<(), StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        self.db()?
            .execute(
                "UPDATE blobs SET last_used_at = ?3 WHERE account_id = ?1 AND blob_id = ?2",
                rusqlite::params![account.to_string(), blob_id, now()],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn usage(&self, account: AccountId) -> Result<Usage, StoreError> {
        let record = self
            .account(account)?
            .ok_or_else(|| StoreError::Corrupt("аккаунт исчез".to_string()))?;
        let connection = self.db()?;
        let (blobs_bytes, blob_count): (i64, i64) = connection
            .query_row(
                "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM blobs WHERE account_id = ?1",
                rusqlite::params![account.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;

        Ok(Usage {
            used: record.used_bytes,
            quota: record.quota_bytes,
            vault_bytes: record.vault_size,
            blobs_bytes: blobs_bytes as u64,
            blob_count: blob_count as u32,
        })
    }

    fn check_quota(
        &self,
        record: &AccountRecord,
        replaced: Option<u64>,
        len: u64,
    ) -> Result<(), StoreError> {
        let after = record
            .used_bytes
            .saturating_sub(replaced.unwrap_or(0))
            .saturating_add(len);
        if after > record.quota_bytes {
            return Err(StoreError::QuotaExceeded {
                used: record.used_bytes,
                quota: record.quota_bytes,
            });
        }
        Ok(())
    }
}

fn blob_temp_path(root: &Path, account: AccountId, blob_id: &str) -> std::path::PathBuf {
    paths::blobs_dir(root, account).join(format!("{blob_id}.tmp"))
}

fn read_blob(row: &rusqlite::Row<'_>) -> rusqlite::Result<BlobInfo> {
    Ok(BlobInfo {
        blob_id: row.get(0)?,
        size: row.get::<_, i64>(1)? as u64,
        digest: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
    })
}

/// Пишет ровно `len` байт и возвращает BLAKE3 в hex.
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
