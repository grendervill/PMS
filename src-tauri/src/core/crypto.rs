use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use super::error::VaultError;
use super::models::{CipherSuite, KdfParams, KdfSlot, KdfSlotKind, VaultHeader, WrapData};

#[derive(Debug, Clone)]
pub enum UnlockSecretInput {
    Passphrase {
        value: String,
    },
    Keyfile {
        bytes: Vec<u8>,
        passphrase: Option<String>,
    },
    PassphraseKeyfile {
        passphrase: String,
        bytes: Vec<u8>,
        keyfile_passphrase: Option<String>,
    },
}

impl UnlockSecretInput {
    pub fn kind(&self) -> KdfSlotKind {
        match self {
            Self::Passphrase { .. } => KdfSlotKind::Passphrase,
            Self::Keyfile { .. } => KdfSlotKind::Keyfile,
            Self::PassphraseKeyfile { .. } => KdfSlotKind::PassphraseKeyfile,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KdfProfile {
    Standard,
    Strong,
    Paranoid,
}

pub fn kdf_params(profile: KdfProfile, salt: [u8; 32]) -> KdfParams {
    let (m_kib, t, p) = match profile {
        KdfProfile::Standard => (65_536, 3, 4),
        KdfProfile::Strong => (262_144, 4, 4),
        KdfProfile::Paranoid => (524_288, 6, 4),
    };

    KdfParams {
        algo: "argon2id".to_string(),
        m_kib,
        t,
        p,
        salt: b64(&salt),
    }
}

pub fn generate_salt() -> [u8; 32] {
    random_array()
}

pub fn derive_kek(secret: &[u8], params: &KdfParams) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    if !params.algo.eq_ignore_ascii_case("argon2id") {
        return Err(VaultError::BadParams);
    }
    if !(8_192..=1_048_576).contains(&params.m_kib)
        || !(1..=10).contains(&params.t)
        || !(1..=8).contains(&params.p)
    {
        return Err(VaultError::BadParams);
    }

    let salt = unb64(&params.salt)?;
    if !(16..=64).contains(&salt.len()) {
        return Err(VaultError::BadParams);
    }

    let argon_params = Params::new(params.m_kib, params.t, params.p, Some(32))
        .map_err(|_| VaultError::BadParams)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut kek = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(secret, &salt, &mut *kek)
        .map_err(|_| VaultError::Crypto)?;

    Ok(kek)
}

pub fn wrap_vault_key(
    kek: &[u8; 32],
    vault_key: &[u8; 32],
    aad: &[u8],
) -> Result<WrapData, VaultError> {
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(kek));
    let nonce_bytes = random_array::<24>();
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: vault_key,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)?;

    Ok(WrapData {
        nonce: b64(&nonce_bytes),
        ciphertext: b64(&ciphertext),
    })
}

pub fn unwrap_vault_key(
    kek: &[u8; 32],
    wrap: &WrapData,
    aad: &[u8],
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let nonce_bytes = unb64(&wrap.nonce)?;
    if nonce_bytes.len() != 24 {
        return Err(VaultError::BadParams);
    }
    let ciphertext = unb64(&wrap.ciphertext)?;
    if ciphertext.len() < 32 + 16 {
        return Err(VaultError::BadParams);
    }

    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(kek));
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce_bytes),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::InvalidCredentials)?;

    if plaintext.len() != 32 {
        return Err(VaultError::InvalidCredentials);
    }

    let mut vault_key = Zeroizing::new([0u8; 32]);
    vault_key.copy_from_slice(&plaintext);
    Ok(vault_key)
}

