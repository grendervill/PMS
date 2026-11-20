//! Проверка сертификата: системные корни, pin по SPKI или явное «доверяю всему».

use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use super::{RemoteError, RemoteProfile, TlsPolicy};

/// Отпечаток SubjectPublicKeyInfo: 32 байта SHA-256 в hex.
pub fn parse_pin(value: &str) -> Result<[u8; 32], RemoteError> {
    let lowered = value.trim().to_ascii_lowercase();
    let bytes = hex::decode(&lowered)
        .map_err(|_| RemoteError::Tls("отпечаток должен быть hex".to_string()))?;
    if bytes.len() != 32 || lowered != value.trim() {
        return Err(RemoteError::Tls(
            "отпечаток — 64 hex-символа в нижнем регистре".to_string(),
        ));
    }
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes);
    Ok(digest)
}

/// SPKI-дайджест из DER сертификата.
pub fn spki_digest(certificate_der: &[u8]) -> Result<[u8; 32], RemoteError> {
    let (_, parsed) = x509_parser::parse_x509_certificate(certificate_der)
        .map_err(|error| RemoteError::Tls(format!("сертификат не разбирается: {error}")))?;
    let spki = parsed.tbs_certificate.subject_pki.raw;
    let mut hasher = Sha256::new();
    hasher.update(spki);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

pub fn base_url(profile: &RemoteProfile) -> Result<url::Url, RemoteError> {
    let url = url::Url::parse(profile.base_url.trim())
        .map_err(|error| RemoteError::BadUrl(error.to_string()))?;

    match url.scheme() {
        "https" => Ok(url),
        "http" if profile.tls == TlsPolicy::Insecure => Ok(url),
        "http" => Err(RemoteError::InsecureForbidden),
        other => Err(RemoteError::BadUrl(format!(
            "схема {other}: поддерживаются только https и, для локального сервера, http"
        ))),
    }
}

pub fn build_client(profile: &RemoteProfile) -> Result<reqwest::Client, RemoteError> {
    let _ = base_url(profile)?;

    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("pms/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());

    match &profile.tls {
        TlsPolicy::System => {}
        TlsPolicy::Insecure => {
            builder = builder.danger_accept_invalid_certs(true);
        }
        TlsPolicy::Pin { spki_sha256 } => {
            let expected = parse_pin(spki_sha256)?;
            let config = pinned_config(PinVerifier {
                expected,
                captured: Arc::new(Mutex::new(None)),
            })?;
            builder = builder.use_preconfigured_tls(config);
        }
    }

    builder
        .build()
        .map_err(|error| RemoteError::Tls(error.to_string()))
}

/// Забирает отпечаток SPKI у сервера, которому мы пока не доверяем.
///
/// Нужен ровно один раз на профиль: интерфейс показывает отпечаток человеку и
/// сохраняет его как pin (доверие по первому подключению).
pub async fn fetch_spki_sha256(base_url: &str) -> Result<String, RemoteError> {
    let url =
        url::Url::parse(base_url.trim()).map_err(|error| RemoteError::BadUrl(error.to_string()))?;
    if url.scheme() != "https" {
        return Err(RemoteError::Tls(
            "по http сертификата нет — отпечаток не нужен".to_string(),
        ));
    }

    let captured = Arc::new(Mutex::new(None));
    let config = pinned_config(CaptureVerifier {
        captured: Arc::clone(&captured),
    })?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .use_preconfigured_tls(config)
        .build()
        .map_err(|error| RemoteError::Tls(error.to_string()))?;

    let health = url
        .join("/v1/healthz")
        .map_err(|error| RemoteError::BadUrl(error.to_string()))?;
    let response = client.get(health).send().await.map_err(map_request_error)?;
    if !response.status().is_success() {
        return Err(RemoteError::Server {
            status: response.status().as_u16(),
            message: "проверка живости не удалась".to_string(),
        });
    }

    let fingerprint = {
        let captured = captured.lock().expect("capture mutex poisoned");
        *captured
    };
    fingerprint
        .map(hex::encode)
        .ok_or_else(|| RemoteError::Tls("сервер не предъявил сертификат".to_string()))
}

fn map_request_error(error: reqwest::Error) -> RemoteError {
    if error.is_connect() || error.is_timeout() || error.is_request() {
        RemoteError::Unreachable(error.to_string())
    } else if error.is_builder() {
        RemoteError::BadUrl(error.to_string())
    } else {
        RemoteError::Unreachable(error.to_string())
    }
}

fn pinned_config(
    verifier: impl rustls::client::danger::ServerCertVerifier + 'static,
) -> Result<rustls::ClientConfig, RemoteError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|error| RemoteError::Tls(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(config)
}

#[derive(Debug)]
struct PinVerifier {
    expected: [u8; 32],
    captured: Arc<Mutex<Option<[u8; 32]>>>,
}

impl rustls::client::danger::ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let digest = spki_digest(end_entity.as_ref())
            .map_err(|error| rustls::Error::General(error.to_string()))?;
        *self.captured.lock().expect("capture mutex poisoned") = Some(digest);

        use subtle::ConstantTimeEq;
        if digest.ct_eq(&self.expected).into() {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "отпечаток ключа не совпал с сохранённым".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Верификатор, который ничего не проверяет, но запоминает отпечаток.
#[derive(Debug)]
struct CaptureVerifier {
    captured: Arc<Mutex<Option<[u8; 32]>>>,
}

impl rustls::client::danger::ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let digest = spki_digest(end_entity.as_ref())
            .map_err(|error| rustls::Error::General(error.to_string()))?;
        *self.captured.lock().expect("capture mutex poisoned") = Some(digest);
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(url: &str, tls: TlsPolicy) -> RemoteProfile {
        RemoteProfile {
            base_url: url.to_string(),
            tls,
            login: "alice".to_string(),
        }
    }

    #[test]
    fn http_is_refused_unless_insecure() {
        let error = base_url(&profile("http://192.168.1.10:8443", TlsPolicy::System)).unwrap_err();
        assert_eq!(error, RemoteError::InsecureForbidden);
        assert_eq!(error.code(), "remote_insecure_forbidden");

        assert!(base_url(&profile("http://127.0.0.1:8443", TlsPolicy::Insecure)).is_ok());
    }

    #[test]
    fn https_with_insecure_policy_is_allowed() {
        assert!(build_client(&profile("https://pms.home:8443", TlsPolicy::Insecure)).is_ok());
    }

    #[test]
    fn pin_accepts_lowercase_hex_only() {
        let good = "ab".repeat(32);
        assert!(parse_pin(&good).is_ok());
        assert!(
            parse_pin(&good.to_uppercase()).is_err(),
            "верхний регистр не принимаем"
        );
        assert!(parse_pin("ab").is_err());
        assert!(parse_pin(&"zz".repeat(32)).is_err());
    }

    #[test]
    fn nonsense_scheme_is_a_bad_url() {
        assert!(matches!(
            base_url(&profile("ftp://pms.home", TlsPolicy::Insecure)),
            Err(RemoteError::BadUrl(_))
        ));
    }
}
