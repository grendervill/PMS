//! Клиент сервера синхронизации.
//!
//! Сеть живёт только здесь: рендерер не видит ни токенов, ни байтов сейфа сверх
//! существующих путей. Политика TLS решается до первого запроса — профиль с
//! `http://` без явного разрешения не отправит ни байта.

pub mod blobs;
pub mod client;
pub mod identity;
pub mod temp;
pub mod tls;
pub mod vault;

use pms_proto::dto::VaultMeta;
use pms_proto::ErrorCode;

/// Как клиент проверяет сертификат сервера.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TlsPolicy {
    /// Системные корни (Let's Encrypt, коммерческий CA).
    System,
    /// Доверие одному ключу: SHA-256 по SubjectPublicKeyInfo, hex в нижнем регистре.
    Pin {
        #[serde(rename = "spkiSha256")]
        spki_sha256: String,
    },
    /// Разрешает `http://` и любые сертификаты. Только для локального сервера:
    /// интерфейс обязан показывать предупреждение.
    Insecure,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteProfile {
    pub base_url: String,
    pub tls: TlsPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    #[error("адрес сервера не разбирается: {0}")]
    BadUrl(String),
    #[error("незашифрованное соединение запрещено: включите разрешение для http в профиле")]
    InsecureForbidden,
    #[error("сертификат сервера не прошёл проверку: {0}")]
    Tls(String),
    #[error("сервер недоступен: {0}")]
    Unreachable(String),
    #[error("сервер говорит на другой версии протокола ({0})")]
    Protocol(String),
    #[error("версия сейфа устарела: на сервере {}", .0.version)]
    Conflict(Box<VaultMeta>),
    #[error("место на сервере закончилось")]
    QuotaExceeded,
    #[error("нужен вход: сессия отсутствует или истекла")]
    Unauthorized,
    #[error("на сервере этого нет")]
    NotFound,
    #[error("сервер отклонил запрос: {0}")]
    BadRequest(String),
    #[error("слишком много попыток, попробуйте позже")]
    RateLimited,
    #[error("сервер ответил ошибкой {status}: {message}")]
    Server { status: u16, message: String },
    #[error("ошибка хранилища профиля: {0}")]
    Storage(String),
}

impl RemoteError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadUrl(_) => "remote_bad_url",
            Self::InsecureForbidden => "remote_insecure_forbidden",
            Self::Tls(_) => "remote_tls",
            Self::Unreachable(_) => "remote_unreachable",
            Self::Protocol(_) => "remote_protocol",
            Self::Conflict(_) => "remote_conflict",
            Self::QuotaExceeded => "remote_quota_exceeded",
            Self::Unauthorized => "remote_unauthorized",
            Self::NotFound => "remote_not_found",
            Self::BadRequest(_) => "remote_bad_request",
            Self::RateLimited => "remote_rate_limited",
            Self::Server { .. } => "remote_server",
            Self::Storage(_) => "remote_storage",
        }
    }

    /// Переводит код ошибки сервера в наш вариант.
    pub fn from_wire(code: ErrorCode, status: u16, message: String) -> Self {
        match code {
            ErrorCode::Unauthorized => Self::Unauthorized,
            // 403 — это «регистрация закрыта» или неверный токен регистрации:
            // говорить «сессия истекла» значит гонять пользователя по кругу.
            ErrorCode::Forbidden => Self::BadRequest(message),
            ErrorCode::QuotaExceeded | ErrorCode::PayloadTooLarge => Self::QuotaExceeded,
            ErrorCode::NotFound | ErrorCode::VaultMissing => Self::NotFound,
            ErrorCode::BadRequest | ErrorCode::DigestMismatch => Self::BadRequest(message),
            ErrorCode::RateLimited => Self::RateLimited,
            ErrorCode::UnsupportedProtocol => Self::Protocol(message),
            _ => Self::Server { status, message },
        }
    }

    pub fn message(&self) -> String {
        self.to_string()
    }

    pub fn as_api_error(&self) -> crate::commands::ApiError {
        crate::commands::ApiError::new(self.code(), self.to_string())
    }
}
