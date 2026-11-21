//! Сейф по HTTP: метаданные, потоковая отдача и приём с `If-Match`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use pms_proto::dto::VaultMeta;
use pms_proto::ErrorCode;
use tokio_util::io::ReaderStream;

use super::{ApiError, AppState, Authed, HEADER_DIGEST};
use crate::store::vault::ExpectedVersion;
use crate::store::StoreError;

pub const HEADER_VAULT_VERSION: &str = "x-pms-vault-version";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/vault/meta", get(meta))
        .route("/v1/vault", get(download).put(upload))
}

pub async fn meta(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
) -> Result<Json<VaultMeta>, ApiError> {
    match state.store.vault_meta(account)? {
        Some(meta) => Ok(Json(meta)),
        None => Err(ApiError::new(ErrorCode::VaultMissing, "сейфа ещё нет")),
    }
}

pub async fn download(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
) -> Result<Response, ApiError> {
    let Some((meta, file)) = state.store.open_vault(account).await? else {
        return Err(ApiError::new(ErrorCode::VaultMissing, "сейфа ещё нет"));
    };
    // Длину берём у файла: если запись на диск и обновление индекса разошлись
    // при сбое, отдавать заведомо неверный Content-Length нельзя.
    let size = file
        .metadata()
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(meta.size);

    let body = Body::from_stream(ReaderStream::with_capacity(file, 64 * 1024));
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, size.to_string())
        .header(header::ETAG, etag(&meta))
        .header(HEADER_VAULT_VERSION, meta.version.to_string())
        .header(HEADER_DIGEST, &meta.digest)
        .body(body)
        .map_err(|_| ApiError::new(ErrorCode::Internal, "не удалось собрать ответ"))?;
    Ok(response)
}

/// Принимает сейф потоком. Проверка квоты происходит до чтения тела.
pub async fn upload(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let expected = expected_version(&headers)?;
    let len = super::content_length(&headers)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "нужен Content-Length"))?;
    if len > state.config.limits.max_body_bytes {
        return Err(ApiError::new(
            ErrorCode::PayloadTooLarge,
            format!(
                "тело больше лимита {} байт",
                state.config.limits.max_body_bytes
            ),
        ));
    }

    let reader = Box::pin(super::body_reader(
        body,
        std::time::Duration::from_secs(state.config.limits.body_timeout_secs),
    ));

    let result = state.store.put_vault(account, expected, reader, len).await;
    let meta = match result {
        Ok(meta) => meta,
        Err(StoreError::Conflict(current)) => {
            let response = (
                StatusCode::CONFLICT,
                [
                    (HEADER_VAULT_VERSION, current.version.to_string()),
                    (HEADER_DIGEST, current.digest.clone()),
                ],
                Json(current),
            )
                .into_response();
            return Ok(response);
        }
        Err(error) => return Err(error.into()),
    };

    Ok((
        StatusCode::OK,
        [
            (HEADER_VAULT_VERSION, meta.version.to_string()),
            (HEADER_DIGEST, meta.digest.clone()),
        ],
        Json(meta),
    )
        .into_response())
}

fn etag(meta: &VaultMeta) -> String {
    format!("\"v{}-{}\"", meta.version, meta.digest)
}

/// `If-Match: 7`, `If-Match: "7"` или `If-Match: *`.
fn expected_version(headers: &HeaderMap) -> Result<ExpectedVersion, ApiError> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::new(ErrorCode::PreconditionRequired, "нужен If-Match"))?;
    let value = raw.trim().trim_matches('"');
    if value == "*" {
        return Ok(ExpectedVersion::Any);
    }
    // Принимаем и голую версию, и собственный ETag сервера вида "v7-<digest>".
    let version = value
        .strip_prefix('v')
        .and_then(|rest| rest.split('-').next())
        .unwrap_or(value);

    version
        .parse::<u64>()
        .map(ExpectedVersion::Version)
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "If-Match не число"))
}
