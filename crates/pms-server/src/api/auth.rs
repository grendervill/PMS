//! Регистрация аккаунта. Identity-подпись подтверждает, что клиент владеет
//! ключом, из которого выведен `account_id`.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use pms_proto::dto::{
    ChallengeRequest, ChallengeResponse, VaultMeta, VerifyRequest, VerifyResponse,
};
use pms_proto::identity::{normalize_login, verify_register, AccountId, DeviceId};
use pms_proto::ErrorCode;

use super::{
    body_reader, content_length, header, hex_array, now_rfc3339, ApiError, AppState,
    HEADER_ACCOUNT, HEADER_DEVICE, HEADER_LOGIN, HEADER_PUBLIC_KEY, HEADER_REGISTRATION_TOKEN,
    HEADER_SIGNATURE,
};
use crate::store::vault::ExpectedVersion;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/accounts", post(register))
        .route("/v1/auth/challenge", post(challenge))
        .route("/v1/auth/verify", post(verify))
}

fn unix_to_rfc3339(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).expect("epoch"))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub async fn challenge(
    State(state): State<AppState>,
    Json(request): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let login = normalize_login(&request.login)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "недопустимый логин"))?;
    let device = DeviceId::from_hex(&request.device_id)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "device_id не hex"))?;

    state.auth.rate_limiter().check(&login)?;

    let Some(record) = state.store.account_by_login(&login)? else {
        // Неизвестный логин и неверный пароль отвечают одинаково по форме.
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "неверный логин или пароль",
        ));
    };

    let challenge = state.auth.issue_challenge(record.account_id, device)?;
    Ok(Json(ChallengeResponse {
        nonce: hex::encode(challenge.nonce),
        expires_at: unix_to_rfc3339(challenge.expires_at),
        account_id: record.account_id.to_string(),
    }))
}

pub async fn verify(
    State(state): State<AppState>,
    Json(request): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let account = AccountId::from_hex(&request.account_id)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "account_id не hex"))?;
    let device = DeviceId::from_hex(&request.device_id)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "device_id не hex"))?;

    state.auth.rate_limiter().check(&account.to_string())?;

    // Неизвестный аккаунт и неверная подпись неразличимы для клиента.
    let record = state
        .store
        .account(account)?
        .ok_or_else(|| ApiError::new(ErrorCode::Unauthorized, "неверный логин или пароль"))?;

    let nonce: [u8; 32] = hex_array_of(&request.nonce, "nonce")?;
    let signature: [u8; 64] = hex_array_of(&request.signature, "signature")?;

    let token = state
        .auth
        .verify(account, device, &nonce, &signature, &record.public_key)?;
    state.auth.rate_limiter().forget(&account.to_string());

    Ok(Json(VerifyResponse {
        token: token.expose().to_string(),
        expires_at: unix_to_rfc3339(
            chrono::Utc::now().timestamp() + i64::from(state.config.auth.token_ttl_hours) * 3600,
        ),
    }))
}

fn hex_array_of<const N: usize>(value: &str, name: &str) -> Result<[u8; N], ApiError> {
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

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    if !state.config.registration.enabled {
        return Err(ApiError::new(
            ErrorCode::Forbidden,
            "регистрация на этом сервере выключена",
        ));
    }
    if !state
        .registrations
        .allow(state.config.registration.max_per_minute)
    {
        return Err(ApiError::new(
            ErrorCode::RateLimited,
            "слишком много регистраций, повторите через минуту",
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
    let login = normalize_login(header(&headers, HEADER_LOGIN)?)
        .ok_or_else(|| ApiError::new(ErrorCode::BadRequest, "недопустимый логин"))?;

    if AccountId::from_public_key(&public_key) != account {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "account_id не соответствует публичному ключу",
        ));
    }

    let quota = state.config.quota.default_bytes;
    let len = content_length(&headers).unwrap_or(0);
    if len > state.config.limits.max_body_bytes || len > quota {
        return Err(ApiError::new(
            ErrorCode::QuotaExceeded,
            format!("сейф не может быть больше {quota} байт"),
        ));
    }

    // Тело идёт на диск, а не в память: регистрация может нести целый сейф.
    let reader = Box::pin(body_reader(
        body,
        std::time::Duration::from_secs(state.config.limits.body_timeout_secs),
    ));
    let staged = state.store.stage_upload(reader, len).await?;

    let digest = hex::decode(staged.digest())
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| ApiError::new(ErrorCode::Internal, "хеш тела не посчитался"))?;
    if !verify_register(&public_key, &account, &device, &digest, &signature) {
        return Err(ApiError::new(
            ErrorCode::Unauthorized,
            "подпись регистрации не подходит",
        ));
    }

    if state.store.account_by_login(&login)?.is_some() {
        return Err(ApiError::new(ErrorCode::LoginTaken, "этот логин уже занят"));
    }

    let created = state
        .store
        .create_account(account, public_key, quota, &login)
        .map_err(|error| match error {
            crate::store::StoreError::Db(message) if message.contains("UNIQUE") => {
                ApiError::new(ErrorCode::LoginTaken, "этот логин уже занят")
            }
            other => other.into(),
        })?;
    if !created {
        return Err(ApiError::new(
            ErrorCode::Conflict,
            "аккаунт уже существует — выполните вход",
        ));
    }

    let meta = if staged.is_empty() {
        VaultMeta {
            version: 0,
            size: 0,
            digest: String::new(),
            updated_at: now_rfc3339(),
        }
    } else {
        let file = tokio::fs::File::open(staged.path())
            .await
            .map_err(|error| ApiError::new(ErrorCode::Internal, error.to_string()))?;
        match state
            .store
            .put_vault(account, ExpectedVersion::Any, file, staged.len())
            .await
        {
            Ok(meta) => meta,
            Err(error) => {
                // Аккаунт без сейфа оставлять нельзя: повторная регистрация
                // упрётся в 409, а входить будет некуда.
                let _ = state.store.delete_account(account);
                return Err(error.into());
            }
        }
    };

    Ok((StatusCode::CREATED, Json(meta)).into_response())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    left.len() == right.len() && left.ct_eq(right).into()
}
