//! Хранилище: файлы на диске плюс SQLite-индекс.
//!
//! Слой ничего не знает про HTTP. Соединение с SQLite одно и под мьютексом:
//! запросы — короткие обращения к индексу, а потоковые операции с байтами идут
//! мимо него и не держат блокировку.

pub mod blobs;
pub mod paths;
pub mod sqlite;
pub mod staging;
pub mod state;
pub mod temp;
pub mod vault;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use pms_proto::identity::AccountId;
use rusqlite::Connection;

pub use paths::is_blob_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    pub account_id: AccountId,
    /// Логин на этом сервере. `None` у аккаунтов прототипа без логина.
    pub login: Option<String>,
    pub public_key: [u8; 32],
    pub quota_bytes: u64,
    pub used_bytes: u64,
    pub vault_version: u64,
    pub vault_digest: String,
    pub vault_size: u64,
    pub updated_at: String,
}

/// Строка для CLI: идентификатор, логин, занятое место, квота.
pub type AccountRow = (AccountId, Option<String>, u64, u64);

pub struct Store {
    root: PathBuf,
    default_quota: u64,
    db: Mutex<Connection>,
    locks: Mutex<HashMap<AccountId, Arc<tokio::sync::Mutex<()>>>>,
}

impl Store {
    /// Открывает (или создаёт) хранилище и приводит `used_bytes` в соответствие
    /// с тем, что реально лежит на диске.
    pub fn open(root: &Path, default_quota: u64) -> Result<Self, StoreError> {
        create_private_dir(root)?;

        let connection = sqlite::open(&paths::db_path(root)).map_err(db_error)?;
        let store = Self {
            root: root.to_path_buf(),
            default_quota,
            db: Mutex::new(connection),
            locks: Mutex::new(HashMap::new()),
        };
        store.recount()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Квота для новых аккаунтов, если для конкретного не задана иная.
    pub fn default_quota(&self) -> u64 {
        self.default_quota
    }

    fn db(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.db.lock().map_err(|_| StoreError::Locked)
    }

    /// Возвращает блокировку записи для аккаунта. Карта мьютексов держится
    /// только на время выдачи `Arc`, сама запись — на весь срок операции.
    pub fn account_lock(&self, account: AccountId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().expect("locks mutex poisoned");
        locks
            .entry(account)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    pub fn create_account(
        &self,
        account: AccountId,
        public_key: [u8; 32],
        quota_bytes: u64,
        login: &str,
    ) -> Result<bool, StoreError> {
        let directory = paths::account_dir(&self.root, account);
        create_private_dir(&directory)?;
        create_private_dir(&paths::blobs_dir(&self.root, account))?;

        let now = now();
        let inserted = self
            .db()?
            .execute(
                "INSERT INTO accounts (account_id, public_key, quota_bytes, used_bytes, \
                 vault_version, vault_digest, vault_size, created_at, updated_at, login) \
                 VALUES (?1, ?2, ?3, 0, 0, '', 0, ?4, ?4, ?5) \
                 ON CONFLICT(account_id) DO NOTHING",
                rusqlite::params![
                    account.to_string(),
                    public_key.to_vec(),
                    quota_bytes as i64,
                    now,
                    login
                ],
            )
            .map_err(db_error)?;
        Ok(inserted == 1)
    }

    /// Ищет аккаунт по логину (без учёта регистра: логин нормализован при записи).
    pub fn account_by_login(&self, login: &str) -> Result<Option<AccountRecord>, StoreError> {
        let account = {
            let connection = self.db()?;
            let mut statement = connection
                .prepare("SELECT account_id FROM accounts WHERE login = ?1")
                .map_err(db_error)?;
            let mut rows = statement
                .query(rusqlite::params![login.to_ascii_lowercase()])
                .map_err(db_error)?;
            match rows.next().map_err(db_error)? {
                Some(row) => {
                    let hex: String = row.get(0).map_err(db_error)?;
                    AccountId::from_hex(&hex)
                }
                None => None,
            }
        };
        match account {
            Some(account) => self.account(account),
            None => Ok(None),
        }
    }

    pub fn account(&self, account: AccountId) -> Result<Option<AccountRecord>, StoreError> {
        let connection = self.db()?;
        let mut statement = connection
            .prepare(
                "SELECT account_id, public_key, quota_bytes, used_bytes, vault_version, \
                 vault_digest, vault_size, updated_at, login FROM accounts WHERE account_id = ?1",
            )
            .map_err(db_error)?;
        let mut rows = statement
            .query(rusqlite::params![account.to_string()])
            .map_err(db_error)?;

        match rows.next().map_err(db_error)? {
            Some(row) => Ok(Some(read_account(row, account)?)),
            None => Ok(None),
        }
    }

    /// Список аккаунтов для CLI: идентификатор, логин, занятое место, квота.
    pub fn list_accounts(&self) -> Result<Vec<AccountRow>, StoreError> {
        let connection = self.db()?;
        let mut statement = connection
            .prepare("SELECT account_id, login, used_bytes, quota_bytes FROM accounts ORDER BY created_at")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                ))
            })
            .map_err(db_error)?;

        let mut out = Vec::new();
        for row in rows {
            let (hex, login, used, quota) = row.map_err(db_error)?;
            if let Some(account) = AccountId::from_hex(&hex) {
                out.push((account, login, used, quota));
            }
        }
        Ok(out)
    }

    /// Меняет квоту аккаунта. Админка не делается: квота правится CLI.
    pub fn set_quota(&self, account: AccountId, quota_bytes: u64) -> Result<bool, StoreError> {
        let updated = self
            .db()?
            .execute(
                "UPDATE accounts SET quota_bytes = ?2 WHERE account_id = ?1",
                rusqlite::params![account.to_string(), quota_bytes as i64],
            )
            .map_err(db_error)?;
        Ok(updated == 1)
    }

    /// Удаляет аккаунт целиком: каталог с байтами и все строки индекса.
    pub fn delete_account(&self, account: AccountId) -> Result<bool, StoreError> {
        if self.account(account)?.is_none() {
            return Ok(false);
        }
        match std::fs::remove_dir_all(paths::account_dir(&self.root, account)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(account = %account, %error, "не удалось удалить каталог аккаунта");
            }
        }

        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        for table in ["nonces", "sessions", "blobs"] {
            transaction
                .execute(
                    &format!("DELETE FROM {table} WHERE account_id = ?1"),
                    rusqlite::params![account.to_string()],
                )
                .map_err(db_error)?;
        }
        transaction
            .execute(
                "DELETE FROM accounts WHERE account_id = ?1",
                rusqlite::params![account.to_string()],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(true)
    }

    /// Пересчитывает занятое место по файловой системе и убирает из индекса
    /// блобы, чьих файлов больше нет. Вызывается при старте: после сбоя индекс
    /// мог разойтись с диском, и квота должна опираться на реальность.
    fn recount(&self) -> Result<(), StoreError> {
        let accounts: Vec<AccountId> = {
            let connection = self.db()?;
            let mut statement = connection
                .prepare("SELECT account_id FROM accounts")
                .map_err(db_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(db_error)?;
            let mut accounts = Vec::new();
            for row in rows {
                let text = row.map_err(db_error)?;
                if let Some(account) = AccountId::from_hex(&text) {
                    accounts.push(account);
                }
            }
            accounts
        };

        for account in accounts {
            let vault_size = std::fs::metadata(paths::vault_path(&self.root, account))
                .map(|meta| meta.len())
                .unwrap_or(0);

            let mut present = HashSet::new();
            let mut blobs_total = 0u64;
            if let Ok(entries) = std::fs::read_dir(paths::blobs_dir(&self.root, account)) {
                for entry in entries.flatten() {
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    if !meta.is_file() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    // Временные файлы прерванных загрузок не считаются блобами.
                    if name.ends_with(".tmp") {
                        let _ = std::fs::remove_file(entry.path());
                        continue;
                    }
                    blobs_total += meta.len();
                    present.insert(name);
                }
            }
            if let Ok(entries) = std::fs::read_dir(paths::account_dir(&self.root, account)) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".tmp") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }

            let connection = self.db()?;
            let known: Vec<String> = {
                let mut statement = connection
                    .prepare("SELECT blob_id FROM blobs WHERE account_id = ?1")
                    .map_err(db_error)?;
                let rows = statement
                    .query_map(rusqlite::params![account.to_string()], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(db_error)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(db_error)?
            };
            for blob_id in known {
                if !present.contains(&blob_id) {
                    connection
                        .execute(
                            "DELETE FROM blobs WHERE account_id = ?1 AND blob_id = ?2",
                            rusqlite::params![account.to_string(), blob_id],
                        )
                        .map_err(db_error)?;
                }
            }

            connection
                .execute(
                    "UPDATE accounts SET used_bytes = ?2, vault_size = ?3, \
                     vault_version = CASE WHEN ?3 = 0 THEN 0 ELSE vault_version END, \
                     vault_digest = CASE WHEN ?3 = 0 THEN '' ELSE vault_digest END \
                     WHERE account_id = ?1",
                    rusqlite::params![
                        account.to_string(),
                        (vault_size + blobs_total) as i64,
                        vault_size as i64
                    ],
                )
                .map_err(db_error)?;
        }

        Ok(())
    }
}

