//! HTTP-слой: роутер, лимиты, единый формат ошибок.

pub mod auth;
pub mod vault;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use pms_proto::WireError;
use pms_proto::{ErrorCode, PROTOCOL_VERSION};
use tower::limit::ConcurrencyLimitLayer;

use crate::auth::{Auth, AuthError};
use crate::config::Config;
use crate::store::{Store, StoreError};

pub const HEADER_PROTOCOL: &str = "x-pms-protocol";
pub const HEADER_ACCOUNT: &str = "x-pms-account-id";
pub const HEADER_PUBLIC_KEY: &str = "x-pms-public-key";
pub const HEADER_DEVICE: &str = "x-pms-device-id";
pub const HEADER_SIGNATURE: &str = "x-pms-signature";
pub const HEADER_DIGEST: &str = "x-pms-digest";
pub const HEADER_REGISTRATION_TOKEN: &str = "x-pms-registration-token";

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub auth: Arc<Auth>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(store: Arc<Store>, auth: Arc<Auth>, config: Arc<Config>) -> Self {
        Self {
            store,
            auth,
            config,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let limits = state.config.limits.clone();

    Router::new()
        .route("/v1/healthz", get(healthz))
        .merge(auth::routes())
        .merge(vault::routes())
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(limits.max_body_bytes as usize))
        .layer(ConcurrencyLimitLayer::new(limits.max_connections))
        .layer(middleware::from_fn(check_protocol))
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "protocol": PROTOCOL_VERSION }))
}

async fn not_found() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "такого метода нет")
}

/// Требует заголовок версии протокола на всём, кроме проверки живости.
async fn check_protocol(request: Request, next: Next) -> Response {
    if request.uri().path() == "/v1/healthz" {
        return next.run(request).await;
    }
    let allowed = request
        .headers()
        .get(HEADER_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == PROTOCOL_VERSION.to_string());

    if !allowed {
        return ApiError::new(
            ErrorCode::UnsupportedProtocol,
            format!("нужен заголовок {HEADER_PROTOCOL}: {PROTOCOL_VERSION}"),
        )
        .into_response();
    }
    next.run(request).await
}

/// Ошибка API. Наружу уходит только код и текст, без внутренних деталей.
#[derive(Debug)]
pub struct ApiError(WireError);

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(WireError::new(code, message))
    }
}

impl From<WireError> for ApiError {
    fn from(error: WireError) -> Self {
        Self(error)
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Conflict(meta) => Self::new(
                ErrorCode::Conflict,
                format!("на сервере версия {}", meta.version),
            ),
            StoreError::PreconditionFailed => Self::new(
                ErrorCode::PreconditionFailed,
                "If-Match указывает на будущую версию",
            ),
            StoreError::QuotaExceeded { used, quota } => Self::new(
                ErrorCode::QuotaExceeded,
                format!("занято {used} из {quota} байт"),
            ),
            StoreError::Truncated { expected, got } => Self::new(
                ErrorCode::BadRequest,
                format!("тело короче заявленного: {got} из {expected}"),
            ),
            StoreError::DigestMismatch => Self::new(ErrorCode::DigestMismatch, "BLAKE3 не совпал"),
            StoreError::BadBlobId => Self::new(ErrorCode::BadRequest, "неверный идентификатор"),
            StoreError::BadDigest => Self::new(ErrorCode::BadRequest, "неверный BLAKE3"),
            StoreError::Io(_) | StoreError::Db(_) | StoreError::Locked | StoreError::Corrupt(_) => {
                tracing::error!(error = %error, "внутренняя ошибка хранилища");
                Self::new(ErrorCode::Internal, "внутренняя ошибка сервера")
            }
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::BadNonce => Self::new(ErrorCode::BadRequest, "nonce пустой или испорчен"),
            AuthError::Unauthorized => Self::new(ErrorCode::Unauthorized, "нужна авторизация"),
            AuthError::RateLimited {
                retry_after_seconds,
            } => Self::new(
                ErrorCode::RateLimited,
                format!("слишком много попыток, повторите через {retry_after_seconds} с"),
            ),
            AuthError::Store(_) => {
                tracing::error!(error = %error, "ошибка хранилища при аутентификации");
                Self::new(ErrorCode::Internal, "внутренняя ошибка сервера")
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = status_of(self.0.code);
        (status, Json(self.0)).into_response()
    }
}

fn status_of(code: ErrorCode) -> axum::http::StatusCode {
    use axum::http::StatusCode;
    match code {
        ErrorCode::BadRequest => StatusCode::BAD_REQUEST,
        ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::Conflict => StatusCode::CONFLICT,
        ErrorCode::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
        ErrorCode::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
        ErrorCode::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedProtocol => StatusCode::UPGRADE_REQUIRED,
        ErrorCode::QuotaExceeded | ErrorCode::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::VaultMissing => StatusCode::NOT_FOUND,
        ErrorCode::DigestMismatch => StatusCode::BAD_REQUEST,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Извлечённая сессия: аккаунт и устройство из `Authorization: Bearer`.
#[derive(Debug, Clone, Copy)]
pub struct Authed {
    pub account: pms_proto::identity::AccountId,
    pub device: pms_proto::identity::DeviceId,
}

impl axum::extract::FromRequestParts<AppState> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized, "нужен Bearer-токен"))?;

        let session = state.auth.authenticate(token.trim())?;
        Ok(Self {
            account: session.account,
            device: session.device,
        })
    }
}

/// Метка времени для ответов, где нужен `updatedAt` без записи в индекс.
pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Заголовок как строка, либо `400 bad_request`.
pub(crate) fn header<'a>(
    headers: &'a axum::http::HeaderMap,
    name: &str,
) -> Result<&'a str, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, format!("нужен заголовок {name}")))
}

pub(crate) fn hex_array<const N: usize>(
    headers: &axum::http::HeaderMap,
    name: &str,
) -> Result<[u8; N], ApiError> {
    let value = header(headers, name)?;
    let bytes = hex::decode(value)
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, format!("{name} не hex")))?;
    if bytes.len() != N {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            format!("{name}: ожидалось {N} байт"),
        ));
    }
    let mut array = [0u8; N];
    array.copy_from_slice(&bytes);
    Ok(array)
}
