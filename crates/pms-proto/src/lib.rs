#![forbid(unsafe_code)]
//! Общий протокол PMS: типы провода, коды ошибок и вывод identity-ключей.
//!
//! Здесь живёт только то, что должно совпадать у клиента и сервера. Криптография
//! сейфа (Argon2-обёртки ключа, AEAD payload) остаётся в приложении.

pub mod dto;
pub mod error;

pub use dto::{
    BlobInfo, ChallengeRequest, ChallengeResponse, Deleted, Usage, VaultMeta, VerifyRequest,
    VerifyResponse,
};
pub use error::{ErrorCode, WireError};
pub mod identity;

/// Версия протокола. Несовпадение с сервером — `426 unsupported_protocol`.
pub const PROTOCOL_VERSION: u32 = 1;
