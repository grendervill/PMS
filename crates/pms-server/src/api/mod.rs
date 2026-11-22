//! HTTP-слой: роутер, лимиты, единый формат ошибок.

pub mod account;
pub mod auth;
pub mod blobs;
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
pub const HEADER_LOGIN: &str = "x-pms-login";
pub const HEADER_SIGNATURE: &str = "x-pms-signature";
pub const HEADER_TIMESTAMP: &str = "x-pms-timestamp";
pub const HEADER_DIGEST: &str = "x-pms-digest";
pub const HEADER_REGISTRATION_TOKEN: &str = "x-pms-registration-token";

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub auth: Arc<Auth>,
    pub config: Arc<Config>,
    /// Бюджет регистраций на весь сервер: без него один хост создаёт аккаунты
    /// пачками, а каждый аккаунт получает полную квоту.
    pub registrations: Arc<RegistrationWindow>,
}

impl AppState {
    pub fn new(store: Arc<Store>, auth: Arc<Auth>, config: Arc<Config>) -> Self {
        Self {
            store,
            auth,
            config,
            registrations: Arc::new(RegistrationWindow::default()),
        }
    }
}

/// Простое окно «N регистраций в минуту».
pub struct RegistrationWindow {
    state: std::sync::Mutex<(std::time::Instant, u32)>,
}

impl Default for RegistrationWindow {
    fn default() -> Self {
        Self {
            state: std::sync::Mutex::new((std::time::Instant::now(), 0)),
        }
    }
}

impl RegistrationWindow {
    pub fn allow(&self, limit: u32) -> bool {
        if limit == 0 {
            return true;
        }
        let mut state = self.state.lock().expect("registration window poisoned");
        let now = std::time::Instant::now();
        if now.duration_since(state.0).as_secs() >= 60 {
            *state = (now, 0);
        }
        if state.1 >= limit {
            return false;
        }
        state.1 += 1;
        true
    }
}

pub fn router(state: AppState) -> Router {
    let limits = state.config.limits.clone();

    // Живость проверяется вне лимита соединений: иначе под нагрузкой сервер
    // выглядит мёртвым именно тогда, когда он занят работой.
    let health = Router::new().route("/v1/healthz", get(healthz));

    let api: Router<AppState> = Router::new()
        .merge(auth::routes())
        .merge(vault::routes())
        .merge(blobs::routes())
        .merge(account::routes())
        .layer(DefaultBodyLimit::max(limits.max_body_bytes as usize))
        .layer(ConcurrencyLimitLayer::new(limits.max_connections));

    Router::new()
        .merge(health)
        .merge(api)
        .fallback(not_found)
        .layer(middleware::from_fn(check_protocol))
        .with_state(state)
}

/// Поток тела с таймаутом на каждый кусок.
///
/// Без него одна медленная загрузка держит блокировку аккаунта и место в лимите
/// соединений сколько угодно, а временный файл растёт.
pub(crate) fn body_reader(
    body: axum::body::Body,
    timeout: std::time::Duration,
) -> tokio_util::io::StreamReader<
    impl futures_util::Stream<Item = std::io::Result<axum::body::Bytes>>,
    axum::body::Bytes,
> {
    use futures_util::{StreamExt, TryStreamExt};

    let stream = body
        .into_data_stream()
        .map_err(|error| std::io::Error::other(error.to_string()));
    let timed = tokio_stream::StreamExt::timeout(stream, timeout);
    let mapped = timed.map(|item| match item {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(error),
        Err(_elapsed) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "тело запроса не пришло вовремя",
        )),
    });
    tokio_util::io::StreamReader::new(mapped)
}

/// Минимальная скорость приёма тела: ниже — запрос обрывается.
const MIN_BYTES_PER_SEC: u64 = 512 * 1024;
/// JSON-тела наших запросов крошечные; больше — уже подозрительно.
const MAX_JSON_BYTES: usize = 16 * 1024;
const JSON_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// Бюджет на **весь** приём тела: базовый таймаут плюс время на передачу по
/// минимальной скорости. Таймаут на кусок такой защиты не даёт: байт в минуту
/// не нарушает его никогда.
pub(crate) fn body_budget(limits: &crate::config::Limits, len: u64) -> std::time::Duration {
    std::time::Duration::from_secs(limits.body_timeout_secs)
        + std::time::Duration::from_secs(len / MIN_BYTES_PER_SEC)
}

/// Читает небольшое JSON-тело с общим дедлайном.
pub(crate) async fn json_body<T: serde::de::DeserializeOwned>(
    body: axum::body::Body,
    len: u64,
) -> Result<T, ApiError> {
    if len as usize > MAX_JSON_BYTES {
        return Err(ApiError::new(
            ErrorCode::PayloadTooLarge,
            "тело слишком большое",
        ));
    }

    let read = tokio::time::timeout(JSON_DEADLINE, http_body_util::BodyExt::collect(body))
        .await
        .map_err(|_| ApiError::new(ErrorCode::Timeout, "тело запроса не пришло вовремя"))?
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "тело запроса не прочитано"))?;

    serde_json::from_slice(&read.to_bytes())
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "тело не JSON"))
}

pub(crate) fn content_length(headers: &axum::http::HeaderMap) -> Option<u64> {
    headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

/// Тело собирается один раз при первом обращении: у проверки живости не должно
/// быть ни сериализации, ни аллокаций на запрос.
fn healthz_body() -> &'static str {
    static BODY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BODY.get_or_init(|| format!("{{\"status\":\"ok\",\"protocol\":{PROTOCOL_VERSION}}}"))
}

async fn healthz() -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        healthz_body(),
    )
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
        .and_then(|value| value.parse::<u32>().ok())
        == Some(PROTOCOL_VERSION);

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
            StoreError::NoSpace { used, capacity } => {
                tracing::warn!(used, capacity, "нет места под временные файлы");
                Self::new(
                    ErrorCode::Unavailable,
                    "на сервере временно нет места, попробуйте позже",
                )
            }
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
        ErrorCode::Timeout => StatusCode::REQUEST_TIMEOUT,
        ErrorCode::VaultMissing => StatusCode::NOT_FOUND,
        ErrorCode::LoginTaken => StatusCode::CONFLICT,
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

        // Проверка токена ходит в SQLite: уводим её с воркера, чтобы запросы
        // других пользователей не ждали чужой индекс.
        let auth = std::sync::Arc::clone(&state.auth);
        let presented = token.trim().to_string();
        let session = tokio::task::spawn_blocking(move || auth.authenticate(&presented))
            .await
            .map_err(|_| ApiError::new(ErrorCode::Internal, "внутренняя ошибка сервера"))??;
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
