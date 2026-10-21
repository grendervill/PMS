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
    pub attachments: Vec<AttachmentMeta>,
    #[serde(default)]
    pub drafts: Vec<Draft>,
}