pub fn seal_payload(
    vault_key: &[u8; 32],
    cipher: CipherSuite,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let salt = random_array::<32>();
    let payload_key = derive_payload_key(vault_key, &salt)?;
    let nonce = random_bytes(cipher.nonce_len());

    let ciphertext = match cipher {
        CipherSuite::XChaCha20Poly1305 => {
            let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*payload_key));
            cipher
                .encrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| VaultError::Crypto)?
        }
        CipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(&*payload_key));
            cipher
                .encrypt(
                    aes_gcm::Nonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| VaultError::Crypto)?
        }
    };

    let mut envelope = Vec::with_capacity(1 + 32 + nonce.len() + ciphertext.len());
    envelope.push(1);
    envelope.extend_from_slice(&salt);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

pub fn open_payload(
    vault_key: &[u8; 32],
    cipher: CipherSuite,
    aad: &[u8],
    envelope: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let nonce_len = cipher.nonce_len();
    if envelope.len() < 1 + 32 + nonce_len + 16 {
        return Err(VaultError::BadPayload);
    }
    if envelope[0] != 1 {
        return Err(VaultError::UnsupportedVersion);
    }

    let salt = &envelope[1..33];
    let nonce = &envelope[33..33 + nonce_len];
    let ciphertext = &envelope[33 + nonce_len..];
    let payload_key = derive_payload_key(vault_key, salt)?;

    let plaintext = match cipher {
        CipherSuite::XChaCha20Poly1305 => {
            let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*payload_key));
            cipher
                .decrypt(
                    XNonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| VaultError::BadPayload)?
        }
        CipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(&*payload_key));
            cipher
                .decrypt(
                    aes_gcm::Nonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| VaultError::BadPayload)?
        }
    };

    Ok(plaintext)
}

pub fn header_core_aad(header: &VaultHeader) -> Vec<u8> {
    let mut aad = Vec::with_capacity(64 + header.created_at.len());
    aad.extend_from_slice(b"PMS-CORE-v1");
    aad.extend_from_slice(&header.format.to_le_bytes());
    aad.extend_from_slice(header.vault_id.as_bytes());
    aad.push(header.cipher.id());
    aad.extend_from_slice(&(header.created_at.len() as u32).to_le_bytes());
    aad.extend_from_slice(header.created_at.as_bytes());
    aad
}

pub fn slot_aad(header: &VaultHeader, slot: &KdfSlot) -> Result<Vec<u8>, VaultError> {
    let salt = unb64(&slot.kdf.salt)?;
    let algo = slot.kdf.algo.as_bytes();

    let mut aad = Vec::with_capacity(96 + salt.len() + algo.len());
    aad.extend_from_slice(b"PMS-SLOT-v1");
    aad.extend_from_slice(&header.format.to_le_bytes());
    aad.extend_from_slice(header.vault_id.as_bytes());
    aad.extend_from_slice(slot.id.as_bytes());
    aad.push(slot.kind.id());
    aad.extend_from_slice(&(algo.len() as u32).to_le_bytes());
    aad.extend_from_slice(algo);
    aad.extend_from_slice(&slot.kdf.m_kib.to_le_bytes());
    aad.extend_from_slice(&slot.kdf.t.to_le_bytes());
    aad.extend_from_slice(&slot.kdf.p.to_le_bytes());
    aad.extend_from_slice(&salt);
    Ok(aad)
}

pub fn canonical_secret(secret: &UnlockSecretInput) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    match secret {
        UnlockSecretInput::Passphrase { value } => {
            let normalized: String = value.nfc().collect();
            if normalized.is_empty() {
                return Err(VaultError::EmptySecret);
            }
            Ok(Zeroizing::new(normalized.into_bytes()))
        }
        UnlockSecretInput::Keyfile { bytes, passphrase } => {
            keyfile_canonical(bytes, passphrase.as_deref())
        }
        UnlockSecretInput::PassphraseKeyfile {
            passphrase,
            bytes,
            keyfile_passphrase,
        } => {
            let normalized: String = passphrase.nfc().collect();
            if normalized.is_empty() || bytes.is_empty() {
                return Err(VaultError::EmptySecret);
            }
            let keyfile = keyfile_canonical(bytes, keyfile_passphrase.as_deref())?;
            let mut hasher = Sha256::new();
            hasher.update(b"PMS-PASSFILE-v1");
            hasher.update((normalized.len() as u32).to_le_bytes());
            hasher.update(normalized.as_bytes());
            hasher.update(&*keyfile);
            Ok(Zeroizing::new(hasher.finalize().to_vec()))
        }
    }
}

