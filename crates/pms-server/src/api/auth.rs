//! Регистрация аккаунта. Identity-подпись подтверждает, что клиент владеет
//! ключом, из которого выведен `account_id`.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use pms_proto::dto::VaultMeta;
use pms_proto::identity::{verify_register, AccountId, DeviceId};
use pms_proto::ErrorCode;

use super::{
    header, hex_array, now_rfc3339, ApiError, AppState, HEADER_ACCOUNT, HEADER_DEVICE,
    HEADER_PUBLIC_KEY, HEADER_REGISTRATION_TOKEN, HEADER_SIGNATURE,
};
use crate::store::vault::ExpectedVersion;

pub fn routes() -> Router<AppState> {
    Router::new().route("/v1/accounts", post(register))
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if !state.config.registration.enabled {
        return Err(ApiError::new(
            ErrorCode::Forbidden,
            "регистрация на этом сервере выключена",
        ));
    }
    let expected_token = &state.config.registration.token;
    if !expected_token.is_empty() {
        let provided = headers
            .get(HEADER_REGISTRATION_TOKEN)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !constant_time_eq(provided.as_bytes(), expected_token.as_bytes()) {
            return Err(ApiError::new(
                ErrorCode::Forbidden,
                "неверный токен регистрации",
            ));
        }
    }

    let account = AccountId::from_hex(header(&headers, HEADER_ACCOUNT)?)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "account_id не hex"))?;
    let public_key: [u8; 32] = hex_array(&headers, HEADER_PUBLIC_KEY)?;
    let device = DeviceId::from_hex(header(&headers, HEADER_DEVICE)?)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "device_id не hex"))?;
    let signature: [u8; 64] = hex_array(&headers, HEADER_SIGNATURE)?;

    if AccountId::from_public_key(&public_key) != account {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "account_id не соответствует публичному ключу",
        ));
    }

    let digest = *blake3::hash(&body).as_bytes();
    if !verify_register(&public_key, &account, &device, &digest, &signature) {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись регистрации не подходит",
        ));
    }

    let created =
        state
            .store
            .create_account(account, public_key, state.config.quota.default_bytes)?;
    if !created {
        return Err(ApiError::new(
            ErrorCode::Conflict,
            "аккаунт уже существует — выполните вход",
        ));
    }

    let meta = if body.is_empty() {
        VaultMeta {
            version: 0,
            size: 0,
            digest: String::new(),
            updated_at: now_rfc3339(),
        }
    } else {
        state
            .store
            .put_vault(
                account,
                ExpectedVersion::Any,
                body.as_ref(),
                body.len() as u64,
            )
            .await?
    };

    Ok((StatusCode::CREATED, Json(meta)).into_response())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    left.len() == right.len() && left.ct_eq(right).into()
}
