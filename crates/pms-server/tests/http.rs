mod support;

use pms_proto::identity::{
    derive_identity, sign_register, AccountId, DeviceId, Identity, IdentityKdf,
};
use pms_server::config::ConfigError;
use pms_server::store::vault::ExpectedVersion;
use support::TestServer;

fn identity() -> Identity {
    derive_identity(b"master-passphrase", "alice", &IdentityKdf::FAST_FOR_TESTS)
}

fn register_request(
    server: &TestServer,
    identity: &Identity,
    device: DeviceId,
    vault: &[u8],
    signature: Option<[u8; 64]>,
) -> reqwest::RequestBuilder {
    let account = AccountId::from_public_key(&identity.public);
    let digest = *blake3::hash(vault).as_bytes();
    let signature =
        signature.unwrap_or_else(|| sign_register(identity, &account, &device, &digest));

    server
        .client
        .post(server.url("/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", account.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-login", "alice")
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode(signature))
        .header("content-type", "application/octet-stream")
        .body(vault.to_vec())
}

#[tokio::test]
async fn healthz_returns_ok_without_auth() {
    let server = TestServer::start("").await;
    let response = server
        .client
        .get(server.url("/v1/healthz"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn unknown_route_is_json_404() {
    let server = TestServer::start("").await;
    let response = server
        .client
        .get(server.url("/v1/nope"))
        .header("x-pms-protocol", "1")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn wrong_protocol_header_is_426() {
    let server = TestServer::start("").await;
    let response = server
        .client
        .get(server.url("/v1/vault/meta"))
        .header("x-pms-protocol", "2")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 426);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "unsupported_protocol");
}

#[tokio::test]
async fn oversized_header_block_is_rejected() {
    let server = TestServer::start("").await;
    let huge = "a".repeat(800 * 1024);
    let response = server
        .client
        .get(server.url("/v1/healthz"))
        .header("x-huge", huge)
        .send()
        .await;

    match response {
        Ok(response) => assert!(
            response.status().is_client_error(),
            "ожидали 4xx, получили {}",
            response.status()
        ),
        Err(_) => { /* соединение закрыто — тоже допустимо */ }
    }
}

#[tokio::test]
async fn register_with_mismatched_account_id_is_rejected() {
    let server = TestServer::start("").await;
    let identity = identity();
    let device = DeviceId::new([1u8; 16]);
    let other = AccountId::new([9u8; 16]);

    let response = server
        .client
        .post(server.url("/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", other.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-login", "alice")
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode([0u8; 64]))
        .body(Vec::new())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn register_with_bad_signature_is_rejected() {
    let server = TestServer::start("").await;
    let identity = identity();
    let device = DeviceId::new([1u8; 16]);

    let response = register_request(&server, &identity, device, b"", Some([0u8; 64]))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let account = AccountId::from_public_key(&identity.public);
    assert!(server.store.account(account).unwrap().is_none());
}

#[tokio::test]
async fn register_with_vault_sets_version_one() {
    let server = TestServer::start("").await;
    let identity = identity();
    let device = DeviceId::new([2u8; 16]);
    let vault = b"sealed-vault".to_vec();

    let response = register_request(&server, &identity, device, &vault, None)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    let meta: pms_proto::dto::VaultMeta = response.json().await.unwrap();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.size, vault.len() as u64);

    let account = AccountId::from_public_key(&identity.public);
    assert!(server.store.account(account).unwrap().is_some());
}

#[tokio::test]
async fn register_then_register_again_is_409() {
    let server = TestServer::start("").await;
    let identity = identity();
    let device = DeviceId::new([3u8; 16]);

    register_request(&server, &identity, device, b"", None)
        .send()
        .await
        .unwrap();
    let second = register_request(&server, &identity, device, b"", None)
        .send()
        .await
        .unwrap();

    assert_eq!(second.status(), 409);
}

#[tokio::test]
async fn registration_token_is_enforced_when_configured() {
    let server = TestServer::start("[registration]\ntoken = \"s3cret-token\"\n").await;
    let identity = identity();
    let device = DeviceId::new([4u8; 16]);

    let without = register_request(&server, &identity, device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(without.status(), 403);

    let with = register_request(&server, &identity, device, b"", None)
        .header("x-pms-registration-token", "s3cret-token")
        .send()
        .await
        .unwrap();
    assert_eq!(with.status(), 201);
}

#[tokio::test]
async fn example_config_is_loadable_through_support() {
    // Страховка: тестовый стенд строит тот же Config, что и прод-загрузчик.
    let server = TestServer::start("").await;
    assert_eq!(server.config.quota.default_bytes, 1_073_741_824);
    assert_eq!(
        pms_server::tls::resolve(&server.config).err(),
        Some(ConfigError::TlsRequired)
    );
    let _ = ExpectedVersion::Any;
}

#[tokio::test]
async fn registration_budget_is_enforced_per_server() {
    let server = TestServer::start("[registration]\nmax_per_minute = 1\n").await;
    let device = DeviceId::new([9u8; 16]);

    let first = register_request(&server, &identity(), device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 201);

    let second = register_request(
        &server,
        &derive_identity(
            "другой ключ".as_bytes(),
            "alice",
            &IdentityKdf::FAST_FOR_TESTS,
        ),
        device,
        b"",
        None,
    )
    .send()
    .await
    .unwrap();
    assert_eq!(second.status(), 429);
}

#[tokio::test]
async fn login_is_taken_once_and_routes_the_challenge() {
    let server = TestServer::start("").await;
    let alice = identity();
    let bob = derive_identity(b"bob phrase", "bob", &IdentityKdf::FAST_FOR_TESTS);
    let device = DeviceId::new([6u8; 16]);

    let first = register_request(&server, &alice, device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 201);

    // Тот же логин с другим (валидным) ключом занимать нельзя.
    let taken = register_request_for(&server, &bob, "alice", device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(taken.status(), 409);
    let body: serde_json::Value = taken.json().await.unwrap();
    assert_eq!(body["code"], "login_taken");

    // Другой логин — отдельный аккаунт.
    let second = register_request_for(&server, &bob, "bob", device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 201);

    let alice_challenge = server
        .client
        .post(server.url("/v1/auth/challenge"))
        .header("x-pms-protocol", "1")
        .json(&pms_proto::dto::ChallengeRequest {
            login: "alice".to_string(),
            device_id: device.to_string(),
        })
        .send()
        .await
        .unwrap();
    let challenge: pms_proto::dto::ChallengeResponse = alice_challenge.json().await.unwrap();
    assert_eq!(
        challenge.account_id,
        AccountId::from_public_key(&alice.public).to_string()
    );
}

#[tokio::test]
async fn invalid_login_is_refused() {
    let server = TestServer::start("").await;
    let identity = identity();
    let device = DeviceId::new([7u8; 16]);

    let response = register_request_for(&server, &identity, "а", device, b"", None)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

fn register_request_for(
    server: &TestServer,
    identity: &Identity,
    login: &str,
    device: DeviceId,
    vault: &[u8],
    signature: Option<[u8; 64]>,
) -> reqwest::RequestBuilder {
    let account = AccountId::from_public_key(&identity.public);
    let digest = *blake3::hash(vault).as_bytes();
    let signature =
        signature.unwrap_or_else(|| sign_register(identity, &account, &device, &digest));

    server
        .client
        .post(server.url("/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", account.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-login", login)
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode(signature))
        .header("content-type", "application/octet-stream")
        .body(vault.to_vec())
}
