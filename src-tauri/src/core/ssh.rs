use serde::{Deserialize, Serialize};
use ssh_key::private::KeypairData;
use ssh_key::{Algorithm, EcdsaCurve, HashAlg, LineEnding, PrivateKey};
use zeroize::Zeroizing;

use super::error::VaultError;
use super::models::SshKeyType;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SshGenerateOptions {
    pub key_type: SshKeyType,
    pub rsa_bits: Option<u16>,
    pub curve: Option<String>,
    pub comment: String,
    pub passphrase: Option<String>,
}

impl Default for SshGenerateOptions {
    fn default() -> Self {
        Self {
            key_type: SshKeyType::Ed25519,
            rsa_bits: None,
            curve: None,
            comment: String::new(),
            passphrase: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSshKey {
    pub key_type: SshKeyType,
    pub rsa_bits: Option<u16>,
    pub curve: Option<String>,
    pub public_key: String,
    pub private_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshFormats {
    pub public_key: String,
    pub openssh: String,
    pub pem: String,
    pub pkcs8: String,
    pub fingerprint: String,
}

pub fn generate(options: &SshGenerateOptions) -> Result<GeneratedSshKey, VaultError> {
    let mut rng = rand::rngs::OsRng;

    let mut key = match options.key_type {
        SshKeyType::Ed25519 => {
            PrivateKey::random(&mut rng, Algorithm::Ed25519).map_err(|_| VaultError::Crypto)?
        }
        SshKeyType::Ecdsa => {
            let curve = match options.curve.as_deref().unwrap_or("p256") {
                "p256" => EcdsaCurve::NistP256,
                "p384" => EcdsaCurve::NistP384,
                "p521" => EcdsaCurve::NistP521,
                _ => return Err(VaultError::BadParams),
            };
            PrivateKey::random(&mut rng, Algorithm::Ecdsa { curve })
                .map_err(|_| VaultError::Crypto)?
        }
        SshKeyType::Rsa => {
            let bits = options.rsa_bits.unwrap_or(2048);
            if !(2048..=4096).contains(&bits) || !bits.is_multiple_of(256) {
                return Err(VaultError::BadParams);
            }
            let rsa_key =
                rsa::RsaPrivateKey::new(&mut rng, bits as usize).map_err(|_| VaultError::Crypto)?;
            let keypair =
                ssh_key::private::RsaKeypair::try_from(rsa_key).map_err(|_| VaultError::Crypto)?;
            PrivateKey::new(KeypairData::Rsa(keypair), options.comment.clone())
                .map_err(|_| VaultError::Crypto)?
        }
    };

    if key.comment().is_empty() && !options.comment.is_empty() {
        key.set_comment(options.comment.clone());
    }

    if let Some(passphrase) = options.passphrase.as_deref().filter(|value| !value.is_empty()) {
        key = key
            .encrypt(&mut rng, passphrase)
            .map_err(|_| VaultError::Crypto)?;
    }

    let private_key = key
        .to_openssh(LineEnding::LF)
        .map_err(|_| VaultError::Crypto)?
        .to_string();
    let public_key = key
        .public_key()
        .to_openssh()
        .map_err(|_| VaultError::Crypto)?;
    let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();

    Ok(GeneratedSshKey {
        key_type: options.key_type,
        rsa_bits: options.rsa_bits,
        curve: options.curve.clone(),
        public_key,
        private_key,
        fingerprint,
    })
}

pub fn formats(
    private_key: &str,
    passphrase: Option<&str>,
) -> Result<SshFormats, VaultError> {
    let mut key = PrivateKey::from_openssh(private_key).map_err(|_| VaultError::InvalidCredentials)?;

    if key.is_encrypted() {
        let passphrase = passphrase.ok_or(VaultError::SshPassphraseRequired)?;
        key = key
            .decrypt(passphrase)
            .map_err(|_| VaultError::SshPassphraseInvalid)?;
    }

    let openssh = key
        .to_openssh(LineEnding::LF)
        .map_err(|_| VaultError::Crypto)?
        .to_string();
    let public_key = key
        .public_key()
        .to_openssh()
        .map_err(|_| VaultError::Crypto)?;
    let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();

    let (pem, pkcs8) = match key.key_data() {
        KeypairData::Ed25519(keypair) => {
            use pkcs8::EncodePrivateKey;
            let signing = ed25519_dalek::SigningKey::from_bytes(&keypair.private.to_bytes());
            let value = Zeroizing::new(
                signing
                    .to_pkcs8_pem(LineEnding::LF)
                    .map_err(|_| VaultError::Crypto)?
                    .to_string(),
            );
            (value.to_string(), value.to_string())
        }
        KeypairData::Rsa(keypair) => {
            use pkcs8::EncodePrivateKey;
            use rsa::pkcs1::EncodeRsaPrivateKey;
            let rsa = rsa_from_ssh_keypair(keypair)?;
            let pem = rsa
                .to_pkcs1_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            let pkcs8 = rsa
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            (pem, pkcs8)
        }
        KeypairData::Ecdsa(keypair) => ecdsa_pem(keypair)?,
        _ => return Err(VaultError::UnsupportedKeyfile),
    };

    Ok(SshFormats {
        public_key,
        openssh,
        pem,
        pkcs8,
        fingerprint,
    })
}

fn ecdsa_pem(keypair: &ssh_key::private::EcdsaKeypair) -> Result<(String, String), VaultError> {
    use pkcs8::EncodePrivateKey;

    match keypair.algorithm() {
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        } => {
            let key = p256::SecretKey::from_slice(keypair.private_key_bytes())
                .map_err(|_| VaultError::Crypto)?;
            let sec1 = key
                .to_sec1_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            let pkcs8 = key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            Ok((sec1, pkcs8))
        }
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        } => {
            let key = p384::SecretKey::from_slice(keypair.private_key_bytes())
                .map_err(|_| VaultError::Crypto)?;
            let sec1 = key
                .to_sec1_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            let pkcs8 = key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            Ok((sec1, pkcs8))
        }
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        } => {
            let key = p521::SecretKey::from_slice(keypair.private_key_bytes())
                .map_err(|_| VaultError::Crypto)?;
            let sec1 = key
                .to_sec1_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            let pkcs8 = key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|_| VaultError::Crypto)?
                .to_string();
            Ok((sec1, pkcs8))
        }
        _ => Err(VaultError::UnsupportedKeyfile),
    }
}

