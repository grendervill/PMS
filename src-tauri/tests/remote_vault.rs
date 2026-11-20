mod support;

use pms_lib::testing::remote::client::RemoteClient;
use pms_lib::testing::remote::identity::identity_from_secret;
use pms_lib::testing::remote::vault::destination;
use pms_lib::testing::remote::{RemoteProfile, TlsPolicy};
use pms_proto::identity::DeviceId;
use support::{secret, TestServer};

struct Fixture {
    server: TestServer,
    client: RemoteClient,
    identity: pms_proto::identity::Identity,
    token: String,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn account_hex(&self) -> String {
        pms_proto::identity::AccountId::from_public_key(&self.identity.public).to_string()
    }
}

async fn fixture() -> Fixture {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let profile = RemoteProfile {
        base_url: server.base_url.clone(),
        tls: TlsPolicy::Pin { spki_sha256: pin },
        login: "alice".to_string(),
    };
    let client = RemoteClient::new(&profile).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();
    let device = DeviceId::new([4u8; 16]);
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
        _dir: tempfile::tempdir().unwrap(),
    }
}

#[tokio::test]
async fn push_then_pull_returns_the_same_bytes() {
    let fixture = fixture().await;
    let vault_dir = fixture._dir.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    let local = vault_dir.join("local.bin");
    let body = b"sealed vault bytes".to_vec();
    std::fs::write(&local, &body).unwrap();

    let pushed = fixture
        .client
        .push_vault(&fixture.token, None, &local)
        .await
        .unwrap();
    assert_eq!(pushed.version, 1);
    assert_eq!(pushed.size, body.len() as u64);

    let meta = fixture
        .client
        .pull_vault(&fixture.token, &vault_dir)
        .await
        .unwrap();
    assert_eq!(meta.version, 1);
    assert_eq!(std::fs::read(destination(&vault_dir)).unwrap(), body);

    let leftovers: Vec<String> = std::fs::read_dir(&vault_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".part"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "остались временные файлы: {leftovers:?}"
    );
}

#[tokio::test]
async fn stale_push_reports_the_server_version() {
    let fixture = fixture().await;
    let dir = fixture._dir.path();
    let first = dir.join("first.bin");
    std::fs::write(&first, b"one").unwrap();
    fixture
        .client
        .push_vault(&fixture.token, None, &first)
        .await
        .unwrap();

    let second = dir.join("second.bin");
    std::fs::write(&second, b"two").unwrap();
    let pushed = fixture
        .client
        .push_vault(&fixture.token, Some(1), &second)
        .await
        .unwrap();
    assert_eq!(pushed.version, 2);

    let error = fixture
        .client
        .push_vault(&fixture.token, Some(1), &second)
        .await
        .unwrap_err();
    match error {
        pms_lib::testing::RemoteError::Conflict(meta) => assert_eq!(meta.version, 2),
        other => panic!("ожидали конфликт, получили {other:?}"),
    }
}

#[tokio::test]
async fn tampered_vault_is_refused_and_local_copy_survives() {
    let fixture = fixture().await;
    let dir = fixture._dir.path().join("vault");
    std::fs::create_dir_all(&dir).unwrap();
    let local = dir.join("local.bin");
    std::fs::write(&local, b"genuine").unwrap();
    fixture
        .client
        .push_vault(&fixture.token, None, &local)
        .await
        .unwrap();

    // Подмена на диске сервера в обход индекса: Content-Length не спасёт, а
    // BLAKE3 — должен.
    fixture
        .server
        .tamper_vault(&fixture.account_hex(), b"tampered");

    let dest = fixture._dir.path().join("pulled");
    std::fs::create_dir_all(&dest).unwrap();
    let error = fixture
        .client
        .pull_vault(&fixture.token, &dest)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "remote_protocol", "{error}");
    assert!(!destination(&dest).exists());
    assert!(std::fs::read_dir(&dest).unwrap().next().is_none());
}

#[tokio::test]
async fn pull_of_missing_vault_creates_no_file() {
    let fixture = fixture().await;
    let dest = fixture._dir.path().join("empty");
    std::fs::create_dir_all(&dest).unwrap();

    let error = fixture
        .client
        .pull_vault(&fixture.token, &dest)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "remote_not_found");
    assert!(!destination(&dest).exists());
}

#[tokio::test]
async fn push_from_missing_file_is_a_storage_error() {
    let fixture = fixture().await;
    let missing = fixture._dir.path().join("nope.bin");

    let error = fixture
        .client
        .push_vault(&fixture.token, None, &missing)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "remote_storage");
}

#[tokio::test]
async fn identity_of_the_same_secret_reaches_the_same_account() {
    let fixture = fixture().await;
    let same = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();
    let other = identity_from_secret(&secret("другой ключ"), "bob").unwrap();
    assert_eq!(
        pms_proto::identity::AccountId::from_public_key(&same.public),
        pms_proto::identity::AccountId::from_public_key(&fixture.identity.public)
    );
    assert_ne!(
        pms_proto::identity::AccountId::from_public_key(&other.public),
        pms_proto::identity::AccountId::from_public_key(&fixture.identity.public)
    );
    assert!(!fixture.server.pin().await.is_empty());
}
