#![allow(dead_code)]

use pms_lib::testing::remote_tls;
use pms_lib::testing::UnlockSecretInput;

/// Мастер-ключ в виде пароль-фразы.
pub fn secret(passphrase: &str) -> UnlockSecretInput {
    UnlockSecretInput::Passphrase {
        value: passphrase.to_string(),
    }
}
use pms_server::config::Config;

/// Настоящий `pms-server` на loopback с самоподписанным сертификатом.
pub struct TestServer {
    pub base_url: String,
    pub handle: pms_server::ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    /// Портим сейф на диске сервера, не обновляя индекс: так проверяется, что
    /// клиент сверяет BLAKE3, а не верит только `Content-Length`.
    pub fn tamper_vault(&self, account: &str, bytes: &[u8]) {
        let path = self
            ._dir
            .path()
            .join("data")
            .join(&account[..2])
            .join(account)
            .join("vault.bin");
        std::fs::write(path, bytes).expect("подмена сейфа");
    }

    pub async fn spawn(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
        let cert = dir.path().join("server.crt");
        let key = dir.path().join("server.key");
        std::fs::write(&cert, certified.cert.pem()).unwrap();
        std::fs::write(&key, certified.key_pair.serialize_pem()).unwrap();

        let text = format!(
            "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n\
             [tls]\nrequired = true\nmode = \"files\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n{extra}",
            dir.path().join("data").display(),
            cert.display(),
            key.display()
        );
        let config = Config::parse(&text).unwrap();
        let handle = pms_server::run(config).await.unwrap();
        let base_url = format!("https://{}", handle.addr());

        Self {
            base_url,
            handle,
            _dir: dir,
        }
    }

    /// Тот же отпечаток, что покажет интерфейс при первом подключении.
    pub async fn pin(&self) -> String {
        remote_tls::fetch_spki_sha256(&self.base_url)
            .await
            .expect("отпечаток сервера")
    }
}
