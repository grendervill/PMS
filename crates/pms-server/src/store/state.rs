//! Nonce одноразовых challenge и сессии. В базе лежат только хеши токенов.

use pms_proto::identity::{AccountId, DeviceId};

use rusqlite::OptionalExtension;

use super::{db_error, Store, StoreError};

/// Что стало с одноразовым nonce при попытке его забрать.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceStatus {
    Valid,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRecord {
    pub account: AccountId,
    pub device: DeviceId,
    pub expires_at: i64,
}

impl Store {
    pub fn insert_nonce(
        &self,
        account: AccountId,
        nonce: &[u8; 32],
        expires_at: i64,
    ) -> Result<(), StoreError> {
        let connection = self.db()?;
        let account_hex = account.to_string();
        // Один активный nonce на аккаунт: вход — двухшаговый, старая попытка
        // всё равно не нужна, а таблица перестаёт расти.
        connection
            .execute(
                "DELETE FROM nonces WHERE account_id = ?1",
                rusqlite::params![&account_hex],
            )
            .map_err(db_error)?;
        connection
            .execute(
                "INSERT INTO nonces (account_id, nonce, expires_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![account_hex, nonce.to_vec(), expires_at],
            )
            .map_err(db_error)?;

        // Глобальную чистку делаем не на каждой вставке: иначе это лишний
        // write-запрос (и полный скан до появления индекса) на каждый challenge.
        let count = self
            .inserts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count.is_multiple_of(64) {
            connection
                .execute(
                    "DELETE FROM nonces WHERE expires_at <= ?1",
                    rusqlite::params![chrono::Utc::now().timestamp()],
                )
                .map_err(db_error)?;
        }
        Ok(())
    }

    /// Забирает nonce из индекса и сообщает, что с ним было.
    ///
    /// Удаление и проверка идут в одной транзакции: повторное использование
    /// невозможно даже при параллельных запросах.
    pub fn take_nonce(
        &self,
        account: AccountId,
        nonce: &[u8; 32],
    ) -> Result<NonceStatus, StoreError> {
        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT expires_at FROM nonces WHERE account_id = ?1 AND nonce = ?2",
                rusqlite::params![account.to_string(), nonce.to_vec()],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM nonces WHERE account_id = ?1 AND nonce = ?2",
                rusqlite::params![account.to_string(), nonce.to_vec()],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;

        let now = chrono::Utc::now().timestamp();
        Ok(match existing {
            Some(expires_at) if expires_at > now => NonceStatus::Valid,
            Some(_) => NonceStatus::Expired,
            None => NonceStatus::Unknown,
        })
    }

    pub fn insert_session(
        &self,
        account: AccountId,
        device: DeviceId,
        token_hash: &[u8; 32],
        expires_at: i64,
        max_sessions: u32,
    ) -> Result<(), StoreError> {
        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO sessions (token_hash, account_id, device_id, expires_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    token_hash.to_vec(),
                    account.to_string(),
                    device.to_string(),
                    expires_at
                ],
            )
            .map_err(db_error)?;
        // Вытесняем по порядку вставки (`rowid`), а не по `expires_at`: при двух
        // входах в одну секунду старая сортировка выбирала устройство случайно.
        transaction
            .execute(
                "DELETE FROM sessions WHERE token_hash IN (\
                   SELECT token_hash FROM sessions WHERE account_id = ?1 \
                   ORDER BY rowid DESC LIMIT -1 OFFSET ?2)",
                rusqlite::params![account.to_string(), max_sessions as i64],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(())
    }

    pub fn session(&self, token_hash: &[u8; 32]) -> Result<Option<SessionRecord>, StoreError> {
        let connection = self.db()?;
        let mut statement = connection
            .prepare("SELECT account_id, device_id, expires_at FROM sessions WHERE token_hash = ?1")
            .map_err(db_error)?;
        let mut rows = statement
            .query(rusqlite::params![token_hash.to_vec()])
            .map_err(db_error)?;
        let Some(row) = rows.next().map_err(db_error)? else {
            return Ok(None);
        };
        let account_hex: String = row.get(0).map_err(db_error)?;
        let device_hex: String = row.get(1).map_err(db_error)?;
        let expires_at: i64 = row.get(2).map_err(db_error)?;

        let account = AccountId::from_hex(&account_hex)
            .ok_or_else(|| StoreError::Corrupt("account_id в сессии".to_string()))?;
        let device = DeviceId::from_hex(&device_hex)
            .ok_or_else(|| StoreError::Corrupt("device_id в сессии".to_string()))?;
        Ok(Some(SessionRecord {
            account,
            device,
            expires_at,
        }))
    }

    /// Удаляет все просроченные сессии (вызывается редко, по таймеру).
    pub fn delete_expired_sessions(&self, now: i64) -> Result<usize, StoreError> {
        self.db()?
            .execute(
                "DELETE FROM sessions WHERE expires_at <= ?1",
                rusqlite::params![now],
            )
            .map_err(db_error)
    }

    pub fn delete_session(&self, token_hash: &[u8; 32]) -> Result<(), StoreError> {
        self.db()?
            .execute(
                "DELETE FROM sessions WHERE token_hash = ?1",
                rusqlite::params![token_hash.to_vec()],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn delete_sessions(&self, account: AccountId) -> Result<(), StoreError> {
        self.db()?
            .execute(
                "DELETE FROM sessions WHERE account_id = ?1",
                rusqlite::params![account.to_string()],
            )
            .map_err(db_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expired_nonces_are_pruned_on_insert() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), 1024).unwrap();
        let account = AccountId::new([0x61; 16]);
        store
            .create_account(account, [1u8; 32], 1024, "alice")
            .unwrap();

        for index in 0..200u8 {
            let mut nonce = [0u8; 32];
            nonce[0] = index;
            store.insert_nonce(account, &nonce, 1).unwrap();
        }

        let connection = store.db().unwrap();
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM nonces", [], |row| row.get(0))
            .unwrap();
        assert!(rows <= 1, "просроченные nonce копятся: {rows}");
    }
}
