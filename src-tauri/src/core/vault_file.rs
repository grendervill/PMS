use std::collections::HashSet;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use super::error::VaultError;
use super::models::{VaultHeader, VAULT_FORMAT_VERSION};

pub const MAGIC: &[u8; 5] = b"PMSV1";
const FIXED_HEADER_LEN: usize = 5 + 4 + 8;
const MAX_HEADER_LEN: usize = 1024 * 1024;
const MAX_PAYLOAD_LEN: usize = 512 * 1024 * 1024;
const MIN_PAYLOAD_LEN: usize = 1 + 32 + 12 + 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFile {
    pub header: VaultHeader,
    pub payload: Vec<u8>,
}

pub fn parse_vault_file(bytes: &[u8]) -> Result<VaultFile, VaultError> {
    if bytes.len() < FIXED_HEADER_LEN {
        return Err(VaultError::Truncated);
    }
    if &bytes[..5] != MAGIC {
        return Err(VaultError::BadMagic);
    }

    let header_len =
        u32::from_le_bytes(bytes[5..9].try_into().map_err(|_| VaultError::Truncated)?) as usize;
    let payload_len =
        u64::from_le_bytes(bytes[9..17].try_into().map_err(|_| VaultError::Truncated)?) as usize;

    if header_len == 0 || header_len > MAX_HEADER_LEN {
        return Err(VaultError::BadHeader);
    }
    if payload_len == 0 {
        return Err(VaultError::BadPayload);
    }
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(VaultError::BadPayload);
    }

    let expected_len = FIXED_HEADER_LEN
        .checked_add(header_len)
        .and_then(|len| len.checked_add(payload_len))
        .ok_or(VaultError::BadPayload)?;

    if bytes.len() < expected_len {
        return Err(VaultError::Truncated);
    }
    if bytes.len() > expected_len {
        return Err(VaultError::BadPayload);
    }

    let header_json = &bytes[FIXED_HEADER_LEN..FIXED_HEADER_LEN + header_len];
    let header: VaultHeader =
        serde_json::from_slice(header_json).map_err(|_| VaultError::BadHeader)?;

    validate_header(&header)?;

    let payload = bytes[FIXED_HEADER_LEN + header_len..].to_vec();
    if payload.len() < MIN_PAYLOAD_LEN {
        return Err(VaultError::BadPayload);
    }

    Ok(VaultFile { header, payload })
}