fn keyfile_canonical(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    if bytes.is_empty() {
        return Err(VaultError::EmptySecret);
    }
    let (domain, material) = keyfile_material(bytes, passphrase)?;
    let material = Zeroizing::new(material);
    let mut hasher = Sha256::new();
    if domain == "file" {
        hasher.update(b"PMS-KEYFILE-FILE-v1");
    } else {
        hasher.update(b"PMS-KEYFILE-SSH-v1");
        hasher.update((domain.len() as u32).to_le_bytes());
        hasher.update(domain.as_bytes());
    }
    hasher.update(&*material);
    Ok(Zeroizing::new(hasher.finalize().to_vec()))
}

fn derive_payload_key(
    vault_key: &[u8; 32],
    salt: &[u8],
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), vault_key);
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"PMS vault payload v1", &mut *key)
        .map_err(|_| VaultError::Crypto)?;
    Ok(key)
}

fn keyfile_material(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<(String, Vec<u8>), VaultError> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if let Ok(key) = ssh_key::PrivateKey::from_openssh(text) {
            let key = decrypt_ssh_key(key, passphrase)?;
            return canonicalize_ssh_key(&key);
        }

        if text.contains("BEGIN ENCRYPTED PRIVATE KEY") {
            return Err(VaultError::UnsupportedKeyfile);
        }

        if let Some(material) = parse_pem_private_key(text)? {
            return Ok(material);
        }

        if text.contains("PRIVATE KEY") {
            return Err(VaultError::UnsupportedKeyfile);
        }

        if ssh_key::PublicKey::from_openssh(text).is_ok() {
            return Err(VaultError::KeyfilePublicKey);
        }
    }

    Ok(("file".to_string(), bytes.to_vec()))
}

fn decrypt_ssh_key(
    key: ssh_key::PrivateKey,
    passphrase: Option<&str>,
) -> Result<ssh_key::PrivateKey, VaultError> {
    if !key.is_encrypted() {
        return Ok(key);
    }

    let passphrase = passphrase.ok_or(VaultError::KeyfilePassphraseRequired)?;
    key.decrypt(passphrase)
        .map_err(|_| VaultError::KeyfilePassphraseInvalid)
}

fn rsa_from_ssh_keypair(
    keypair: &ssh_key::private::RsaKeypair,
) -> Result<rsa::RsaPrivateKey, VaultError> {
    let n = keypair
        .public
        .n
        .as_positive_bytes()
        .ok_or(VaultError::UnsupportedKeyfile)?;
    let e = keypair
        .public
        .e
        .as_positive_bytes()
        .ok_or(VaultError::UnsupportedKeyfile)?;
    let d = keypair
        .private
        .d
        .as_positive_bytes()
        .ok_or(VaultError::UnsupportedKeyfile)?;
    let p = keypair
        .private
        .p
        .as_positive_bytes()
        .ok_or(VaultError::UnsupportedKeyfile)?;
    let q = keypair
        .private
        .q
        .as_positive_bytes()
        .ok_or(VaultError::UnsupportedKeyfile)?;

    rsa::RsaPrivateKey::from_components(
        rsa::BigUint::from_bytes_be(n),
        rsa::BigUint::from_bytes_be(e),
        rsa::BigUint::from_bytes_be(d),
        vec![
            rsa::BigUint::from_bytes_be(p),
            rsa::BigUint::from_bytes_be(q),
        ],
    )
    .map_err(|_| VaultError::UnsupportedKeyfile)
}

