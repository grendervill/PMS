use std::sync::Arc;

use pms_server::config::Config;
use pms_server::run;
use pms_server::tls::TlsMode;

fn write_self_signed(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cert_path = dir.join("server.crt");
    let key_path = dir.join("server.key");
    std::fs::write(&cert_path, certified.cert.pem()).unwrap();
    std::fs::write(&key_path, certified.key_pair.serialize_pem()).unwrap();
    (cert_path, key_path)
}

fn config_text(dir: &std::path::Path, extra: &str) -> String {
    format!(
        "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n{extra}",
        dir.join("data").display()
    )
}

#[tokio::test]
async fn server_refuses_to_start_without_cert_when_required() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse(&config_text(dir.path(), "")).unwrap();

    let result = run(config).await;
    assert!(result.is_err(), "сервер не должен стартовать без TLS");
    assert!(format!("{}", result.err().unwrap()).contains("сертификат"));
}

#[tokio::test]
async fn off_mode_serves_healthz_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse(&config_text(
        dir.path(),
        "[tls]\nrequired = false\nmode = \"off\"\n",
    ))
    .unwrap();
    let mode = pms_server::tls::resolve(&config).unwrap();
    assert_eq!(mode, TlsMode::Off);

    let server = run(config).await.unwrap();
    let response = reqwest::get(format!("http://{}/v1/healthz", server.addr()))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    server.shutdown();
}

#[tokio::test]
async fn tls_mode_serves_healthz_with_trusted_cert() {
    let dir = tempfile::tempdir().unwrap();
    let (cert_path, key_path) = write_self_signed(dir.path());
    let config = Config::parse(&config_text(
        dir.path(),
        &format!(
            "[tls]\nrequired = true\nmode = \"files\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n",
            cert_path.display(),
            key_path.display()
        ),
    ))
    .unwrap();

    let server = run(config).await.unwrap();
    let client = reqwest::Client::builder()
        .add_root_certificate(
            reqwest::Certificate::from_pem(&std::fs::read(&cert_path).unwrap()).unwrap(),
        )
        .build()
        .unwrap();

    let response = client
        .get(format!("https://{}/v1/healthz", server.addr()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    server.shutdown();
}

#[tokio::test]
async fn tls_mode_rejects_untrusted_client() {
    let dir = tempfile::tempdir().unwrap();
    let (cert_path, key_path) = write_self_signed(dir.path());
    let config = Config::parse(&config_text(
        dir.path(),
        &format!(
            "[tls]\nrequired = true\nmode = \"files\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n",
            cert_path.display(),
            key_path.display()
        ),
    ))
    .unwrap();

    let server = run(config).await.unwrap();
    let client = reqwest::Client::builder().build().unwrap();
    let result = client
        .get(format!("https://{}/v1/healthz", server.addr()))
        .send()
        .await;

    assert!(
        result.is_err(),
        "клиент без корневого сертификата не должен пройти"
    );
    server.shutdown();
}

#[tokio::test]
async fn running_server_uses_configured_quota() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse(&config_text(
        dir.path(),
        "[tls]\nrequired = false\nmode = \"off\"\n\n[quota]\ndefault_bytes = 2048\n",
    ))
    .unwrap();
    let server = run(config).await.unwrap();
    assert_eq!(server.quota_default(), 2048);
    let _ = Arc::new(());
    server.shutdown();
}
