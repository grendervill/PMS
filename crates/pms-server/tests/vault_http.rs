mod support;

use pms_proto::dto::{
    ChallengeRequest, ChallengeResponse, VaultMeta, VerifyRequest, VerifyResponse,
};
use pms_proto::identity::{
    derive_identity, sign_challenge, sign_register, AccountId, DeviceId, Identity, IdentityKdf,
};
use support::TestServer;

const DEVICE: [u8; 16] = [7u8; 16];

struct Account {
    identity: Identity,
    account: AccountId,
    device: DeviceId,
}

impl Account {
    fn new() -> Self {
        let identity = derive_identity(b"master-passphrase", &IdentityKdf::FAST_FOR_TESTS);
        Self {
            account: AccountId::from_public_key(&identity.public),
            identity,
            device: DeviceId::new(DEVICE),
        }
    }
}

async fn register(
    server: &TestServer,
    Account {
        identity,
        account,
        device,
    }: &Account,
    vault: &[u8],
) {
    let digest = *blake3::hash(vault).as_bytes();
    let signature = sign_register(identity, account, device, &digest);
    let response = server
        .client
        .post(server.url("/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", account.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode(signature))
        .body(vault.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201, "регистрация не удалась");
}

async fn login(
    server: &TestServer,
    Account {
        identity,
        account,
        device,
    }: &Account,
) -> String {
    let response = server
        .client
        .post(server.url("/v1/auth/challenge"))
        .header("x-pms-protocol", "1")
        .json(&ChallengeRequest {
            account_id: account.to_string(),
            device_id: device.to_string(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "challenge не выдан");
    let challenge: ChallengeResponse = response.json().await.unwrap();

    let nonce = hex::decode(&challenge.nonce).unwrap();
    let mut nonce_bytes = [0u8; 32];
    nonce_bytes.copy_from_slice(&nonce);
    let signature = sign_challenge(identity, account, device, &nonce_bytes);

    let response = server
        .client
        .post(server.url("/v1/auth/verify"))
        .header("x-pms-protocol", "1")
        .json(&VerifyRequest {
            account_id: account.to_string(),
            device_id: device.to_string(),
            nonce: challenge.nonce,
            signature: hex::encode(signature),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "verify не прошёл");
    let verified: VerifyResponse = response.json().await.unwrap();
    verified.token
}

fn if_match(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}

#[tokio::test]
async fn challenge_then_verify_yields_working_token() {
    let server = TestServer::start("").await;
    let account = Account::new();
    register(&server, &account, b"vault").await;

    let token = login(&server, &account).await;
    let response = server
        .client
        .get(server.url("/v1/vault/meta"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let meta: VaultMeta = response.json().await.unwrap();
    assert_eq!(meta.version, 1);
}

#[tokio::test]
async fn challenge_for_unknown_account_is_404() {
    let server = TestServer::start("").await;
    let account = Account::new();

    let response = server
        .client
        .post(server.url("/v1/auth/challenge"))
        .header("x-pms-protocol", "1")
        .json(&ChallengeRequest {
            account_id: account.account.to_string(),
            device_id: account.device.to_string(),
        })
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn unauthenticated_vault_put_is_unauthorized() {
    let server = TestServer::start("").await;
    let response = server
        .client
        .put(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .header("if-match", "1")
        .body(b"x".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn put_vault_without_if_match_is_428() {
    let server = TestServer::start("").await;
    let account = Account::new();
    register(&server, &account, b"vault").await;
    let token = login(&server, &account).await;

    let response = server
        .client
        .put(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .body(b"next".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 428);
}

#[tokio::test]
async fn put_vault_with_stale_version_is_409_with_current_meta_headers() {
    let server = TestServer::start("").await;
    let account = Account::new();
    register(&server, &account, b"vault").await;
    let token = login(&server, &account).await;

    let response = server
        .client
        .put(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .header("if-match", if_match("0"))
        .body(b"next".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert_eq!(
        response
            .headers()
            .get("x-pms-vault-version")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
}

#[tokio::test]
async fn get_vault_streams_exact_bytes_with_etag() {
    let server = TestServer::start("").await;
    let account = Account::new();
    let vault = b"sealed-vault-bytes".to_vec();
    register(&server, &account, &vault).await;
    let token = login(&server, &account).await;

    let response = server
        .client
        .get(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .unwrap()
        .to_string();
    assert!(etag.starts_with("\"v1-"), "ETag: {etag}");
    let body = response.bytes().await.unwrap();
    assert_eq!(body.as_ref(), vault.as_slice());
}

#[tokio::test]
async fn two_clients_with_same_version_one_gets_409() {
    let server = TestServer::start("").await;
    let account = Account::new();
    register(&server, &account, b"vault").await;
    let token = login(&server, &account).await;

    let first = server
        .client
        .put(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .header("if-match", if_match("1"))
        .body(b"winner".to_vec())
        .send()
        .await
        .unwrap();
    let second = server
        .client
        .put(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .header("if-match", if_match("1"))
        .body(b"loser".to_vec())
        .send()
        .await
        .unwrap();

    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);

    let body = server
        .client
        .get(server.url("/v1/vault"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"winner");
}

#[tokio::test]
async fn meta_of_empty_account_is_404_vault_missing() {
    let server = TestServer::start("").await;
    let account = Account::new();
    register(&server, &account, b"").await;
    let token = login(&server, &account).await;

    let response = server
        .client
        .get(server.url("/v1/vault/meta"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "vault_missing");
}