fn read_account(row: &rusqlite::Row<'_>, account: AccountId) -> Result<AccountRecord, StoreError> {
    let public_key: Vec<u8> = row.get(1).map_err(db_error)?;
    if public_key.len() != 32 {
        return Err(StoreError::Corrupt("public_key не 32 байта".to_string()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&public_key);

    Ok(AccountRecord {
        account_id: account,
        login: row.get(8).map_err(db_error)?,
        public_key: key,
        quota_bytes: row.get::<_, i64>(2).map_err(db_error)? as u64,
        used_bytes: row.get::<_, i64>(3).map_err(db_error)? as u64,
        vault_version: row.get::<_, i64>(4).map_err(db_error)? as u64,
        vault_digest: row.get(5).map_err(db_error)?,
        vault_size: row.get::<_, i64>(6).map_err(db_error)? as u64,
        updated_at: row.get(7).map_err(db_error)?,
    })
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(crate) fn create_private_dir(path: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(path).map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(path, permissions).map_err(io_error)?;
    }
    Ok(())
}

/// Пишет ровно `len` байт из `body` в `path` и возвращает BLAKE3 в hex.
///
/// Один буфер на 64 КиБ и никакой буферизации целиком: именно это позволяет
/// держать память сервера независимой от размера сейфа.
pub(crate) async fn hash_stream_to_file<R: tokio::io::AsyncRead + Unpin>(
    path: &Path,
    body: &mut R,
    len: u64,
) -> Result<String, StoreError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const CHUNK: usize = 64 * 1024;

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

pub(crate) fn io_error(error: std::io::Error) -> StoreError {
    StoreError::Io(error.to_string())
}

pub(crate) fn db_error(error: rusqlite::Error) -> StoreError {
    StoreError::Db(error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("ошибка ввода-вывода: {0}")]
    Io(String),
    #[error("ошибка индекса: {0}")]
    Db(String),
    #[error("хранилище заблокировано")]
    Locked,
    #[error("данные повреждены: {0}")]
    Corrupt(String),
    #[error("версия сейфа устарела, на сервере {0:?}")]
    Conflict(pms_proto::dto::VaultMeta),
    #[error("версия сейфа в будущем: клиент прислал неправильный If-Match")]
    PreconditionFailed,
    #[error("квота исчерпана: занято {used} из {quota}")]
    QuotaExceeded { used: u64, quota: u64 },
    #[error("тело короче заявленного: ожидалось {expected}, получено {got}")]
    Truncated { expected: u64, got: u64 },
    #[error("BLAKE3 не совпал с заявленным")]
    DigestMismatch,
    #[error("недопустимый идентификатор блоба")]
    BadBlobId,
    #[error("недопустимый BLAKE3-дайджест")]
    BadDigest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn account_lock_is_exclusive_and_per_account() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), 1024).unwrap();
        let first = AccountId::new([1u8; 16]);
        let second = AccountId::new([2u8; 16]);

        let guard = store.account_lock(first).lock_owned().await;
        assert!(
            store.account_lock(first).try_lock_owned().is_err(),
            "второй писатель в тот же аккаунт должен ждать"
        );
        assert!(
            store.account_lock(second).try_lock_owned().is_ok(),
            "другой аккаунт не должен ждать"
        );
        drop(guard);
        assert!(store.account_lock(first).try_lock_owned().is_ok());
    }
}
