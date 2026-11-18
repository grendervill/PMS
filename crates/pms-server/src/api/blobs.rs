//! Вложения по HTTP: загрузка с проверкой дайджеста, отдача потоком, список и
//! удаление (сборка мусора делает клиент по правилам спеки §10).

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use pms_proto::dto::BlobInfo;
use pms_proto::ErrorCode;
use tokio_util::io::ReaderStream;

use super::{header as header_value, ApiError, AppState, Authed, HEADER_DIGEST};
use crate::store::StoreError;

pub fn routes() -> Router<AppState> {
    Router::new().route("/v1/blobs", get(list)).route(
        "/v1/blobs/{blob_id}",
        get(download).put(upload).delete(remove).head(head),
    )
}

async fn list(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
) -> Result<Json<Vec<BlobInfo>>, ApiError> {
    Ok(Json(state.store.list_blobs(account)?))
}

async fn download(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
    Path(blob_id): Path<String>,
) -> Result<Response, ApiError> {
    let Some((info, file)) = state.store.get_blob(account, &blob_id).await? else {
        return Err(ApiError::new(ErrorCode::NotFound, "блоба нет"));
    };
    state.store.bump_blob_use(account, &blob_id)?;

    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, info.size.to_string())
        .header(HEADER_DIGEST, info.digest)
        .body(body)
        .map_err(|_| ApiError::new(ErrorCode::Internal, "не удалось собрать ответ"))
}

async fn head(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
    Path(blob_id): Path<String>,
) -> Result<Response, ApiError> {
    let Some(info) = state.store.blob_info(account, &blob_id)? else {
        return Err(ApiError::new(ErrorCode::NotFound, "блоба нет"));
    };
    state.store.bump_blob_use(account, &blob_id)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, info.size.to_string())
        .header(HEADER_DIGEST, info.digest)
        .body(Body::empty())
        .map_err(|_| ApiError::new(ErrorCode::Internal, "не удалось собрать ответ"))
}

async fn upload(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
    Path(blob_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let digest = header_value(&headers, HEADER_DIGEST)?;
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

    let info = state
        .store
        .put_blob(account, &blob_id, digest, reader, len)
        .await?;
    Ok((StatusCode::OK, Json(info)).into_response())
}

async fn remove(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
    Path(blob_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    match state.store.delete_blob(account, &blob_id) {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(ApiError::new(ErrorCode::NotFound, "блоба нет")),
        Err(StoreError::BadBlobId) => Err(ApiError::new(ErrorCode::BadRequest, "неверный id")),
        Err(error) => Err(error.into()),
    }
}
