use std::sync::Arc;

use pms_proto::identity::AccountId;
use pms_server::store::vault::ExpectedVersion;
use pms_server::store::{Store, StoreError};

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn blob_id(index: u8) -> String {
    format!("0b7c1d2e-3f40-4516-8798-a9bacbdced{index:02x}")
}

#[tokio::test]
async fn quota_counts_vault_and_blobs_together() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), 1_000).unwrap();
    let account = AccountId::new([0x31; 16]);
    store.create_account(account, [1u8; 32], 1_000).unwrap();

    let vault = vec![1u8; 400];
    store
        .put_vault(
            account,
            ExpectedVersion::Any,
            vault.as_slice(),
            vault.len() as u64,
        )
        .await
        .unwrap();

    let first = vec![2u8; 500];
    store
        .put_blob(
            account,
            &blob_id(1),
            &digest(&first),
            first.as_slice(),
            first.len() as u64,
        )
        .await
        .unwrap();

    let second = vec![3u8; 200];
    let refused = store
        .put_blob(
            account,
            &blob_id(2),
            &digest(&second),
            second.as_slice(),
            second.len() as u64,
        )
        .await;
    assert!(matches!(refused, Err(StoreError::QuotaExceeded { .. })));

    let usage = store.usage(account).unwrap();
    assert_eq!(usage.used, 900);
    assert_eq!(usage.quota, 1_000);
    assert_eq!(usage.vault_bytes, 400);
    assert_eq!(usage.blobs_bytes, 500);
    assert_eq!(usage.blob_count, 1);
}

#[tokio::test]
async fn parallel_blob_uploads_cannot_exceed_quota() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path(), 1_000).unwrap());
    let account = AccountId::new([0x32; 16]);
    store.create_account(account, [1u8; 32], 1_000).unwrap();

    let mut handles = Vec::new();
    for index in 0..8u8 {
        let store = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let body = vec![index; 300];
            store
                .put_blob(
                    account,
                    &blob_id(index),
                    &digest(&body),
                    body.as_slice(),
                    body.len() as u64,
                )
                .await
        }));
    }

    let mut ok = 0;
    let mut refused = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => ok += 1,
            Err(StoreError::QuotaExceeded { .. }) => refused += 1,
            Err(other) => panic!("неожиданная ошибка: {other}"),
        }
    }

    assert_eq!(ok, 3, "в 1000 байт помещается ровно три блоба по 300");
    assert_eq!(refused, 5);
    assert!(store.usage(account).unwrap().used <= 1_000);
}
