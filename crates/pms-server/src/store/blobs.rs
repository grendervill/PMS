//! Запечатанные вложения. Клиент присылает уже зашифрованные байты и BLAKE3,
//! сервер их проверяет, кладёт файлом и ведёт учёт места.

use std::path::Path;

use pms_proto::dto::{BlobInfo, Usage};
use pms_proto::identity::AccountId;
use tokio::io::AsyncRead;

use super::paths::{self, is_blob_id};
use super::temp::TempFile;
use super::{db_error, io_error, now, AccountRecord, Store, StoreError};

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

        // Тот же блоб с тем же содержимым: не пишем и не трогаем индекс, если
        // отметка использования ещё свежая (иначе каждая повторная загрузка и
        // каждое скачивание генерируют запись в WAL).
        if let Some(info) = &existing {
            if info.digest == digest && info.size == len && len > 0 {
                self.bump_blob_use(account, blob_id)?;
                return Ok(info.clone());
            }
        }

        self.check_quota(&record, existing.as_ref().map(|info| info.size), len)?;

        let temp_path = blob_temp_path(self.root(), account, blob_id);
        let final_path = paths::blob_path(self.root(), account, blob_id);

        let _reservation = self.reserve(len)?;
        let guard = TempFile::at(temp_path.clone());
        let policy = self.fsync_policy();
        let written = super::hash_stream_to_file(&temp_path, &mut body, len, policy).await?;
        if written != digest {
            return Err(StoreError::DigestMismatch);
        }

        tokio::fs::rename(&temp_path, &final_path)
            .await
            .map_err(io_error)?;
        drop(guard);

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
        self.with_read(|connection| {
            let mut statement = connection
                .prepare_cached(
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
        })
    }

    pub fn list_blobs(&self, account: AccountId) -> Result<Vec<BlobInfo>, StoreError> {
        self.with_read(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT blob_id, size, digest, created_at, last_used_at FROM blobs \
                     WHERE account_id = ?1 ORDER BY created_at",
                )
                .map_err(db_error)?;
            let rows = statement
                .query_map(rusqlite::params![account.to_string()], read_blob)
                .map_err(db_error)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
        })
    }

    /// Удаляет блоб и возвращает признак того, что файл был.
    pub fn delete_blob(&self, account: AccountId, blob_id: &str) -> Result<bool, StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        let Some(info) = self.blob_info(account, blob_id)? else {
            return Ok(false);
        };

        match std::fs::remove_file(paths::blob_path(self.root(), account, blob_id)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(account = %account, blob_id, %error, "не удалось удалить файл блоба");
            }
        }

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

    /// Обновляет отметку использования, но не чаще раза в час: иначе скачивание
    /// одного и того же вложения пишет в WAL на каждый GET.
    pub fn bump_blob_use(&self, account: AccountId, blob_id: &str) -> Result<(), StoreError> {
        if !is_blob_id(blob_id) {
            return Err(StoreError::BadBlobId);
        }
        let threshold = (chrono::Utc::now() - chrono::Duration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.db()?
            .prepare_cached(
                "UPDATE blobs SET last_used_at = ?3 \
                 WHERE account_id = ?1 AND blob_id = ?2 AND last_used_at < ?4",
            )
            .map_err(db_error)?
            .execute(rusqlite::params![
                account.to_string(),
                blob_id,
                now(),
                threshold
            ])
            .map_err(db_error)?;
        Ok(())
    }

    pub fn usage(&self, account: AccountId) -> Result<Usage, StoreError> {
        let record = self
            .account(account)?
            .ok_or_else(|| StoreError::Corrupt("аккаунт исчез".to_string()))?;
        let (blobs_bytes, blob_count): (i64, i64) = self.with_read(|connection| {
            connection
                .query_row(
                    "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM blobs WHERE account_id = ?1",
                    rusqlite::params![account.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(db_error)
        })?;

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