pub fn serialize_vault_file(file: &VaultFile) -> Result<Vec<u8>, VaultError> {
    validate_header(&file.header)?;
    if file.payload.is_empty() || file.payload.len() > MAX_PAYLOAD_LEN {
        return Err(VaultError::BadPayload);
    }

    let header_json = serde_json::to_vec(&file.header).map_err(|_| VaultError::BadHeader)?;
    if header_json.len() > MAX_HEADER_LEN {
        return Err(VaultError::BadHeader);
    }

    let mut bytes = Vec::with_capacity(FIXED_HEADER_LEN + header_json.len() + file.payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(file.payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&header_json);
    bytes.extend_from_slice(&file.payload);
    Ok(bytes)
}

fn validate_header(header: &VaultHeader) -> Result<(), VaultError> {
    if header.format != VAULT_FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion);
    }
    if header.kdf_slots.is_empty() {
        return Err(VaultError::NoSlots);
    }
    if header.kdf_slots.len() > 8 {
        return Err(VaultError::TooManySlots);
    }

    let mut ids = HashSet::with_capacity(header.kdf_slots.len());
    let mut kinds = HashSet::with_capacity(header.kdf_slots.len());
    for slot in &header.kdf_slots {
        if !ids.insert(slot.id) {
            return Err(VaultError::BadHeader);
        }
        if !kinds.insert(slot.kind) {
            return Err(VaultError::BadHeader);
        }

        if !slot.kdf.algo.eq_ignore_ascii_case("argon2id") {
            return Err(VaultError::BadParams);
        }
        if !(8_192..=1_048_576).contains(&slot.kdf.m_kib) {
            return Err(VaultError::BadParams);
        }
        if !(1..=10).contains(&slot.kdf.t) || !(1..=8).contains(&slot.kdf.p) {
            return Err(VaultError::BadParams);
        }

        let salt = STANDARD
            .decode(&slot.kdf.salt)
            .map_err(|_| VaultError::BadParams)?;
        if !(16..=64).contains(&salt.len()) {
            return Err(VaultError::BadParams);
        }

        let nonce = STANDARD
            .decode(&slot.wrap.nonce)
            .map_err(|_| VaultError::BadParams)?;
        let expected_nonce_len = 24;
        if nonce.len() != expected_nonce_len {
            return Err(VaultError::BadParams);
        }

        let wrapped = STANDARD
            .decode(&slot.wrap.ciphertext)
            .map_err(|_| VaultError::BadParams)?;
        if wrapped.len() < 32 + 16 {
            return Err(VaultError::BadParams);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use uuid::Uuid;

    use super::{parse_vault_file, serialize_vault_file, VaultFile};
    use crate::core::error::VaultError;
    use crate::core::models::{
        CipherSuite, KdfParams, KdfSlot, KdfSlotKind, VaultHeader, WrapData,
    };

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn sample_header() -> VaultHeader {
        VaultHeader {
            format: 1,
            vault_id: Uuid::parse_str("0e7d5f4a-5f4a-4f5a-9f4a-5f4a5f4a5f4a").unwrap(),
            created_at: "2026-10-20T12:00:00Z".to_string(),
            cipher: CipherSuite::XChaCha20Poly1305,
            kdf_slots: vec![KdfSlot {
                id: Uuid::parse_str("1a2b3c4d-5e6f-4a5b-8c9d-0e1f2a3b4c5d").unwrap(),
                kind: KdfSlotKind::Passphrase,
                kdf: KdfParams {
                    algo: "argon2id".to_string(),
                    m_kib: 65_536,
                    t: 3,
                    p: 4,
                    salt: b64(&[1u8; 32]),
                },
                wrap: WrapData {
                    nonce: b64(&[2u8; 24]),
                    ciphertext: b64(&[3u8; 48]),
                },
            }],
        }
    }

    fn sample_file() -> Vec<u8> {
        serialize_vault_file(&VaultFile {
            header: sample_header(),
            payload: vec![7u8; 128],
        })
        .expect("sample vault file must serialize")
    }

    fn raw_file(header_json: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PMSV1");
        bytes.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(header_json);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn header_value() -> serde_json::Value {
        serde_json::to_value(sample_header()).unwrap()
    }

    #[test]
    fn roundtrip_header_and_payload() {
        let bytes = sample_file();
        let parsed = parse_vault_file(&bytes).expect("valid file must parse");

        assert_eq!(parsed.header, sample_header());
        assert_eq!(parsed.payload, vec![7u8; 128]);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = sample_file();
        bytes[0] = b'X';

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadMagic));
    }

    #[test]
    fn rejects_truncated_file() {
        let bytes = sample_file();
        let truncated = &bytes[..bytes.len() - 10];

        assert_eq!(parse_vault_file(truncated), Err(VaultError::Truncated));
    }

    #[test]
    fn rejects_trailing_garbage() {
        let mut bytes = sample_file();
        bytes.extend_from_slice(b"garbage");

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadPayload));
    }

    #[test]
    fn rejects_empty_payload() {
        let header_json = serde_json::to_vec(&sample_header()).unwrap();
        let bytes = raw_file(&header_json, &[]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadPayload));
    }

    #[test]
    fn rejects_absurd_kdf_params() {
        let mut value = header_value();
        value["kdfSlots"][0]["kdf"]["mKiB"] = serde_json::json!(4_000_000);
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadParams));
    }

    #[test]
    fn rejects_no_slots() {
        let mut value = header_value();
        value["kdfSlots"] = serde_json::json!([]);
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::NoSlots));
    }

    #[test]
    fn rejects_too_many_slots() {
        let mut value = header_value();
        let slot = value["kdfSlots"][0].clone();
        value["kdfSlots"] = serde_json::Value::Array(vec![slot; 9]);
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::TooManySlots));
    }

    #[test]
    fn rejects_unknown_cipher() {
        let mut value = header_value();
        value["cipher"] = serde_json::json!("rot13");
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadHeader));
    }

    #[test]
    fn rejects_duplicate_slot_kinds() {
        let mut value = header_value();
        let mut slot = value["kdfSlots"][0].clone();
        slot["id"] = serde_json::json!("2b3c4d5e-6f7a-4b5c-8d9e-0f1a2b3c4d5e");
        value["kdfSlots"] = serde_json::Value::Array(vec![value["kdfSlots"][0].clone(), slot]);
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadHeader));
    }

    #[test]
    fn rejects_duplicate_slot_ids() {
        let mut value = header_value();
        let slot = value["kdfSlots"][0].clone();
        value["kdfSlots"] = serde_json::Value::Array(vec![slot.clone(), slot]);
        let header_json = serde_json::to_vec(&value).unwrap();
        let bytes = raw_file(&header_json, &[7u8; 128]);

        assert_eq!(parse_vault_file(&bytes), Err(VaultError::BadHeader));
    }
}
