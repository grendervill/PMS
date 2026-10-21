use std::io::{Read, Write};

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::error::VaultError;
use super::models::TotpConfig;

const PLAIN_PREFIX: &str = "PMSQR1:";
const ENCRYPTED_PREFIX: &str = "PMSQR1E:";
const MAX_QR_CHARS: usize = 2300;
const MAX_DECOMPRESSED: usize = 64 * 1024;
const QR_AAD: &[u8] = b"PMSQR1E";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SharedCard {
    pub name: String,
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub totp: Option<TotpConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    v: u8,
    card: SharedCard,
}

fn compress(json: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(json).map_err(|_| VaultError::QrInvalid)?;
    encoder.finish().map_err(|_| VaultError::QrInvalid)
}

fn decompress(data: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut decoder = ZlibDecoder::new(data).take((MAX_DECOMPRESSED + 1) as u64);
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|_| VaultError::QrInvalid)?;
    if output.len() > MAX_DECOMPRESSED {
        return Err(VaultError::QrTooLarge);
    }
    Ok(output)
}

fn ensure_size(value: String) -> Result<String, VaultError> {
    if value.len() > MAX_QR_CHARS {
        return Err(VaultError::QrTooLarge);
    }
    Ok(value)
}

pub fn encode(card: &SharedCard, password: Option<&str>) -> Result<String, VaultError> {
    let envelope = Envelope {
        v: 1,
        card: card.clone(),
    };
    let json = serde_json::to_vec(&envelope).map_err(|_| VaultError::QrInvalid)?;
    let compressed = compress(&json)?;

    let Some(password) = password.filter(|value| !value.is_empty()) else {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed);
        return ensure_size(format!("{PLAIN_PREFIX}{payload}"));
    };

    let mut salt = [0u8; 32];
    let mut nonce = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let params =
        argon2::Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| VaultError::QrInvalid)?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let key = Zeroizing::new([0u8; 32]);
    let mut key = key;
    argon
        .hash_password_into(password.as_bytes(), &salt, &mut *key)
        .map_err(|_| VaultError::QrInvalid)?;

    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*key));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &compressed,
                aad: QR_AAD,
            },
        )
        .map_err(|_| VaultError::QrInvalid)?;

    let mut binary = Vec::with_capacity(1 + salt.len() + nonce.len() + ciphertext.len());
    binary.push(1);
    binary.extend_from_slice(&salt);
    binary.extend_from_slice(&nonce);
    binary.extend_from_slice(&ciphertext);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(binary);
    ensure_size(format!("{ENCRYPTED_PREFIX}{payload}"))
}

pub fn decode(payload: &str, password: Option<&str>) -> Result<SharedCard, VaultError> {
    let payload = payload.trim();
    let compressed = if let Some(encoded) = payload.strip_prefix(PLAIN_PREFIX) {
        let binary = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| VaultError::QrInvalid)?;
        decompress(&binary)?
    } else if let Some(encoded) = payload.strip_prefix(ENCRYPTED_PREFIX) {
        let password = password
            .filter(|value| !value.is_empty())
            .ok_or(VaultError::QrPasswordRequired)?;
        let binary = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| VaultError::QrInvalid)?;
        if binary.len() < 1 + 32 + 24 + 16 || binary[0] != 1 {
            return Err(VaultError::QrInvalid);
        }
        let salt = &binary[1..33];
        let nonce = &binary[33..57];
        let ciphertext = &binary[57..];

        let params =
            argon2::Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| VaultError::QrInvalid)?;
        let argon =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut key = Zeroizing::new([0u8; 32]);
        argon
            .hash_password_into(password.as_bytes(), salt, &mut *key)
            .map_err(|_| VaultError::QrPasswordInvalid)?;
        let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*key));
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: QR_AAD,
                },
            )
            .map_err(|_| VaultError::QrPasswordInvalid)?;
        decompress(&plaintext)?
    } else {
        return Err(VaultError::QrInvalid);
    };

    let envelope: Envelope =
        serde_json::from_slice(&compressed).map_err(|_| VaultError::QrInvalid)?;
    if envelope.v != 1 {
        return Err(VaultError::QrInvalid);
    }
    Ok(envelope.card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::TotpAlgorithm;

    fn card() -> SharedCard {
        SharedCard {
            name: "GitHub".to_string(),
            login: "alex".to_string(),
            password: "s3cret".to_string(),
            urls: vec![
                "https://github.com".to_string(),
                "https://gist.github.com".to_string(),
            ],
            comment: "личный аккаунт".to_string(),
            totp: Some(TotpConfig {
                secret: "JBSWY3DPEHPK3PXP".to_string(),
                algorithm: TotpAlgorithm::Sha256,
                digits: 8,
                period: 60,
                issuer: Some("GitHub".to_string()),
                account: Some("alex".to_string()),
            }),
        }
    }

    #[test]
    fn plain_roundtrip_preserves_all_fields() {
        let encoded = encode(&card(), None).unwrap();
        assert!(encoded.starts_with(PLAIN_PREFIX));
        assert_eq!(decode(&encoded, None).unwrap(), card());
    }

    #[test]
    fn encrypted_roundtrip_requires_password() {
        let encoded = encode(&card(), Some("qr-pass")).unwrap();
        assert!(encoded.starts_with(ENCRYPTED_PREFIX));
        assert_eq!(decode(&encoded, None), Err(VaultError::QrPasswordRequired));
        assert_eq!(
            decode(&encoded, Some("wrong")),
            Err(VaultError::QrPasswordInvalid)
        );
        assert_eq!(decode(&encoded, Some("qr-pass")).unwrap(), card());
    }

    #[test]
    fn rejects_unknown_and_malformed_payloads() {
        assert_eq!(decode("hello", None), Err(VaultError::QrInvalid));
        assert_eq!(
            decode("PMSQR1:not-base64!", None),
            Err(VaultError::QrInvalid)
        );
    }

    #[test]
    fn rejects_oversized_output() {
        assert_eq!(
            ensure_size(format!("PMSQR1:{}", "a".repeat(MAX_QR_CHARS))),
            Err(VaultError::QrTooLarge)
        );
    }
}
