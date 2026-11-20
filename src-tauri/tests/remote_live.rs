//! Проверка против живого сервера из `PMS_TEST_SERVER`.
//!
//! По умолчанию пропускается: в CI и на чужой машине сервера нет. Запуск:
//!
//! ```bash
//! PMS_TEST_SERVER=https://192.168.1.233:8443 \
//! PMS_TEST_PIN=<64 hex SPKI> \
//! PMS_TEST_SECRET='пароль-фраза для тестового аккаунта' \
//! PMS_TEST_LOGIN=live-test \
//! cargo test -p pms --test remote_live -- --ignored --nocapture
//! ```

use pms_lib::testing::remote::client::RemoteClient;
use pms_lib::testing::remote::identity::identity_from_secret;
use pms_lib::testing::remote::{RemoteProfile, TlsPolicy};
use pms_lib::testing::UnlockSecretInput;
use pms_proto::identity::DeviceId;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[tokio::test]
#[ignore = "нужен живой сервер: PMS_TEST_SERVER, PMS_TEST_PIN, PMS_TEST_SECRET"]
async fn live_server_round_trip() {
    let base_url = env("PMS_TEST_SERVER").expect("PMS_TEST_SERVER");
    let pin = env("PMS_TEST_PIN").expect("PMS_TEST_PIN");
    let phrase = env("PMS_TEST_SECRET").expect("PMS_TEST_SECRET");
    let login = env("PMS_TEST_LOGIN").unwrap_or_else(|| "live-test".to_string());

    let profile = RemoteProfile {
        base_url: base_url.clone(),
        tls: TlsPolicy::Pin { spki_sha256: pin },
        login: login.clone(),
    };
    let client = RemoteClient::new(&profile).expect("клиент");
    let probe = client.probe().await.expect("сервер жив");
    assert_eq!(probe.protocol, 1);

    let identity =
        identity_from_secret(&UnlockSecretInput::Passphrase { value: phrase }, "alice").unwrap();
    let device = DeviceId::new([0xAB; 16]);

    let vault = std::env::temp_dir().join("pms-live-test.vault");
    std::fs::write(&vault, b"live-test-vault").unwrap();
    let registration = client
        .register(&identity, device, &login, Some(&vault))
        .await
        .unwrap_or_else(|error| panic!("регистрация: {error:?}"));
    let _ = std::fs::remove_file(&vault);
    println!(
        "аккаунт {}: создан сейчас: {}",
        registration.login.account, registration.created
    );

    let session = registration.login;
    let meta = client
        .vault_meta(session.token.expose())
        .await
        .expect("метаданные")
        .expect("у аккаунта есть сейф");
    let usage = client.usage(session.token.expose()).await.expect("квота");

    println!(
        "сервер {base_url}: сейф версии {}, занято {} из {} байт",
        meta.version, usage.used, usage.quota
    );
    assert!(usage.quota > 0);
    assert!(meta.size > 0);
}
