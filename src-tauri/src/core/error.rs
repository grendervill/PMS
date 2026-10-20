use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("bad file magic")]
    BadMagic,
    #[error("unsupported vault version")]
    UnsupportedVersion,
    #[error("truncated vault file")]
    Truncated,
    #[error("bad vault header")]
    BadHeader,
    #[error("bad KDF parameters")]
    BadParams,
    #[error("vault has no unlock slots")]
    NoSlots,
    #[error("vault has too many unlock slots")]
    TooManySlots,
    #[error("bad vault payload")]
    BadPayload,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("session not found")]
    SessionNotFound,
    #[error("cannot remove the last unlock method")]
    LastUnlockMethod,
    #[error("unlock slot not found")]
    SlotNotFound,
    #[error("empty secret")]
    EmptySecret,
    #[error("keyfile passphrase required")]
    KeyfilePassphraseRequired,
    #[error("keyfile passphrase invalid")]
    KeyfilePassphraseInvalid,
    #[error("public key cannot be used as a vault keyfile")]
    KeyfilePublicKey,
    #[error("unsupported keyfile format")]
    UnsupportedKeyfile,
    #[error("empty character set")]
    EmptyCharset,
    #[error("vault id mismatch")]
    VaultIdMismatch,
    #[error("cryptographic failure")]
    Crypto,
    #[error("invalid JSON")]
    Serde,
    #[error("storage failure: {0}")]
    Storage(String),
}

impl VaultError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadMagic => "bad_magic",
            Self::UnsupportedVersion => "unsupported_version",
            Self::Truncated => "truncated",
            Self::BadHeader => "bad_header",
            Self::BadParams => "bad_params",
            Self::NoSlots => "no_slots",
            Self::TooManySlots => "too_many_slots",
            Self::BadPayload => "bad_payload",
            Self::InvalidCredentials => "invalid_credentials",
            Self::SessionNotFound => "session_not_found",
            Self::LastUnlockMethod => "last_unlock_method",
            Self::SlotNotFound => "slot_not_found",
            Self::EmptySecret => "empty_secret",
            Self::KeyfilePassphraseRequired => "keyfile_passphrase_required",
            Self::KeyfilePassphraseInvalid => "keyfile_passphrase_invalid",
            Self::KeyfilePublicKey => "keyfile_public_key",
            Self::UnsupportedKeyfile => "unsupported_keyfile",
            Self::EmptyCharset => "empty_charset",
            Self::VaultIdMismatch => "vault_id_mismatch",
            Self::Crypto => "crypto",
            Self::Serde => "serde",
            Self::Storage(_) => "storage",
        }
    }
}