fn canonicalize_ssh_key(
    key: &ssh_key::PrivateKey,
) -> Result<(String, Vec<u8>), VaultError> {
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use ssh_key::private::KeypairData;

    match key.key_data() {
        KeypairData::Ed25519(keypair) => {
            Ok(("ed25519".to_string(), keypair.private.to_bytes().to_vec()))
        }
        KeypairData::Rsa(keypair) => {
            let key = rsa_from_ssh_keypair(keypair)?;
            let der = key
                .to_pkcs1_der()
                .map_err(|_| VaultError::UnsupportedKeyfile)?;
            Ok(("rsa".to_string(), der.as_bytes().to_vec()))
        }
        KeypairData::Ecdsa(keypair) => {
            let curve = match keypair.algorithm() {
                ssh_key::Algorithm::Ecdsa {
                    curve: ssh_key::EcdsaCurve::NistP256,
                } => "p256",
                ssh_key::Algorithm::Ecdsa {
                    curve: ssh_key::EcdsaCurve::NistP384,
                } => "p384",
                ssh_key::Algorithm::Ecdsa {
                    curve: ssh_key::EcdsaCurve::NistP521,
                } => "p521",
                _ => return Err(VaultError::UnsupportedKeyfile),
            };
            Ok((curve.to_string(), keypair.private_key_bytes().to_vec()))
        }
        _ => Err(VaultError::UnsupportedKeyfile),
    }
}

fn parse_pem_private_key(text: &str) -> Result<Option<(String, Vec<u8>)>, VaultError> {
    use pkcs8::DecodePrivateKey;
    use rsa::pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};

    if let Ok(key) = rsa::RsaPrivateKey::from_pkcs8_pem(text) {
        let der = key
            .to_pkcs1_der()
            .map_err(|_| VaultError::UnsupportedKeyfile)?;
        return Ok(Some(("rsa".to_string(), der.as_bytes().to_vec())));
    }
    if let Ok(key) = rsa::RsaPrivateKey::from_pkcs1_pem(text) {
        let der = key
            .to_pkcs1_der()
            .map_err(|_| VaultError::UnsupportedKeyfile)?;
        return Ok(Some(("rsa".to_string(), der.as_bytes().to_vec())));
    }

    if let Ok(key) = ed25519_dalek::SigningKey::from_pkcs8_pem(text) {
        return Ok(Some(("ed25519".to_string(), key.to_bytes().to_vec())));
    }

    if let Some(bytes) = parse_nist_p256(text)? {
        return Ok(Some(("p256".to_string(), bytes)));
    }
    if let Some(bytes) = parse_nist_p384(text)? {
        return Ok(Some(("p384".to_string(), bytes)));
    }
    if let Some(bytes) = parse_nist_p521(text)? {
        return Ok(Some(("p521".to_string(), bytes)));
    }

    Ok(None)
}

fn parse_nist_p256(text: &str) -> Result<Option<Vec<u8>>, VaultError> {
    use pkcs8::DecodePrivateKey;

    if let Ok(key) = p256::SecretKey::from_pkcs8_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    if let Ok(key) = p256::SecretKey::from_sec1_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    Ok(None)
}

fn parse_nist_p384(text: &str) -> Result<Option<Vec<u8>>, VaultError> {
    use pkcs8::DecodePrivateKey;

    if let Ok(key) = p384::SecretKey::from_pkcs8_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    if let Ok(key) = p384::SecretKey::from_sec1_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    Ok(None)
}

fn parse_nist_p521(text: &str) -> Result<Option<Vec<u8>>, VaultError> {
    use pkcs8::DecodePrivateKey;

    if let Ok(key) = p521::SecretKey::from_pkcs8_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    if let Ok(key) = p521::SecretKey::from_sec1_pem(text) {
        return Ok(Some(key.to_bytes().as_slice().to_vec()));
    }
    Ok(None)
}