fn rsa_from_ssh_keypair(
    keypair: &ssh_key::private::RsaKeypair,
) -> Result<rsa::RsaPrivateKey, VaultError> {
    let n = keypair
        .public
        .n
        .as_positive_bytes()
        .ok_or(VaultError::Crypto)?;
    let e = keypair
        .public
        .e
        .as_positive_bytes()
        .ok_or(VaultError::Crypto)?;
    let d = keypair
        .private
        .d
        .as_positive_bytes()
        .ok_or(VaultError::Crypto)?;
    let p = keypair
        .private
        .p
        .as_positive_bytes()
        .ok_or(VaultError::Crypto)?;
    let q = keypair
        .private
        .q
        .as_positive_bytes()
        .ok_or(VaultError::Crypto)?;

    rsa::RsaPrivateKey::from_components(
        rsa::BigUint::from_bytes_be(n),
        rsa::BigUint::from_bytes_be(e),
        rsa::BigUint::from_bytes_be(d),
        vec![
            rsa::BigUint::from_bytes_be(p),
            rsa::BigUint::from_bytes_be(q),
        ],
    )
    .map_err(|_| VaultError::Crypto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_ed25519_and_formats() {
        let generated = generate(&SshGenerateOptions::default()).unwrap();
        assert!(generated.private_key.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(generated.public_key.starts_with("ssh-ed25519 "));
        assert!(generated.fingerprint.starts_with("SHA256:"));

        let formats = formats(&generated.private_key, None).unwrap();
        assert!(formats.openssh.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(formats.pkcs8.contains("BEGIN PRIVATE KEY"));
        assert!(formats.pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(formats.public_key, generated.public_key);
    }

    #[test]
    fn generates_ecdsa_p256() {
        let generated = generate(&SshGenerateOptions {
            key_type: SshKeyType::Ecdsa,
            curve: Some("p256".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert!(generated.public_key.starts_with("ecdsa-sha2-nistp256 "));
        let formats = formats(&generated.private_key, None).unwrap();
        assert!(formats.pem.contains("BEGIN EC PRIVATE KEY"));
        assert!(formats.pkcs8.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn encrypted_key_requires_passphrase() {
        let generated = generate(&SshGenerateOptions {
            passphrase: Some("ssh-pass".to_string()),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(
            formats(&generated.private_key, None),
            Err(VaultError::SshPassphraseRequired)
        );
        assert_eq!(
            formats(&generated.private_key, Some("wrong")),
            Err(VaultError::SshPassphraseInvalid)
        );
        assert!(formats(&generated.private_key, Some("ssh-pass")).is_ok());
    }

    #[test]
    #[ignore = "RSA generation is slow in debug builds"]
    fn generates_rsa_2048() {
        let generated = generate(&SshGenerateOptions {
            key_type: SshKeyType::Rsa,
            rsa_bits: Some(2048),
            ..Default::default()
        })
        .unwrap();
        assert!(generated.public_key.starts_with("ssh-rsa "));
        let formats = formats(&generated.private_key, None).unwrap();
        assert!(formats.pem.contains("BEGIN RSA PRIVATE KEY"));
        assert!(formats.pkcs8.contains("BEGIN PRIVATE KEY"));
    }
}
