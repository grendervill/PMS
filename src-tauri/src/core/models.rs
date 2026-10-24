use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const VAULT_FORMAT_VERSION: u16 = 1;
pub const ENTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CipherSuite {
    #[serde(rename = "xchacha20-poly1305")]
    XChaCha20Poly1305,
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm,
}

impl CipherSuite {
    pub fn id(self) -> u8 {
        match self {
            Self::XChaCha20Poly1305 => 1,
            Self::Aes256Gcm => 2,
        }
    }

    pub fn nonce_len(self) -> usize {
        match self {
            Self::XChaCha20Poly1305 => 24,
            Self::Aes256Gcm => 12,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum KdfSlotKind {
    Passphrase,
    Keyfile,
    #[serde(rename = "passphrase-keyfile")]
    PassphraseKeyfile,
}

impl KdfSlotKind {
    pub fn id(self) -> u8 {
        match self {
            Self::Passphrase => 1,
            Self::Keyfile => 2,
            Self::PassphraseKeyfile => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub algo: String,
    #[serde(rename = "mKiB")]
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrapData {
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfSlot {
    pub id: Uuid,
    pub kind: KdfSlotKind,
    pub kdf: KdfParams,
    pub wrap: WrapData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultHeader {
    pub format: u16,
    pub vault_id: Uuid,
    pub created_at: String,
    pub cipher: CipherSuite,
    pub kdf_slots: Vec<KdfSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultSettings {
    pub auto_lock_minutes: u32,
    #[serde(default)]
    pub auto_lock: Option<String>,
    #[serde(default)]
    pub card_layout: Option<String>,
    pub clipboard_clear_seconds: u32,
    pub backup_retention_days: u32,
    pub auto_backup_days: u32,
    pub favicon_fetch: bool,
    pub max_attachment_mb: u32,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 5,
            auto_lock: None,
            card_layout: None,
            clipboard_clear_seconds: 30,
            backup_retention_days: 7,
            auto_backup_days: 1,
            favicon_fetch: true,
            max_attachment_mb: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Login,
}

fn default_entry_schema_version() -> u32 {
    ENTRY_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub value: String,
    pub changed_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TotpAlgorithm {
    #[serde(rename = "SHA1")]
    Sha1,
    #[serde(rename = "SHA256")]
    Sha256,
    #[serde(rename = "SHA512")]
    Sha512,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TotpConfig {
    pub secret: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub period: u32,
    pub issuer: Option<String>,
    pub account: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: Uuid,
    #[serde(default)]
    pub kind: EntryKind,
    #[serde(default = "default_entry_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub totp: Option<TotpConfig>,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
    #[serde(default)]
    pub ssh_key_ids: Vec<Uuid>,
    #[serde(default)]
    pub login_history: Vec<HistoryItem>,
    #[serde(default)]
    pub password_history: Vec<HistoryItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub key_type: SshKeyType,
    #[serde(default)]
    pub rsa_bits: Option<u16>,
    #[serde(default)]
    pub curve: Option<String>,
    pub public_key: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
    pub private_key: String,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SshKeyType {
    Ed25519,
    Rsa,
    Ecdsa,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Pdf,
    Text,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: Uuid,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: String,
    pub kind: AttachmentKind,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DraftKind {
    Entry,
    Ssh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub kind: DraftKind,
    pub payload: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BankCard {
    pub id: Uuid,
    #[serde(default = "default_entry_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub expiry: String,
    #[serde(default)]
    pub cvv: String,
    #[serde(default)]
    pub holder: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub postal_code: String,
    #[serde(default)]
    pub image_preset: String,
    #[serde(default)]
    pub image_attachment_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultDocument {
    pub id: Uuid,
    #[serde(default = "default_entry_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub passport_kind: Option<String>,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub series: String,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub issued_by: String,
    #[serde(default)]
    pub department_code: String,
    #[serde(default)]
    pub issued_at: String,
    #[serde(default)]
    pub registration_address: String,
    #[serde(default)]
    pub valid_until: String,
    #[serde(default)]
    pub categories: String,
    #[serde(default)]
    pub birth_date: String,
    #[serde(default)]
    pub birth_place: String,
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub settings: VaultSettings,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub ssh_keys: Vec<SshKey>,
    #[serde(default)]
    pub bank_cards: Vec<BankCard>,
    #[serde(default)]
    pub documents: Vec<VaultDocument>,
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
    #[serde(default)]
    pub drafts: Vec<Draft>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_vault_data() -> VaultData {
        let timestamp = "2026-10-24T10:00:00.000Z".to_string();
        VaultData {
            schema_version: 1,
            id: Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap(),
            name: "Личное".to_string(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            settings: VaultSettings {
                auto_lock: Some("10s".to_string()),
                card_layout: Some("advanced".to_string()),
                ..VaultSettings::default()
            },
            entries: Vec::new(),
            ssh_keys: Vec::new(),
            bank_cards: vec![BankCard {
                id: Uuid::parse_str("00000000-0000-4000-8000-000000000002").unwrap(),
                schema_version: 1,
                number: "4111111111111111".to_string(),
                expiry: "12/29".to_string(),
                cvv: "123".to_string(),
                holder: "IVAN IVANOV".to_string(),
                purpose: "Основная".to_string(),
                country: "Россия".to_string(),
                address: "Москва".to_string(),
                postal_code: "101000".to_string(),
                image_preset: "emerald".to_string(),
                image_attachment_id: None,
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
            }],
            documents: vec![VaultDocument {
                id: Uuid::parse_str("00000000-0000-4000-8000-000000000003").unwrap(),
                schema_version: 1,
                kind: "passport".to_string(),
                passport_kind: Some("main".to_string()),
                full_name: "IVAN IVANOV".to_string(),
                series: "4509".to_string(),
                number: "123456".to_string(),
                issued_by: "ОВД".to_string(),
                department_code: "770-001".to_string(),
                issued_at: "2020-01-01".to_string(),
                registration_address: "Москва".to_string(),
                valid_until: String::new(),
                categories: String::new(),
                birth_date: String::new(),
                birth_place: String::new(),
                attachment_ids: Vec::new(),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
            }],
            attachments: Vec::new(),
            drafts: Vec::new(),
        }
    }

    #[test]
    fn vault_data_preserves_cards_documents_and_settings() {
        let data = sample_vault_data();
        let serialized = serde_json::to_value(&data).unwrap();

        assert_eq!(serialized["bankCards"][0]["number"], "4111111111111111");
        assert_eq!(serialized["documents"][0]["kind"], "passport");
        assert_eq!(serialized["settings"]["autoLock"], "10s");
        assert_eq!(serialized["settings"]["cardLayout"], "advanced");

        let restored: VaultData = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored, data);
    }

    #[test]
    fn old_vault_data_defaults_new_arrays_and_settings() {
        let serialized = json!({
            "schemaVersion": 1,
            "id": "00000000-0000-4000-8000-000000000001",
            "name": "Old",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
            "settings": {
                "autoLockMinutes": 5,
                "clipboardClearSeconds": 30,
                "backupRetentionDays": 7,
                "autoBackupDays": 1,
                "faviconFetch": true,
                "maxAttachmentMb": 50
            },
            "entries": [],
            "sshKeys": [],
            "attachments": [],
            "drafts": []
        });

        let restored: VaultData = serde_json::from_value(serialized).unwrap();
        assert!(restored.bank_cards.is_empty());
        assert!(restored.documents.is_empty());
        assert_eq!(restored.settings.auto_lock, None);
        assert_eq!(restored.settings.card_layout, None);
    }
}
