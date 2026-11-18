mod support;

use axum::http::StatusCode;
use pms_proto::dto::{
    BlobInfo, ChallengeRequest, ChallengeResponse, Usage, VerifyRequest, VerifyResponse,
};
use pms_proto::identity::{
    derive_identity, sign_account_delete, sign_challenge, sign_register, AccountId, DeviceId,
    Identity, IdentityKdf,
};
use support::TestServer;

const BLOB_ID: &str = "0b7c1d2e-3f40-4516-8798-a9bacbdcedfe";

struct Account {
    identity: Identity,
    account: AccountId,
    device: DeviceId,
    token: String,
}

async fn registered(server: &TestServer, vault: &[u8]) -> Account {
    let identity = derive_identity(b"master-passphrase", &IdentityKdf::FAST_FOR_TESTS);
    let account = AccountId::from_public_key(&identity.public);
    let device = DeviceId::new([8u8; 16]);
    let digest = *blake3::hash(vault).as_bytes();
    let signature = sign_register(&identity, &account, &device, &digest);

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
    assert_eq!(response.status(), 201);

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
    let challenge: ChallengeResponse = response.json().await.unwrap();
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&hex::decode(&challenge.nonce).unwrap());
    let signature = sign_challenge(&identity, &account, &device, &nonce);

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
    let verified: VerifyResponse = response.json().await.unwrap();

    Account {
        identity,
        account,
        device,
        token: verified.token,
    }
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[tokio::test]
async fn blob_put_requires_digest_header() {
    let server = TestServer::start("").await;
    let account = registered(&server, b"vault").await;

    let response = server
        .client
        .put(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .body(b"bytes".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn blob_put_with_wrong_digest_is_400_and_leaves_nothing() {
    let server = TestServer::start("").await;
    let account = registered(&server, b"vault").await;

    let response = server
        .client
        .put(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .header("x-pms-digest", digest(b"other"))
        .body(b"bytes".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], "digest_mismatch");
    let usage: Usage = server
        .client
        .get(server.url("/v1/usage"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(usage.blob_count, 0);
}

#[tokio::test]
async fn head_blob_bumps_last_used_at() {
    let server = TestServer::start("").await;
    let account = registered(&server, b"vault").await;
    let bytes = b"attachment".to_vec();

    let put = server
        .client
        .put(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .header("x-pms-digest", digest(&bytes))
        .body(bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    let head = server
        .client
        .head(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);

    let list: Vec<BlobInfo> = server
        .client
        .get(server.url("/v1/blobs"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_ne!(list[0].created_at, list[0].last_used_at);

    let downloaded = server
        .client
        .get(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(downloaded.as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn usage_matches_uploaded_sizes() {
    let server = TestServer::start("").await;
    let vault = vec![1u8; 300];
    let account = registered(&server, &vault).await;
    let bytes = vec![2u8; 200];

    server
        .client
        .put(server.url(&format!("/v1/blobs/{BLOB_ID}")))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .header("x-pms-digest", digest(&bytes))
        .body(bytes.clone())
        .send()
        .await
        .unwrap();

    let usage: Usage = server
        .client
        .get(server.url("/v1/usage"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(usage.used, 500);
    assert_eq!(usage.vault_bytes, 300);
    assert_eq!(usage.blobs_bytes, 200);
    assert_eq!(usage.quota, 1_073_741_824);
}

#[tokio::test]
async fn delete_account_removes_files_and_sessions() {
    let server = TestServer::start("").await;
    let account = registered(&server, b"vault").await;
    let signature = sign_account_delete(&account.identity, &account.account, &account.device);

    let deleted = server
        .client
        .delete(server.url("/v1/account"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .header("x-pms-signature", hex::encode(signature))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);

    let after = server
        .client
        .get(server.url("/v1/vault/meta"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
    assert!(server.store.account(account.account).unwrap().is_none());
}

#[tokio::test]
async fn delete_account_without_valid_signature_is_refused() {
    let server = TestServer::start("").await;
    let account = registered(&server, b"vault").await;

    let refused = server
        .client
        .delete(server.url("/v1/account"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&account.token)
        .header("x-pms-signature", hex::encode([0u8; 64]))
        .send()
        .await
        .unwrap();

    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert!(server.store.account(account.account).unwrap().is_some());
}
