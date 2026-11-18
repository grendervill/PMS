//! Раскладка каталогов и проверка идентификаторов.
//!
//! Идентификаторы попадают в пути, поэтому они валидируются, а не берутся как
//! есть: `blob_id` — ровно UUID в нижнем регистре, `account_id` — 32 hex.

use std::path::{Path, PathBuf};

use pms_proto::identity::AccountId;

pub const VAULT_FILE: &str = "vault.bin";
pub const VAULT_TMP_FILE: &str = "vault.bin.tmp";
pub const BLOBS_DIR: &str = "blobs";
pub const DB_FILE: &str = "index.sqlite";

pub fn db_path(root: &Path) -> PathBuf {
    root.join(DB_FILE)
}

/// `<root>/<первые 2 hex>/<32 hex>` — раскладка на 256 подкаталогов.
pub fn account_dir(root: &Path, account: AccountId) -> PathBuf {
    let hex = account.to_string();
    root.join(&hex[..2]).join(hex)
}

pub fn blobs_dir(root: &Path, account: AccountId) -> PathBuf {
    account_dir(root, account).join(BLOBS_DIR)
}

pub fn vault_path(root: &Path, account: AccountId) -> PathBuf {
    account_dir(root, account).join(VAULT_FILE)
}

pub fn vault_tmp_path(root: &Path, account: AccountId) -> PathBuf {
    account_dir(root, account).join(VAULT_TMP_FILE)
}

pub fn blob_path(root: &Path, account: AccountId, blob_id: &str) -> PathBuf {
    blobs_dir(root, account).join(blob_id)
}

/// UUID без учёта дефисов, только нижний hex: 32 символа.
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
