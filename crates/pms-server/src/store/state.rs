//! Nonce одноразовых challenge и сессии. В базе лежат только хеши токенов.

use pms_proto::identity::{AccountId, DeviceId};

use super::{db_error, Store, StoreError};

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
        self.db()?
            .execute(
                "INSERT INTO nonces (account_id, nonce, expires_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![account.to_string(), nonce.to_vec(), expires_at],
            )
            .map_err(db_error)?;
        Ok(())
    }

    /// Забирает nonce из индекса и сообщает, был ли он и не истёк ли.
    ///
    /// Удаление и проверка идут в одной транзакции: повторное использование
    /// невозможно даже при параллельных запросах.
    pub fn take_nonce(&self, account: AccountId, nonce: &[u8; 32]) -> Result<bool, StoreError> {
        let mut connection = self.db()?;
        let transaction = connection.transaction().map_err(db_error)?;
        let expires_at: Option<i64> = transaction
            .query_row(
                "SELECT expires_at FROM nonces WHERE account_id = ?1 AND nonce = ?2",
                rusqlite::params![account.to_string(), nonce.to_vec()],
                |row| row.get(0),
            )
            .ok();
        transaction
            .execute(
                "DELETE FROM nonces WHERE account_id = ?1 AND nonce = ?2",
                rusqlite::params![account.to_string(), nonce.to_vec()],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;

        Ok(matches!(expires_at, Some(expires_at) if expires_at > chrono::Utc::now().timestamp()))
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
        transaction
            .execute(
                "DELETE FROM sessions WHERE token_hash IN (\
                   SELECT token_hash FROM sessions WHERE account_id = ?1 \
                   ORDER BY expires_at DESC LIMIT -1 OFFSET ?2)",
                rusqlite::params![account.to_string(), max_sessions as i64],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "DELETE FROM sessions WHERE expires_at <= ?1",
                rusqlite::params![chrono::Utc::now().timestamp()],
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
