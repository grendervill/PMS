use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::error::VaultError;
use super::models::AttachmentKind;

const MAGIC: &[u8; 5] = b"PMSA1";
const VERSION: u8 = 1;
const CIPHER: u8 = 1;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 5 + 1 + 1 + SALT_LEN + NONCE_LEN + 8;
pub const MAX_ATTACHMENT_BYTES: usize = 100 * 1024 * 1024;

pub fn seal_attachment(
    vault_key: &[u8; 32],
    vault_id: Uuid,
    attachment_id: Uuid,
    plaintext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(VaultError::FileTooLarge);
    }

    let salt = random_array::<SALT_LEN>();
    let nonce = random_array::<NONCE_LEN>();
    let key = derive_key(vault_key, &salt)?;
    let aad = attachment_aad(vault_id, attachment_id, plaintext.len() as u64);

    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*key));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::Crypto)?;

    let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    blob.extend_from_slice(MAGIC);
    blob.push(VERSION);
    blob.push(CIPHER);
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&(plaintext.len() as u64).to_le_bytes());
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

pub fn open_attachment(
    vault_key: &[u8; 32],
    vault_id: Uuid,
    attachment_id: Uuid,
    blob: &[u8],
) -> Result<Vec<u8>, VaultError> {
    if blob.len() < HEADER_LEN + TAG_LEN {
        return Err(VaultError::Truncated);
    }
    if &blob[..5] != MAGIC {
        return Err(VaultError::BadMagic);
    }
    let version = blob[5];
    if version != VERSION {
        return Err(VaultError::UnsupportedVersion);
    }
    let cipher_id = blob[6];
    if cipher_id != CIPHER {
        return Err(VaultError::BadHeader);
    }

    let salt = &blob[7..7 + SALT_LEN];
    let nonce = &blob[7 + SALT_LEN..7 + SALT_LEN + NONCE_LEN];
    let size_start = 7 + SALT_LEN + NONCE_LEN;
    let original_size = u64::from_le_bytes(
        blob[size_start..size_start + 8]
            .try_into()
            .map_err(|_| VaultError::Truncated)?,
    );

    if original_size > MAX_ATTACHMENT_BYTES as u64 {
        return Err(VaultError::FileTooLarge);
    }
    let expected = HEADER_LEN
        .checked_add(original_size as usize)
        .and_then(|value| value.checked_add(TAG_LEN))
        .ok_or(VaultError::BadPayload)?;
    if blob.len() != expected {
        return Err(if blob.len() < expected {
            VaultError::Truncated
        } else {
            VaultError::BadPayload
        });
    }

    let ciphertext = &blob[HEADER_LEN..];
    let key = derive_key(vault_key, salt)?;
    let aad = attachment_aad(vault_id, attachment_id, original_size);
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&*key));
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::BadPayload)?;

    if plaintext.len() as u64 != original_size {
        return Err(VaultError::BadPayload);
    }
    Ok(plaintext)
}

pub fn attachment_kind(mime: &str) -> AttachmentKind {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") && !mime.contains("svg") {
        AttachmentKind::Image
    } else if mime == "application/pdf" {
        AttachmentKind::Pdf
    } else if mime.starts_with("text/") {
        AttachmentKind::Text
    } else {
        AttachmentKind::Other
    }
}

fn derive_key(vault_key: &[u8; 32], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), vault_key);
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"PMS attachment v1", &mut *key)
        .map_err(|_| VaultError::Crypto)?;
    Ok(key)
}

fn attachment_aad(vault_id: Uuid, attachment_id: Uuid, original_size: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(5 + 16 + 16 + 1 + 8);
    aad.extend_from_slice(b"PMS-ATTACH-v1");
    aad.extend_from_slice(vault_id.as_bytes());
    aad.extend_from_slice(attachment_id.as_bytes());
    aad.push(VERSION);
    aad.extend_from_slice(&original_size.to_le_bytes());
    aad
}

fn random_array<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid) {
        (
            Uuid::parse_str("0e7d5f4a-5f4a-4f5a-9f4a-5f4a5f4a5f4a").unwrap(),
            Uuid::parse_str("1a2b3c4d-5e6f-4a5b-8c9d-0e1f2a3b4c5d").unwrap(),
        )
    }

    #[test]
    fn attachment_roundtrip() {
        let (vault_id, attachment_id) = ids();
        let data = b"secret attachment bytes";
        let blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, data).unwrap();
        let opened = open_attachment(&[7u8; 32], vault_id, attachment_id, &blob).unwrap();

        assert_eq!(opened, data);
    }

    #[test]
    fn attachment_rejects_wrong_vault_id() {
        let (vault_id, attachment_id) = ids();
        let blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();

        assert_eq!(
            open_attachment(&[7u8; 32], Uuid::new_v4(), attachment_id, &blob),
            Err(VaultError::BadPayload)
        );
    }

    #[test]
    fn attachment_rejects_wrong_attachment_id() {
        let (vault_id, attachment_id) = ids();
        let blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();

        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, Uuid::new_v4(), &blob),
            Err(VaultError::BadPayload)
        );
    }

    #[test]
    fn attachment_rejects_tampering() {
        let (vault_id, attachment_id) = ids();
        let mut blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 1;

        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &blob),
            Err(VaultError::BadPayload)
        );
    }

    #[test]
    fn attachment_rejects_truncated_blob() {
        let (vault_id, attachment_id) = ids();
        let blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();

        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &blob[..blob.len() - 1]),
            Err(VaultError::Truncated)
        );
    }

    #[test]
    fn attachment_rejects_bad_magic_version_and_cipher() {
        let (vault_id, attachment_id) = ids();
        let blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();

        let mut bad_magic = blob.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &bad_magic),
            Err(VaultError::BadMagic)
        );

        let mut bad_version = blob.clone();
        bad_version[5] = 2;
        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &bad_version),
            Err(VaultError::UnsupportedVersion)
        );

        let mut bad_cipher = blob;
        bad_cipher[6] = 2;
        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &bad_cipher),
            Err(VaultError::BadHeader)
        );
    }

    #[test]
    fn attachment_rejects_oversize_header() {
        let (vault_id, attachment_id) = ids();
        let mut blob = seal_attachment(&[7u8; 32], vault_id, attachment_id, b"data").unwrap();
        let size_start = 7 + SALT_LEN + NONCE_LEN;
        blob[size_start..size_start + 8]
            .copy_from_slice(&((MAX_ATTACHMENT_BYTES as u64) + 1).to_le_bytes());

        assert_eq!(
            open_attachment(&[7u8; 32], vault_id, attachment_id, &blob),
            Err(VaultError::FileTooLarge)
        );
    }

    #[test]
    fn attachment_kind_mapping() {
        assert_eq!(attachment_kind("image/png"), AttachmentKind::Image);
        assert_eq!(attachment_kind("image/svg+xml"), AttachmentKind::Other);
        assert_eq!(attachment_kind("application/pdf"), AttachmentKind::Pdf);
        assert_eq!(attachment_kind("text/plain"), AttachmentKind::Text);
        assert_eq!(attachment_kind("application/zip"), AttachmentKind::Other);
    }
}
