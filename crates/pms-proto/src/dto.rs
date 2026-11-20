//! Типы провода. Поля — camelCase, как в остальном API приложения.

use serde::{Deserialize, Serialize};

/// Метаданные запечатанного сейфа. `version` растёт на единицу на каждую
/// принятую запись и служит для `If-Match`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMeta {
    pub version: u64,
    pub size: u64,
    /// BLAKE3 по байтам сейфа, hex.
    pub digest: String,
    pub updated_at: String,
}

/// Занятое и доступное место аккаунта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub used: u64,
    pub quota: u64,
    pub vault_bytes: u64,
    pub blobs_bytes: u64,
    pub blob_count: u32,
}

/// Один запечатанный блоб (вложение).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobInfo {
    pub blob_id: String,
    pub size: u64,
    pub digest: String,
    pub created_at: String,
    pub last_used_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeRequest {
    /// Логин на этом сервере: по нему сервер находит аккаунт.
    pub login: String,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeResponse {
    /// 32 байта, hex. Одноразовый.
    pub nonce: String,
    pub expires_at: String,
    /// Аккаунт, к которому относится логин: клиент сверяет его со своим.
    pub account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub account_id: String,
    pub device_id: String,
    pub nonce: String,
    /// Ed25519-подпись 64 байта, hex.
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    /// 32 байта, hex. Предъявляется как `Authorization: Bearer`.
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_meta_serializes_camel_case() {
        let meta = VaultMeta {
            version: 7,
            size: 1024,
            digest: "ab".to_string(),
            updated_at: "2026-11-18T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"version\":7"));
        assert!(json.contains("\"updatedAt\""));
    }

    #[test]
    fn usage_and_blob_info_round_trip() {
        let usage = Usage {
            used: 10,
            quota: 1_073_741_824,
            vault_bytes: 4,
            blobs_bytes: 6,
            blob_count: 1,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let back: Usage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, usage);

        let blob = BlobInfo {
            blob_id: "0b7c1d2e-3f40-4516-8798-a9bacbdcedfe".to_string(),
            size: 6,
            digest: "ab".to_string(),
            created_at: "2026-11-18T12:00:00Z".to_string(),
            last_used_at: "2026-11-18T12:05:00Z".to_string(),
        };
        let back: BlobInfo = serde_json::from_str(&serde_json::to_string(&blob).unwrap()).unwrap();
        assert_eq!(back, blob);
    }
}
