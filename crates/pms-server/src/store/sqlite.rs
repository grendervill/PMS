//! SQLite-индекс: аккаунты, блобы, сессии, nonce. Только метаданные — байты
//! сейфа и вложений лежат файлами, сервер их не разбирает.

use std::path::Path;

use rusqlite::Connection;

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    account_id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL,
    quota_bytes INTEGER NOT NULL,
    used_bytes INTEGER NOT NULL DEFAULT 0,
    vault_version INTEGER NOT NULL DEFAULT 0,
    vault_digest TEXT NOT NULL DEFAULT '',
    vault_size INTEGER NOT NULL DEFAULT 0,
    login TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_auth_at TEXT
);

CREATE TABLE IF NOT EXISTS blobs (
    account_id TEXT NOT NULL,
    blob_id TEXT NOT NULL,
    size INTEGER NOT NULL,
    digest TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    PRIMARY KEY (account_id, blob_id),
    FOREIGN KEY (account_id) REFERENCES accounts(account_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS sessions (
    token_hash BLOB PRIMARY KEY,
    account_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    FOREIGN KEY (account_id) REFERENCES accounts(account_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS nonces (
    account_id TEXT NOT NULL,
    nonce BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, nonce)
);

CREATE INDEX IF NOT EXISTS blobs_by_account ON blobs(account_id);
CREATE INDEX IF NOT EXISTS sessions_by_account ON sessions(account_id, expires_at);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.busy_timeout(std::time::Duration::from_millis(1000))?;
    connection.execute_batch(SCHEMA)?;
    migrate(&connection)?;
    Ok(connection)
}

/// Миграции поверх уже существующего индекса.
///
/// `CREATE TABLE IF NOT EXISTS` не добавляет колонки в старую базу, поэтому
/// новые поля появляются здесь, а не в схеме.
fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    let has_login: bool = connection
        .prepare("SELECT 1 FROM pragma_table_info('accounts') WHERE name = 'login'")?
        .exists([])?;
    if !has_login {
        connection.execute("ALTER TABLE accounts ADD COLUMN login TEXT", [])?;
    }
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS accounts_by_login ON accounts(login)",
        [],
    )?;
    Ok(())
}
