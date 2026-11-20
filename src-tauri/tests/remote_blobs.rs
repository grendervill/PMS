mod support;

use pms_lib::testing::remote::client::RemoteClient;
use pms_lib::testing::remote::identity::identity_from_secret;
use pms_lib::testing::remote::{RemoteProfile, TlsPolicy};
use pms_proto::identity::DeviceId;
use support::{secret, TestServer};

const BLOB_ID: &str = "0b7c1d2e-3f40-4516-8798-a9bacbdcedfe";

struct Fixture {
    server: TestServer,
    client: RemoteClient,
    identity: pms_proto::identity::Identity,
    token: String,
    dir: tempfile::TempDir,
}

async fn fixture(extra: &str) -> Fixture {
    let server = TestServer::spawn(extra).await;
    let pin = server.pin().await;
    let profile = RemoteProfile {
        base_url: server.base_url.clone(),
        tls: TlsPolicy::Pin { spki_sha256: pin },
        login: "alice".to_string(),
    };
    let client = RemoteClient::new(&profile).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();
    let device = DeviceId::new([5u8; 16]);
    client
        .register(&identity, device, "alice", None)
        .await
        .unwrap();
    let login = client.login(&identity, device, "alice").await.unwrap();

    Fixture {
        server,
        client,
        identity,
        token: login.token.expose().to_string(),
        dir: tempfile::tempdir().unwrap(),
    }
}

#[tokio::test]
async fn blob_round_trip_and_usage() {
    let fixture = fixture("").await;
    let path = fixture.dir.path().join("attachment.pmsa");
    let body = b"sealed attachment".to_vec();
    std::fs::write(&path, &body).unwrap();

    let info = fixture
        .client
        .put_blob(&fixture.token, BLOB_ID, &path)
        .await
        .unwrap();
    assert_eq!(info.size, body.len() as u64);
    assert_eq!(info.digest, blake3::hash(&body).to_hex().as_str());

    let listed = fixture.client.list_blobs(&fixture.token).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].blob_id, BLOB_ID);

    let usage = fixture.client.usage(&fixture.token).await.unwrap();
    assert_eq!(usage.blobs_bytes, body.len() as u64);
    assert_eq!(usage.used, body.len() as u64);
    assert_eq!(usage.quota, 1_073_741_824);

    let restored = fixture.dir.path().join("restored.pmsa");
    fixture
        .client
        .get_blob(&fixture.token, BLOB_ID, &restored)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&restored).unwrap(), body);

    assert!(fixture
        .client
        .delete_blob(&fixture.token, BLOB_ID)
        .await
        .unwrap());
    assert!(!fixture
        .client
        .delete_blob(&fixture.token, BLOB_ID)
        .await
        .unwrap());
    assert_eq!(fixture.client.usage(&fixture.token).await.unwrap().used, 0);
}

#[tokio::test]
async fn uploading_the_same_blob_twice_is_free() {
    let fixture = fixture("").await;
    let path = fixture.dir.path().join("attachment.pmsa");
    std::fs::write(&path, b"same bytes").unwrap();

    let first = fixture
        .client
        .put_blob(&fixture.token, BLOB_ID, &path)
        .await
        .unwrap();
    let second = fixture
        .client
        .put_blob(&fixture.token, BLOB_ID, &path)
        .await
        .unwrap();

    assert_eq!(first.size, second.size);
    assert_eq!(
        fixture
            .client
            .usage(&fixture.token)
            .await
            .unwrap()
            .blobs_bytes,
        b"same bytes".len() as u64
    );
}

#[tokio::test]
async fn quota_is_enforced_by_the_server() {
    let fixture = fixture("[quota]\ndefault_bytes = 1024\n").await;
    let path = fixture.dir.path().join("big.pmsa");
    std::fs::write(&path, vec![7u8; 4096]).unwrap();

    let error = fixture
        .client
        .put_blob(&fixture.token, BLOB_ID, &path)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "remote_quota_exceeded", "{error}");
}

#[tokio::test]
async fn deleted_account_loses_access() {
    let fixture = fixture("").await;
    let device = DeviceId::new([5u8; 16]);

    fixture
        .client
        .delete_account(&fixture.token, &fixture.identity, device)
        .await
        .unwrap();

    let error = fixture.client.vault_meta(&fixture.token).await.unwrap_err();
    assert_eq!(error.code(), "remote_unauthorized");
    assert!(!fixture.server.pin().await.is_empty());
}

#[tokio::test]
async fn deleting_unknown_blob_reports_false() {
    let fixture = fixture("").await;
    assert!(!fixture
        .client
        .delete_blob(&fixture.token, "0b7c1d2e-3f40-4516-8798-a9bacbdcedff")
        .await
        .unwrap());
}
