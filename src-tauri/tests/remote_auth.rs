mod support;

use pms_lib::testing::remote::client::RemoteClient;
use pms_lib::testing::remote::identity::identity_from_secret;
use pms_lib::testing::remote::{RemoteError, RemoteProfile, TlsPolicy};
use pms_proto::identity::DeviceId;

use support::{secret, TestServer};

fn profile(server: &TestServer, pin: &str) -> RemoteProfile {
    RemoteProfile {
        base_url: server.base_url.clone(),
        tls: TlsPolicy::Pin {
            spki_sha256: pin.to_string(),
        },
        login: "alice".to_string(),
    }
}

fn device() -> DeviceId {
    DeviceId::new([3u8; 16])
}

#[tokio::test]
async fn probe_reports_the_same_protocol_version() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();

    let probe = client.probe().await.unwrap();
    assert_eq!(probe.protocol, 1);
}

#[tokio::test]
async fn register_then_login_gives_a_working_session() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("pms.vault");
    std::fs::write(&vault, b"sealed vault").unwrap();

    let registration = client
        .register(&identity, device(), "alice", Some(&vault))
        .await
        .unwrap();
    assert!(registration.created);
    let meta = registration.vault.expect("метаданные сейфа");
    assert_eq!(meta.version, 1);
    assert_eq!(meta.size, 12);

    let login = client.login(&identity, device(), "alice").await.unwrap();
    assert!(!login.token.expose().is_empty());

    let remote = client.vault_meta(login.token.expose()).await.unwrap();
    assert_eq!(remote.map(|meta| meta.version), Some(1));
}

#[tokio::test]
async fn second_device_logs_in_and_sees_the_same_vault() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("pms.vault");
    std::fs::write(&vault, b"sealed vault").unwrap();
    client
        .register(&identity, DeviceId::new([1u8; 16]), "alice", Some(&vault))
        .await
        .unwrap();

    let phone = client
        .login(&identity, DeviceId::new([2u8; 16]), "alice")
        .await
        .unwrap();
    let meta = client.vault_meta(phone.token.expose()).await.unwrap();
    assert_eq!(meta.map(|meta| meta.version), Some(1));
}

#[tokio::test]
async fn registering_twice_falls_back_to_login() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();

    let first = client
        .register(&identity, device(), "alice", None)
        .await
        .unwrap();
    assert!(first.created);
    assert!(!first.login.token.expose().is_empty());

    let second = client
        .register(&identity, device(), "alice", None)
        .await
        .unwrap();
    assert!(!second.created, "второй раз аккаунт не создаётся заново");
    assert!(!second.login.token.expose().is_empty());

    let meta = client
        .vault_meta(second.login.token.expose())
        .await
        .unwrap();
    assert!(meta.is_none(), "сейфа ещё нет");
}

#[tokio::test]
async fn unknown_account_cannot_log_in() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let stranger = identity_from_secret(&secret("другая пароль-фраза"), "alice").unwrap();

    let error = client
        .login(&stranger, device(), "stranger")
        .await
        .unwrap_err();
    // Неизвестный логин выглядит как неверный пароль: без подсказок.
    assert_eq!(error.code(), "remote_unauthorized", "{error}");
}

#[tokio::test]
async fn garbage_token_is_unauthorized() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();

    let error = client.vault_meta(&"00".repeat(32)).await.unwrap_err();
    assert_eq!(error.code(), "remote_unauthorized");
}

#[tokio::test]
async fn vault_meta_is_none_before_the_first_push() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ"), "alice").unwrap();

    client
        .register(&identity, device(), "alice", None)
        .await
        .unwrap();
    let login = client.login(&identity, device(), "alice").await.unwrap();

    assert!(client
        .vault_meta(login.token.expose())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn unreachable_server_is_reported_as_unreachable() {
    let profile = RemoteProfile {
        base_url: "https://127.0.0.1:1".to_string(),
        tls: TlsPolicy::Insecure,
        login: "alice".to_string(),
    };
    let client = RemoteClient::new(&profile).unwrap();

    let error = client.probe().await.unwrap_err();
    assert_eq!(error.code(), "remote_unreachable", "{error}");
}

#[tokio::test]
async fn tls_failure_is_reported_as_tls() {
    let server = TestServer::spawn("").await;
    let client = RemoteClient::new(&RemoteProfile {
        base_url: server.base_url.clone(),
        tls: TlsPolicy::System,
        login: "alice".to_string(),
    })
    .unwrap();

    let error = client.probe().await.unwrap_err();
    assert_eq!(error.code(), "remote_tls", "{error}");
}

#[allow(dead_code)]
fn unused(_: RemoteError) {}
