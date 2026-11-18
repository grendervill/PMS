use pms_proto::identity::AccountId;
use pms_server::store::{paths, Store};

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn account_dir_is_private_and_sharded() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 1024).unwrap();
    let account = AccountId::new([0xab; 16]);

    assert!(store.create_account(account, [1u8; 32], 1024).unwrap());

    let path = paths::account_dir(dir.path(), account);
    assert!(path.ends_with("ab/abababababababababababababababab"));
    assert!(path.is_dir());
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o700);
}

#[test]
fn create_account_twice_reports_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 1024).unwrap();
    let account = AccountId::new([7u8; 16]);

    assert!(store.create_account(account, [2u8; 32], 1024).unwrap());
    assert!(!store.create_account(account, [2u8; 32], 1024).unwrap());
}

#[test]
fn account_record_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 4096).unwrap();
    let account = AccountId::new([9u8; 16]);

    assert_eq!(store.account(account).unwrap(), None);
    store.create_account(account, [3u8; 32], 4096).unwrap();

    let record = store.account(account).unwrap().unwrap();
    assert_eq!(record.account_id, account);
    assert_eq!(record.public_key, [3u8; 32]);
    assert_eq!(record.quota_bytes, 4096);
    assert_eq!(record.used_bytes, 0);
    assert_eq!(record.vault_version, 0);
}

#[test]
fn reopen_recounts_used_bytes_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let account = AccountId::new([4u8; 16]);
    {
        let store = Store::open(dir.path(), 4096).unwrap();
        store.create_account(account, [4u8; 32], 4096).unwrap();
    }

    std::fs::write(paths::vault_path(dir.path(), account), vec![0u8; 100]).unwrap();
    std::fs::write(
        paths::blobs_dir(dir.path(), account).join("0b7c1d2e-3f40-4516-8798-a9bacbdcedfe"),
        vec![0u8; 50],
    )
    .unwrap();

    let store = Store::open(dir.path(), 4096).unwrap();
    let record = store.account(account).unwrap().unwrap();
    assert_eq!(record.used_bytes, 150);
}

#[test]
fn blob_ids_are_validated() {
    assert!(paths::is_blob_id("0b7c1d2e-3f40-4516-8798-a9bacbdcedfe"));
    assert!(paths::is_blob_id("0b7c1d2e3f4045168798a9bacbdcedfe"));
    assert!(!paths::is_blob_id("../../etc/passwd"));
    assert!(!paths::is_blob_id("0b7c1d2e-3f40-4516-8798-a9bacbdcedf"));
    assert!(!paths::is_blob_id("0B7C1D2E-3F40-4516-8798-A9BACBDCEDFE"));
}
