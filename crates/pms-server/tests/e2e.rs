mod support;

use pms_proto::dto::{ChallengeRequest, VaultMeta};
use support::{register_account, SignedClient, TestServer};

const SECRET: &[u8] = b"same master passphrase";

/// Полный путь, который пройдёт приложение: два устройства с одним мастер-ключом.
#[tokio::test]
async fn two_devices_sync_same_vault() {
    let server = TestServer::start("").await;

    // Устройство A: регистрирует аккаунт с пустым сейфом и заливает первую версию.
    let laptop = register_account(&server, SECRET, 1, b"").await;
    let vault_v1 = b"sealed vault v1".to_vec();
    let response = laptop.put_vault(&server, "0", &vault_v1).await;
    assert_eq!(response.status(), 200);
    let meta: VaultMeta = response.json().await.unwrap();
    assert_eq!(meta.version, 1);

    // Вложение: отдельный блоб, сейф на него только ссылается.
    let blob = b"sealed attachment".to_vec();
    let digest = blake3::hash(&blob).to_hex().to_string();
    let response = server
        .client
        .put(server.url("/v1/blobs/0b7c1d2e-3f40-4516-8798-a9bacbdcedfe"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&laptop.token)
        .header("x-pms-digest", &digest)
        .body(blob.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Устройство B: тот же мастер-ключ, другое устройство, без регистрации.
    let phone = SignedClient::login(&server, SECRET, 2).await;
    assert_eq!(phone.account, laptop.account);
    assert_ne!(phone.device, laptop.device);

    let response = phone.get_vault(&server).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        vault_v1.as_slice()
    );

    let response = server
        .client
        .get(server.url("/v1/usage"))
        .header("x-pms-protocol", "1")
        .bearer_auth(&phone.token)
        .send()
        .await
        .unwrap();
    let usage: pms_proto::dto::Usage = response.json().await.unwrap();
    assert_eq!(usage.vault_bytes, vault_v1.len() as u64);
    assert_eq!(usage.blobs_bytes, blob.len() as u64);
    assert_eq!(usage.blob_count, 1);

    // Ноутбук сохраняет вторую версию; телефон со старой версией получает конфликт.
    let vault_v2 = b"sealed vault v2".to_vec();
    let response = laptop.put_vault(&server, "1", &vault_v2).await;
    assert_eq!(response.status(), 200);
    let meta: VaultMeta = response.json().await.unwrap();
    assert_eq!(meta.version, 2);

    let conflict = phone.put_vault(&server, "1", b"phone edit").await;
    assert_eq!(conflict.status(), 409);
    assert_eq!(
        conflict
            .headers()
            .get("x-pms-vault-version")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );

    // Телефон забирает актуальную версию и сохраняет поверх неё.
    let response = phone.get_vault(&server).await;
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        vault_v2.as_slice()
    );
    let response = phone.put_vault(&server, "2", b"phone edit").await;
    assert_eq!(response.status(), 200);
}

/// Второе устройство не может пройти вход с чужим ключом.
#[tokio::test]
async fn another_master_key_cannot_join_the_account() {
    let server = TestServer::start("").await;
    let _owner = register_account(&server, SECRET, 1, b"vault").await;

    let intruder = pms_proto::identity::derive_identity(
        b"different passphrase",
        &pms_proto::identity::IdentityKdf::FAST_FOR_TESTS,
    );
    let account = pms_proto::identity::AccountId::from_public_key(&intruder.public);
    let device = pms_proto::identity::DeviceId::new([3u8; 16]);

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
    assert_eq!(response.status(), 404, "чужого аккаунта не существует");

    let _ = intruder;
}
