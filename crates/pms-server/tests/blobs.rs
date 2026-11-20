use pms_proto::identity::AccountId;
use pms_server::store::{paths, Store, StoreError};
use tokio::io::AsyncReadExt;

const BLOB_ID: &str = "0b7c1d2e-3f40-4516-8798-a9bacbdcedfe";

fn setup(quota: u64) -> (tempfile::TempDir, Store, AccountId) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), quota).unwrap();
    let account = AccountId::new([0x21; 16]);
    store
        .create_account(account, [1u8; 32], quota, "alice")
        .unwrap();
    (dir, store, account)
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[tokio::test]
async fn same_blob_twice_is_noop_and_keeps_size() {
    let (_dir, store, account) = setup(10_000);
    let body = b"attachment-bytes".to_vec();
    let hash = digest(&body);

    let first = store
        .put_blob(account, BLOB_ID, &hash, body.as_slice(), body.len() as u64)
        .await
        .unwrap();
    let second = store
        .put_blob(account, BLOB_ID, &hash, body.as_slice(), body.len() as u64)
        .await
        .unwrap();

    assert_eq!(first.size, body.len() as u64);
    assert_eq!(second.size, first.size);
    assert_eq!(store.usage(account).unwrap().used, body.len() as u64);
    assert_eq!(store.usage(account).unwrap().blob_count, 1);
}

#[tokio::test]
async fn digest_mismatch_is_refused_and_leaves_nothing() {
    let (dir, store, account) = setup(10_000);
    let body = b"real-bytes".to_vec();
    let wrong = digest(b"other-bytes");

    let result = store
        .put_blob(account, BLOB_ID, &wrong, body.as_slice(), body.len() as u64)
        .await;

    assert!(matches!(result, Err(StoreError::DigestMismatch)));
    assert_eq!(store.usage(account).unwrap().used, 0);
    let entries: Vec<String> = std::fs::read_dir(paths::blobs_dir(dir.path(), account))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(entries.is_empty(), "каталог блобов не пуст: {entries:?}");
}

#[tokio::test]
async fn invalid_blob_id_is_refused() {
    let (_dir, store, account) = setup(10_000);
    let body = b"x".to_vec();
    let result = store
        .put_blob(
            account,
            "../../etc/passwd",
            &digest(&body),
            body.as_slice(),
            body.len() as u64,
        )
        .await;
    assert!(matches!(result, Err(StoreError::BadBlobId)));
}

#[tokio::test]
async fn read_back_and_bump_last_used() {
    let (_dir, store, account) = setup(10_000);
    let body = b"read-me".to_vec();
    store
        .put_blob(
            account,
            BLOB_ID,
            &digest(&body),
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();

    let before = store.blob_info(account, BLOB_ID).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    store.bump_blob_use(account, BLOB_ID).unwrap();
    let after = store.blob_info(account, BLOB_ID).unwrap().unwrap();
    assert_ne!(before.last_used_at, after.last_used_at);
    assert_eq!(before.created_at, after.created_at);

    let (info, mut file) = store.get_blob(account, BLOB_ID).await.unwrap().unwrap();
    let mut read = Vec::new();
    file.read_to_end(&mut read).await.unwrap();
    assert_eq!(read, body);
    assert_eq!(info.size, body.len() as u64);
}

#[tokio::test]
async fn delete_blob_frees_quota_and_is_idempotent() {
    let (_dir, store, account) = setup(10_000);
    let body = b"delete-me".to_vec();
    store
        .put_blob(
            account,
            BLOB_ID,
            &digest(&body),
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();

    assert!(store.delete_blob(account, BLOB_ID).unwrap());
    assert!(!store.delete_blob(account, BLOB_ID).unwrap());
    assert_eq!(store.usage(account).unwrap().used, 0);
    assert!(store.list_blobs(account).unwrap().is_empty());
}
