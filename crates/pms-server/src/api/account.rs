//! Использование квоты и удаление аккаунта.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use pms_proto::dto::{Deleted, Usage};
use pms_proto::identity::verify_account_delete;
use pms_proto::ErrorCode;

use super::{hex_array, ApiError, AppState, Authed, HEADER_SIGNATURE};

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

    let record = state
        .store
        .account(account)?
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "аккаунта нет"))?;
    if !verify_account_delete(&record.public_key, &account, &device, &signature) {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись удаления не подходит",
        ));
    }

    state.auth.revoke_all(account)?;
    state.store.delete_account(account)?;
    Ok((StatusCode::OK, Json(Deleted { deleted: true })))
}
