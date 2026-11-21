//! Использование квоты и удаление аккаунта.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use pms_proto::dto::{Deleted, Usage};
use pms_proto::identity::verify_account_delete;
use pms_proto::ErrorCode;

use super::{hex_array, ApiError, AppState, Authed, HEADER_SIGNATURE, HEADER_TIMESTAMP};

/// Окно, в котором принимается подпись удаления.
const DELETE_WINDOW_SECONDS: i64 = 120;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/usage", get(usage))
        .route("/v1/account", axum::routing::delete(delete_account))
}

async fn usage(
    State(state): State<AppState>,
    Authed { account, .. }: Authed,
) -> Result<Json<Usage>, ApiError> {
    Ok(Json(state.store.usage(account)?))
}

/// Удаление необратимо, поэтому требует свежей подписи ключом аккаунта.
async fn delete_account(
    State(state): State<AppState>,
    Authed { account, device }: Authed,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Deleted>), ApiError> {
    let signature: [u8; 64] = hex_array(&headers, HEADER_SIGNATURE)?;

    // Подпись живёт в окне: иначе один подсмотренный запрос стирает аккаунт
    // в любой момент времени.
    let timestamp = super::header(&headers, HEADER_TIMESTAMP)?
        .parse::<i64>()
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "x-pms-timestamp не число"))?;
    let now = chrono::Utc::now().timestamp();
    if (now - timestamp).abs() > DELETE_WINDOW_SECONDS {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись удаления устарела",
        ));
    }

    let record = state
        .store
        .account(account)?
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "аккаунта нет"))?;
    if timestamp <= record.last_delete_at {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись удаления уже использована",
        ));
    }
    if !verify_account_delete(&record.public_key, &account, &device, timestamp, &signature) {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись удаления не подходит",
        ));
    }

    state.auth.revoke_all(account)?;
    state.store.touch_delete_mark(account, timestamp)?;
    match state.store.delete_account(account).await {
        Ok(_) => Ok((StatusCode::OK, Json(Deleted { deleted: true }))),
        Err(error) => Err(error.into()),
    }
}
