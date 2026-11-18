//! HTTP-клиент сервера: проба, регистрация, вход, метаданные сейфа.

use pms_proto::dto::{
    ChallengeRequest, ChallengeResponse, VaultMeta, VerifyRequest, VerifyResponse,
};
use pms_proto::identity::{sign_challenge, sign_register, AccountId, DeviceId, Identity};
use pms_proto::{ErrorCode, WireError, PROTOCOL_VERSION};

use super::tls;
use super::{RemoteError, RemoteProfile};

pub const HEADER_PROTOCOL: &str = "x-pms-protocol";
pub const HEADER_ACCOUNT: &str = "x-pms-account-id";
pub const HEADER_PUBLIC_KEY: &str = "x-pms-public-key";
pub const HEADER_DEVICE: &str = "x-pms-device-id";
pub const HEADER_SIGNATURE: &str = "x-pms-signature";
pub const HEADER_DIGEST: &str = "x-pms-digest";
pub const HEADER_VAULT_VERSION: &str = "x-pms-vault-version";

pub struct RemoteClient {
    http: reqwest::Client,
    base: url::Url,
}

/// Ответ на проверку живости.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Probe {
    pub protocol: u32,
}

/// Выданный сервером токен сессии.
pub struct SessionToken(String);

impl SessionToken {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SessionToken {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionToken(<скрыт>)")
    }
}

pub struct Login {
    pub account: AccountId,
    pub token: SessionToken,
    pub expires_at: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Login")
            .field("account", &self.account.to_string())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl RemoteClient {
    pub fn new(profile: &RemoteProfile) -> Result<Self, RemoteError> {
        let http = tls::build_client(profile)?;
        let base = tls::base_url(profile)?;
        Ok(Self { http, base })
    }

    fn endpoint(&self, path: &str) -> Result<url::Url, RemoteError> {
        self.base
            .join(path)
            .map_err(|error| RemoteError::BadUrl(error.to_string()))
    }

    pub async fn probe(&self) -> Result<Probe, RemoteError> {
        let response = self
            .http
            .get(self.endpoint("/v1/healthz")?)
            .send()
            .await
            .map_err(unreachable)?;

        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        let probe: Probe = response.json().await.map_err(unreachable)?;
        if probe.protocol != PROTOCOL_VERSION {
            return Err(RemoteError::Protocol(format!(
                "сервер на версии {}, клиент на {PROTOCOL_VERSION}",
                probe.protocol
            )));
        }
        Ok(probe)
    }

    pub async fn register(
        &self,
        identity: &Identity,
        device: DeviceId,
        vault: &[u8],
    ) -> Result<VaultMeta, RemoteError> {
        let account = AccountId::from_public_key(&identity.public);
        let digest = *blake3::hash(vault).as_bytes();
        let signature = sign_register(identity, &account, &device, &digest);

        let response = self
            .http
            .post(self.endpoint("/v1/accounts")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .header(HEADER_ACCOUNT, account.to_string())
            .header(HEADER_PUBLIC_KEY, hex::encode(identity.public))
            .header(HEADER_DEVICE, device.to_string())
            .header(HEADER_SIGNATURE, hex::encode(signature))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(vault.to_vec())
            .send()
            .await
            .map_err(unreachable)?;

        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(RemoteError::BadRequest(
                "аккаунт уже существует на этом сервере — выполните вход".to_string(),
            ));
        }
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        response.json().await.map_err(unreachable)
    }

    pub async fn login(
        &self,
        identity: &Identity,
        device: DeviceId,
    ) -> Result<Login, RemoteError> {
        let account = AccountId::from_public_key(&identity.public);

        let response = self
            .http
            .post(self.endpoint("/v1/auth/challenge")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .json(&ChallengeRequest {
                account_id: account.to_string(),
                device_id: device.to_string(),
            })
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        let challenge: ChallengeResponse = response.json().await.map_err(unreachable)?;

        let nonce = decode_nonce(&challenge.nonce)?;
        let signature = sign_challenge(identity, &account, &device, &nonce);

        let response = self
            .http
            .post(self.endpoint("/v1/auth/verify")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .json(&VerifyRequest {
                account_id: account.to_string(),
                device_id: device.to_string(),
                nonce: challenge.nonce,
                signature: hex::encode(signature),
            })
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        let verified: VerifyResponse = response.json().await.map_err(unreachable)?;

        Ok(Login {
            account,
            token: SessionToken(verified.token),
            expires_at: verified.expires_at,
        })
    }

    /// Метаданные сейфа на сервере (`None`, если сейфа ещё нет).
    pub async fn vault_meta(&self, token: &str) -> Result<Option<VaultMeta>, RemoteError> {
        let response = self
            .http
            .get(self.endpoint("/v1/vault/meta")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .bearer_auth(token)
            .send()
            .await
            .map_err(unreachable)?;

        match response.status() {
            status if status.is_success() => Ok(Some(response.json().await.map_err(unreachable)?)),
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            _ => Err(error_from_response(response).await),
        }
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

pub(crate) fn unreachable(error: reqwest::Error) -> RemoteError {
    if error.is_builder() {
        return RemoteError::BadUrl(error.to_string());
    }
    // Ошибка рукопожатия приходит как «ошибка соединения»: без этого разбора
    // пользователь видел бы «сервер недоступен» вместо «сертификат не тот».
    if is_tls_error(&error) {
        return RemoteError::Tls(error.to_string());
    }
    RemoteError::Unreachable(error.to_string())
}

fn is_tls_error(error: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if current.downcast_ref::<rustls::Error>().is_some() {
            return true;
        }
        let text = current.to_string().to_lowercase();
        if text.contains("certificate")
            || text.contains("handshake")
            || text.contains("tls alert")
            || text.contains("invalid peer certificate")
        {
            return true;
        }
        source = current.source();
    }
    false
}

/// Разбирает тело ошибки сервера в наш `RemoteError`.
pub(crate) async fn error_from_response(response: reqwest::Response) -> RemoteError {
    let status = response.status();
    let code = status.as_u16();
    let body = response.text().await.unwrap_or_default();

    if let Ok(wire) = serde_json::from_str::<WireError>(&body) {
        if wire.code == ErrorCode::Conflict {
            if let Ok(meta) = serde_json::from_str::<VaultMeta>(&body) {
                return RemoteError::Conflict(Box::new(meta));
            }
        }
        return RemoteError::from_wire(wire.code, code, wire.message);
    }

    RemoteError::Server {
        status: code,
        message: body,
    }
}

fn decode_nonce(value: &str) -> Result<[u8; 32], RemoteError> {
    let bytes = hex::decode(value)
        .map_err(|_| RemoteError::Protocol("сервер прислал nonce не в hex".to_string()))?;
    <[u8; 32]>::try_from(bytes)
        .map_err(|_| RemoteError::Protocol("nonce неверной длины".to_string()))
}
