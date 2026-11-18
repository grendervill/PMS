//! Сейф по HTTP: метаданные, потоковая отдача и приём с `If-Match`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::TryStreamExt;
use pms_proto::dto::VaultMeta;
use pms_proto::ErrorCode;
use tokio_util::io::{ReaderStream, StreamReader};

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

    let body = Body::from_stream(ReaderStream::new(file));
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, meta.size.to_string())
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
    let len = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "нужен Content-Length"))?;

    let reader = StreamReader::new(
        body.into_data_stream()
            .map_err(|error| std::io::Error::other(error.to_string())),
    );

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
    value
        .parse::<u64>()
        .map(ExpectedVersion::Version)
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "If-Match не число"))
}
