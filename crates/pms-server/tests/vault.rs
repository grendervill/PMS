use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use pms_proto::identity::AccountId;
use pms_server::store::vault::ExpectedVersion;
use pms_server::store::{paths, Store, StoreError};
use tokio::io::{AsyncRead, ReadBuf};

fn setup() -> (tempfile::TempDir, Arc<Store>, AccountId) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path(), 1_000_000).unwrap());
    let account = AccountId::new([0x11; 16]);
    store.create_account(account, [1u8; 32], 1_000_000).unwrap();
    (dir, store, account)
}

#[tokio::test]
async fn first_put_is_version_one_and_second_needs_matching_version() {
    let (_dir, store, account) = setup();
    let body = b"sealed-vault-bytes".to_vec();

    let first = store
        .put_vault(
            account,
            ExpectedVersion::Any,
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();
    assert_eq!(first.version, 1);
    assert_eq!(first.size, body.len() as u64);

    let stale = store
        .put_vault(account, ExpectedVersion::Version(0), body.as_slice(), 1)
        .await;
    assert!(matches!(stale, Err(StoreError::Conflict(meta)) if meta.version == 1));

    let second = store
        .put_vault(
            account,
            ExpectedVersion::Version(1),
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();
    assert_eq!(second.version, 2);
}

#[tokio::test]
async fn concurrent_puts_with_same_version_allow_exactly_one_winner() {
    let (_dir, store, account) = setup();
    let body = b"one".to_vec();
    store
        .put_vault(
            account,
            ExpectedVersion::Any,
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();

    let mut handles = Vec::new();
    for index in 0..8u8 {
        let store = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let payload = vec![index; 32];
            store
                .put_vault(
                    account,
                    ExpectedVersion::Version(1),
                    payload.as_slice(),
                    payload.len() as u64,
                )
                .await
        }));
    }

    let mut ok = 0;
    let mut conflicts = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(meta) => {
                assert_eq!(meta.version, 2);
                ok += 1;
            }
            Err(StoreError::Conflict(meta)) => {
                assert_eq!(meta.version, 2);
                conflicts += 1;
            }
            Err(other) => panic!("неожиданная ошибка: {other}"),
        }
    }
    assert_eq!((ok, conflicts), (1, 7));
}

#[tokio::test]
async fn digest_is_blake3_of_written_bytes() {
    let (_dir, store, account) = setup();
    let body = b"digest-me".to_vec();

    let meta = store
        .put_vault(
            account,
            ExpectedVersion::Any,
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();

    assert_eq!(meta.digest, blake3::hash(&body).to_hex().to_string());
}

#[tokio::test]
async fn aborted_upload_leaves_no_vault_and_no_tmp() {
    let (dir, store, account) = setup();
    let reader = FailingReader { remaining: 1000 };

    let result = store
        .put_vault(account, ExpectedVersion::Any, reader, 5_000)
        .await;

    assert!(result.is_err(), "оборванная загрузка должна быть ошибкой");
    assert_eq!(store.vault_meta(account).unwrap(), None);

    let files: Vec<String> = std::fs::read_dir(paths::account_dir(dir.path(), account))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("vault.bin"))
        .collect();
    assert!(files.is_empty(), "остались файлы: {files:?}");
}

#[tokio::test]
async fn open_vault_returns_written_bytes() {
    let (_dir, store, account) = setup();
    let body = b"readable".to_vec();
    store
        .put_vault(
            account,
            ExpectedVersion::Any,
            body.as_slice(),
            body.len() as u64,
        )
        .await
        .unwrap();

    let (meta, mut file) = store.open_vault(account).await.unwrap().unwrap();
    let mut read = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut file, &mut read)
        .await
        .unwrap();
    assert_eq!(read, body);
    assert_eq!(meta.version, 1);
}

/// Первые 1000 байт, потом ошибка: имитирует оборванное соединение.
struct FailingReader {
    remaining: usize,
}

impl AsyncRead for FailingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.remaining == 0 {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection reset",
            )));
        }
        let take = self.remaining.min(buf.remaining()).min(1000);
        buf.put_slice(&vec![7u8; take]);
        self.remaining -= take;
        Poll::Ready(Ok(()))
    }
}
