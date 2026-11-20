//! Коды ошибок провода. Имена совпадают с теми, что приложение уже использует
//! в `ApiError`, поэтому рендерер не учится новому словарю.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    PreconditionFailed,
    PreconditionRequired,
    UnsupportedMediaType,
    UnsupportedProtocol,
    QuotaExceeded,
    PayloadTooLarge,
    RateLimited,
    VaultMissing,
    LoginTaken,
    DigestMismatch,
    Internal,
    Unavailable,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::PreconditionFailed => "precondition_failed",
            Self::PreconditionRequired => "precondition_required",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::QuotaExceeded => "quota_exceeded",
            Self::PayloadTooLarge => "payload_too_large",
            Self::RateLimited => "rate_limited",
            Self::VaultMissing => "vault_missing",
            Self::LoginTaken => "login_taken",
            Self::DigestMismatch => "digest_mismatch",
            Self::Internal => "internal",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub code: ErrorCode,
    pub message: String,
}

impl WireError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for WireError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_serializes_snake_case() {
        let json = serde_json::to_string(&ErrorCode::QuotaExceeded).unwrap();
        assert_eq!(json, "\"quota_exceeded\"");
        assert_eq!(
            ErrorCode::PreconditionRequired.as_str(),
            "precondition_required"
        );
    }

    #[test]
    fn wire_error_has_code_and_message_only() {
        let error = WireError::new(ErrorCode::Conflict, "версия устарела");
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(
            json,
            "{\"code\":\"conflict\",\"message\":\"версия устарела\"}"
        );
    }
}
