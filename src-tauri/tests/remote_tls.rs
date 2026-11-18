mod support;

use pms_lib::testing::{RemoteProfile, TlsPolicy};
use support::TestServer;

fn profile(url: &str, tls: TlsPolicy) -> RemoteProfile {
    RemoteProfile {
        base_url: url.to_string(),
        tls,
    }
}

#[tokio::test]
async fn pinned_client_trusts_the_saved_fingerprint() {
    let server = TestServer::spawn("").await;
    let pin = server.pin().await;
    assert_eq!(pin.len(), 64);

    let client = pms_lib::testing::remote_tls::build_client(&profile(
        &server.base_url,
        TlsPolicy::Pin {
            spki_sha256: pin.clone(),
        },
    ))
    .unwrap();

    let response = client
        .get(format!("{}/v1/healthz", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn wrong_fingerprint_breaks_the_handshake() {
    let server = TestServer::spawn("").await;
    let error = pms_lib::testing::remote::client::RemoteClient::new(&profile(
        &server.base_url,
        TlsPolicy::Pin {
            spki_sha256: "11".repeat(32),
        },
    ))
    .unwrap()
    .probe()
    .await
    .unwrap_err();
    assert_eq!(error.code(), "remote_tls", "{error}");
}

#[tokio::test]
async fn system_roots_reject_a_self_signed_server() {
    let server = TestServer::spawn("").await;
    let client =
        pms_lib::testing::remote_tls::build_client(&profile(&server.base_url, TlsPolicy::System))
            .unwrap();

    let result = client
        .get(format!("{}/v1/healthz", server.base_url))
        .send()
        .await;
    assert!(
        result.is_err(),
        "самоподписанный сертификат не должен проходить"
    );
}

#[tokio::test]
async fn insecure_policy_accepts_a_self_signed_server() {
    let server = TestServer::spawn("").await;
    let client =
        pms_lib::testing::remote_tls::build_client(&profile(&server.base_url, TlsPolicy::Insecure))
            .unwrap();

    let response = client
        .get(format!("{}/v1/healthz", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn http_profile_is_refused_before_any_request() {
    let error = pms_lib::testing::remote_tls::build_client(&profile(
        "http://192.168.1.10:8443",
        TlsPolicy::System,
    ))
    .unwrap_err();

    assert_eq!(error.code(), "remote_insecure_forbidden");
}
