#![allow(dead_code)]

use std::sync::Arc;

use pms_server::api::{router, AppState};
use pms_server::auth::Auth;
use pms_server::config::Config;
use pms_server::store::Store;

/// Поднимает настоящее HTTP-приложение на случайном порту loopback.
pub struct TestServer {
    pub base_url: String,
    pub client: reqwest::Client,
    pub store: Arc<Store>,
    pub config: Arc<Config>,
    _dir: tempfile::TempDir,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub async fn start(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n{extra}",
            dir.path().display()
        );
        let config = Arc::new(Config::parse(&text).unwrap());
        config.validate().unwrap();

        let store = Arc::new(Store::open(dir.path(), config.quota.default_bytes).unwrap());
        let auth = Arc::new(Auth::new(Arc::clone(&store), config.auth.clone()));
        let app = router(AppState::new(Arc::clone(&store), auth, Arc::clone(&config)));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self {
            base_url: format!("http://{address}"),
            client: reqwest::Client::new(),
            store,
            config,
            _dir: dir,
            task,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }
}

/// Аккаунт, прошедший регистрацию и вход: всё, что нужно для запросов.
pub struct SignedClient {
    pub identity: pms_proto::identity::Identity,
    pub account: pms_proto::identity::AccountId,
    pub device: pms_proto::identity::DeviceId,
    pub token: String,
}

pub async fn register_account(
    server: &TestServer,
    secret: &[u8],
    device_seed: u8,
    vault: &[u8],
) -> SignedClient {
    use pms_proto::dto::{ChallengeRequest, ChallengeResponse, VerifyRequest, VerifyResponse};
    use pms_proto::identity::{
        derive_identity, sign_challenge, sign_register, AccountId, DeviceId, IdentityKdf,
    };

    let identity = derive_identity(secret, "alice", &IdentityKdf::FAST_FOR_TESTS);
    let account = AccountId::from_public_key(&identity.public);
    let device = DeviceId::new([device_seed; 16]);

    let digest = *blake3::hash(vault).as_bytes();
    let signature = sign_register(&identity, &account, &device, &digest);
    let response = server
        .client
        .post(server.url("/v1/accounts"))
        .header("x-pms-protocol", "1")
        .header("x-pms-account-id", account.to_string())
        .header("x-pms-public-key", hex::encode(identity.public))
        .header("x-pms-login", "alice")
        .header("x-pms-device-id", device.to_string())
        .header("x-pms-signature", hex::encode(signature))
        .body(vault.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201, "регистрация не удалась");

    let response = server
        .client
        .post(server.url("/v1/auth/challenge"))
        .header("x-pms-protocol", "1")
        .json(&ChallengeRequest {
            login: "alice".to_string(),
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

    SignedClient {
        identity,
        account,
        device,
        token: verified.token,
    }
}

impl SignedClient {
    pub async fn login(server: &TestServer, secret: &[u8], device_seed: u8) -> Self {
        use pms_proto::dto::{ChallengeRequest, ChallengeResponse, VerifyRequest, VerifyResponse};
        use pms_proto::identity::{
            derive_identity, sign_challenge, AccountId, DeviceId, IdentityKdf,
        };

        let identity = derive_identity(secret, "alice", &IdentityKdf::FAST_FOR_TESTS);
        let account = AccountId::from_public_key(&identity.public);
        let device = DeviceId::new([device_seed; 16]);

        let response = server
            .client
            .post(server.url("/v1/auth/challenge"))
            .header("x-pms-protocol", "1")
            .json(&ChallengeRequest {
                login: "alice".to_string(),
                device_id: device.to_string(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "challenge не выдан");
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

        Self {
            identity,
            account,
            device,
            token: verified.token,
        }
    }

    pub async fn put_vault(
        &self,
        server: &TestServer,
        if_match: &str,
        bytes: &[u8],
    ) -> reqwest::Response {
        server
            .client
            .put(server.url("/v1/vault"))
            .header("x-pms-protocol", "1")
            .bearer_auth(&self.token)
            .header("if-match", if_match.to_string())
            .body(bytes.to_vec())
            .send()
            .await
            .unwrap()
    }

    pub async fn get_vault(&self, server: &TestServer) -> reqwest::Response {
        server
            .client
            .get(server.url("/v1/vault"))
            .header("x-pms-protocol", "1")
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
