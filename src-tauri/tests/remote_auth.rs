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
    let identity = identity_from_secret(&secret("мастер-ключ")).unwrap();

    let meta = client
        .register(&identity, device(), b"sealed vault")
        .await
        .unwrap();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.size, 12);

    let login = client.login(&identity, device()).await.unwrap();
    assert!(!login.token.expose().is_empty());

    let remote = client.vault_meta(login.token.expose()).await.unwrap();
    assert_eq!(remote.map(|meta| meta.version), Some(1));
}

#[tokio::test]
async fn second_device_logs_in_and_sees_the_same_vault() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ")).unwrap();

    client
        .register(&identity, DeviceId::new([1u8; 16]), b"sealed vault")
        .await
        .unwrap();

    let phone = client.login(&identity, DeviceId::new([2u8; 16])).await.unwrap();
    let meta = client.vault_meta(phone.token.expose()).await.unwrap();
    assert_eq!(meta.map(|meta| meta.version), Some(1));
}

#[tokio::test]
async fn registering_twice_explains_that_login_is_needed() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let identity = identity_from_secret(&secret("мастер-ключ")).unwrap();

    client
        .register(&identity, device(), b"sealed vault")
        .await
        .unwrap();
    let error = client
        .register(&identity, device(), b"sealed vault")
        .await
        .unwrap_err();

    assert_eq!(error.code(), "remote_bad_request");
    assert!(error.to_string().contains("вход"), "{error}");
}

#[tokio::test]
async fn unknown_account_cannot_log_in() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    let client = RemoteClient::new(&profile(&server, &pin)).unwrap();
    let stranger = identity_from_secret(&secret("другая пароль-фраза")).unwrap();

    let error = client.login(&stranger, device()).await.unwrap_err();
    assert_eq!(error.code(), "remote_not_found", "{error}");
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
    let identity = identity_from_secret(&secret("мастер-ключ")).unwrap();

    client.register(&identity, device(), b"").await.unwrap();
    let login = client.login(&identity, device()).await.unwrap();

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
    })
    .unwrap();

    let error = client.probe().await.unwrap_err();
    assert_eq!(error.code(), "remote_tls", "{error}");
}

#[allow(dead_code)]
fn unused(_: RemoteError) {}