fn random_bytes(len: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut bytes = vec![0u8; len];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

fn random_array<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn unb64(value: &str) -> Result<Vec<u8>, VaultError> {
    STANDARD.decode(value).map_err(|_| VaultError::BadParams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{KdfSlotKind, VaultHeader};
    use uuid::Uuid;

    fn fast_params() -> KdfParams {
        KdfParams {
            algo: "argon2id".to_string(),
            m_kib: 8_192,
            t: 1,
            p: 1,
            salt: b64(&[9u8; 32]),
        }
    }

    fn sample_header() -> VaultHeader {
        VaultHeader {
            format: 1,
            vault_id: Uuid::parse_str("0e7d5f4a-5f4a-4f5a-9f4a-5f4a5f4a5f4a").unwrap(),
            created_at: "2026-10-20T12:00:00Z".to_string(),
            cipher: CipherSuite::XChaCha20Poly1305,
            kdf_slots: vec![sample_slot()],
        }
    }

    fn sample_slot() -> KdfSlot {
        KdfSlot {
            id: Uuid::parse_str("1a2b3c4d-5e6f-4a5b-8c9d-0e1f2a3b4c5d").unwrap(),
            kind: KdfSlotKind::Passphrase,
            kdf: fast_params(),
            wrap: WrapData {
                nonce: b64(&[2u8; 24]),
                ciphertext: b64(&[3u8; 48]),
            },
        }
    }

    #[test]
    fn kdf_profiles_have_expected_strength() {
        let salt = [5u8; 32];
        let standard = kdf_params(KdfProfile::Standard, salt);
        let strong = kdf_params(KdfProfile::Strong, salt);
        let paranoid = kdf_params(KdfProfile::Paranoid, salt);

        assert_eq!((standard.m_kib, standard.t, standard.p), (65_536, 3, 4));
        assert_eq!((strong.m_kib, strong.t, strong.p), (262_144, 4, 4));
        assert_eq!((paranoid.m_kib, paranoid.t, paranoid.p), (524_288, 6, 4));
    }

    #[test]
    fn empty_secrets_are_rejected() {
        assert_eq!(
            canonical_secret(&UnlockSecretInput::Passphrase {
                value: String::new(),
            }),
            Err(VaultError::EmptySecret)
        );
        assert_eq!(
            canonical_secret(&UnlockSecretInput::Keyfile {
                bytes: Vec::new(),
                passphrase: None,
            }),
            Err(VaultError::EmptySecret)
        );
    }

    #[test]
    fn argon2_is_deterministic() {
        let params = fast_params();
        let first = derive_kek(b"correct horse", &params).unwrap();
        let second = derive_kek(b"correct horse", &params).unwrap();
        let other = derive_kek(b"correct horse!", &params).unwrap();

        assert_eq!(&*first, &*second);
        assert_ne!(&*first, &*other);
    }

    #[test]
    fn passphrase_nfc_normalization() {
        let decomposed = canonical_secret(&UnlockSecretInput::Passphrase {
            value: "e\u{301}".to_string(),
        })
        .unwrap();
        let composed = canonical_secret(&UnlockSecretInput::Passphrase {
            value: "é".to_string(),
        })
        .unwrap();

        assert_eq!(&*decomposed, &*composed);
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let kek = [7u8; 32];
        let vault_key = [9u8; 32];
        let aad = b"slot-aad";
        let wrap = wrap_vault_key(&kek, &vault_key, aad).unwrap();
        let unwrapped = unwrap_vault_key(&kek, &wrap, aad).unwrap();

        assert_eq!(&*unwrapped, &vault_key);
        assert_eq!(
            unwrap_vault_key(&[8u8; 32], &wrap, aad),
            Err(VaultError::InvalidCredentials)
        );
    }

    #[test]
    fn slot_aad_binds_vault_slot_and_params() {
        let header = sample_header();
        let slot = sample_slot();
        let aad = slot_aad(&header, &slot).unwrap();
        let wrap = wrap_vault_key(&[7u8; 32], &[9u8; 32], &aad).unwrap();

        let mut other_header = header.clone();
        other_header.vault_id = Uuid::new_v4();
        let other_aad = slot_aad(&other_header, &slot).unwrap();

        assert_ne!(aad, other_aad);
        assert_eq!(
            unwrap_vault_key(&[7u8; 32], &wrap, &other_aad),
            Err(VaultError::InvalidCredentials)
        );
    }

    #[test]
    fn payload_roundtrip_xchacha() {
        let envelope = seal_payload(
            &[7u8; 32],
            CipherSuite::XChaCha20Poly1305,
            b"header",
            b"secret data",
        )
        .unwrap();
        let opened = open_payload(
            &[7u8; 32],
            CipherSuite::XChaCha20Poly1305,
            b"header",
            &envelope,
        )
        .unwrap();

        assert_eq!(opened, b"secret data");
    }

    #[test]
    fn payload_roundtrip_aes_gcm() {
        let envelope =
            seal_payload(&[7u8; 32], CipherSuite::Aes256Gcm, b"header", b"secret data").unwrap();
        let opened =
            open_payload(&[7u8; 32], CipherSuite::Aes256Gcm, b"header", &envelope).unwrap();

        assert_eq!(opened, b"secret data");
    }

    #[test]
    fn payload_rejects_tampered_tag() {
        let mut envelope = seal_payload(
            &[7u8; 32],
            CipherSuite::XChaCha20Poly1305,
            b"header",
            b"secret data",
        )
        .unwrap();
        let last = envelope.len() - 1;
        envelope[last] ^= 1;

        assert_eq!(
            open_payload(
                &[7u8; 32],
                CipherSuite::XChaCha20Poly1305,
                b"header",
                &envelope
            ),
            Err(VaultError::BadPayload)
        );
    }

    #[test]
    fn payload_rejects_changed_header_core() {
        let envelope = seal_payload(
            &[7u8; 32],
            CipherSuite::XChaCha20Poly1305,
            b"header-a",
            b"secret data",
        )
        .unwrap();

        assert_eq!(
            open_payload(
                &[7u8; 32],
                CipherSuite::XChaCha20Poly1305,
                b"header-b",
                &envelope
            ),
            Err(VaultError::BadPayload)
        );
    }

    #[test]
    fn payload_salt_changes_key_and_ciphertext() {
        let first =
            seal_payload(&[7u8; 32], CipherSuite::XChaCha20Poly1305, b"header", b"same").unwrap();
        let second =
            seal_payload(&[7u8; 32], CipherSuite::XChaCha20Poly1305, b"header", b"same").unwrap();

        assert_ne!(first, second);
        assert_ne!(first[1..33], second[1..33]);
    }

    #[test]
    fn ssh_keyfile_is_stable_across_openssh_and_pkcs8() {
        use pkcs8::EncodePrivateKey;
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let openssh = key.to_openssh(LineEnding::LF).unwrap().to_string();

        let ed = key.key_data().ed25519().unwrap();
        let signing = ed25519_dalek::SigningKey::from_bytes(&ed.private.to_bytes());
        let pkcs8 = signing.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();

        let from_openssh = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: openssh.into_bytes(),
            passphrase: None,
        })
        .unwrap();
        let from_pkcs8 = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: pkcs8.into_bytes(),
            passphrase: None,
        })
        .unwrap();

        assert_eq!(&*from_openssh, &*from_pkcs8);
    }

    #[test]
    fn ssh_keyfile_ecdsa_is_stable_across_openssh_and_pkcs8() {
        use pkcs8::EncodePrivateKey;
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(
            &mut rng,
            ssh_key::Algorithm::Ecdsa {
                curve: ssh_key::EcdsaCurve::NistP256,
            },
        )
        .unwrap();
        let openssh = key.to_openssh(LineEnding::LF).unwrap().to_string();

        let ecdsa = key.key_data().ecdsa().unwrap();
        let secret = p256::SecretKey::from_slice(ecdsa.private_key_bytes()).unwrap();
        let pkcs8 = secret.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();

        let from_openssh = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: openssh.into_bytes(),
            passphrase: None,
        })
        .unwrap();
        let from_pkcs8 = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: pkcs8.into_bytes(),
            passphrase: None,
        })
        .unwrap();

        assert_eq!(&*from_openssh, &*from_pkcs8);
    }

    #[test]
    #[ignore = "RSA key generation takes ~40s in debug; covered by the SSH key task"]
    fn ssh_keyfile_rsa_is_stable_across_openssh_and_pkcs8() {
        use pkcs8::EncodePrivateKey;
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(
            &mut rng,
            ssh_key::Algorithm::Rsa { hash: None },
        )
        .unwrap();
        let openssh = key.to_openssh(LineEnding::LF).unwrap().to_string();

        let rsa = rsa_from_ssh_keypair(key.key_data().rsa().unwrap()).unwrap();
        let pkcs8 = rsa.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();

        let from_openssh = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: openssh.into_bytes(),
            passphrase: None,
        })
        .unwrap();
        let from_pkcs8 = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: pkcs8.into_bytes(),
            passphrase: None,
        })
        .unwrap();

        assert_eq!(&*from_openssh, &*from_pkcs8);
    }

    #[test]
    fn ssh_keyfile_passphrase_does_not_change_secret() {
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let plain = key.to_openssh(LineEnding::LF).unwrap().to_string();
        let encrypted = key
            .encrypt(&mut rng, "keyfile-passphrase")
            .unwrap()
            .to_openssh(LineEnding::LF)
            .unwrap()
            .to_string();

        let plain_secret = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: plain.into_bytes(),
            passphrase: None,
        })
        .unwrap();
        let encrypted_secret = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: encrypted.into_bytes(),
            passphrase: Some("keyfile-passphrase".to_string()),
        })
        .unwrap();

        assert_eq!(&*plain_secret, &*encrypted_secret);
    }

    #[test]
    fn ssh_keyfile_requires_and_validates_passphrase() {
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let encrypted = key
            .encrypt(&mut rng, "right")
            .unwrap()
            .to_openssh(LineEnding::LF)
            .unwrap()
            .to_string();

        assert_eq!(
            canonical_secret(&UnlockSecretInput::Keyfile {
                bytes: encrypted.clone().into_bytes(),
                passphrase: None,
            }),
            Err(VaultError::KeyfilePassphraseRequired)
        );
        assert_eq!(
            canonical_secret(&UnlockSecretInput::Keyfile {
                bytes: encrypted.into_bytes(),
                passphrase: Some("wrong".to_string()),
            }),
            Err(VaultError::KeyfilePassphraseInvalid)
        );
    }

    #[test]
    fn public_ssh_key_is_rejected_as_keyfile() {
        let mut rng = rand::rngs::OsRng;
        let key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let public = key.public_key().to_openssh().unwrap();

        assert_eq!(
            canonical_secret(&UnlockSecretInput::Keyfile {
                bytes: public.into_bytes(),
                passphrase: None,
            }),
            Err(VaultError::KeyfilePublicKey)
        );
    }

    #[test]
    fn arbitrary_file_secret_uses_bytes() {
        let first = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: b"file-a".to_vec(),
            passphrase: None,
        })
        .unwrap();
        let second = canonical_secret(&UnlockSecretInput::Keyfile {
            bytes: b"file-b".to_vec(),
            passphrase: None,
        })
        .unwrap();

        assert_ne!(&*first, &*second);
    }

    #[test]
    fn kdf_params_are_capped() {
        let mut params = fast_params();
        params.m_kib = 4_000_000;

        assert_eq!(
            derive_kek(b"secret", &params),
            Err(VaultError::BadParams)
        );
    }
}
