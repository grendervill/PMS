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
pub const HEADER_LOGIN: &str = "x-pms-login";
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
    pub device: DeviceId,
}

/// Итог регистрации: аккаунт создан или уже был, и готовая сессия.
pub struct Registration {
    pub created: bool,
    pub login: Login,
    pub vault: Option<VaultMeta>,
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

    pub(crate) fn endpoint(&self, path: &str) -> Result<url::Url, RemoteError> {
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

    /// Регистрирует аккаунт (тело сейфа — файлом, чтобы не держать его в памяти).
    ///
    /// Если аккаунт уже есть, вход выполняется сразу: для пользователя это один
    /// экран «введите мастер-ключ», а не два сценария.
    pub async fn register(
        &self,
        identity: &Identity,
        device: DeviceId,
        login: &str,
        vault: Option<&std::path::Path>,
    ) -> Result<Registration, RemoteError> {
        let login = pms_proto::identity::normalize_login(login)
            .ok_or_else(|| RemoteError::BadRequest("недопустимый логин".to_string()))?;
        let account = AccountId::from_public_key(&identity.public);
        let digest = match vault {
            Some(path) => crate::core::remote::blobs::digest_of_file(path).await?,
            None => blake3::hash(b"").to_hex().to_string(),
        };
        let digest_bytes: [u8; 32] = hex::decode(&digest)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .ok_or_else(|| RemoteError::Protocol("хеш сейфа не посчитался".to_string()))?;
        let signature = sign_register(identity, &account, &device, &digest_bytes);

        let (body, length) = match vault {
            Some(path) => {
                let metadata = tokio::fs::metadata(path).await.map_err(storage)?;
                let file = tokio::fs::File::open(path).await.map_err(storage)?;
                (
                    reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file)),
                    metadata.len(),
                )
            }
            None => (reqwest::Body::from(Vec::new()), 0),
        };

        let response = self
            .http
            .post(self.endpoint("/v1/accounts")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .header(HEADER_ACCOUNT, account.to_string())
            .header(HEADER_PUBLIC_KEY, hex::encode(identity.public))
            .header(HEADER_DEVICE, device.to_string())
            .header(HEADER_LOGIN, &login)
            .header(HEADER_SIGNATURE, hex::encode(signature))
            .header(reqwest::header::CONTENT_LENGTH, length)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(unreachable)?;

        if response.status() == reqwest::StatusCode::CONFLICT {
            let session = self.login(identity, device, &login).await?;
            return Ok(Registration {
                created: false,
                login: session,
                vault: None,
            });
        }
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        // Сервер отвечает метаданными загруженного сейфа: второй запрос не нужен.
        let meta: VaultMeta = response.json().await.map_err(unreachable)?;

        let session = self.login(identity, device, &login).await?;
        Ok(Registration {
            created: true,
            login: session,
            vault: Some(meta),
        })
    }

    pub async fn login(
        &self,
        identity: &Identity,
        device: DeviceId,
        login: &str,
    ) -> Result<Login, RemoteError> {
        let account = AccountId::from_public_key(&identity.public);
        let login = pms_proto::identity::normalize_login(login)
            .ok_or_else(|| RemoteError::BadRequest("недопустимый логин".to_string()))?;

        let response = self
            .http
            .post(self.endpoint("/v1/auth/challenge")?)
            .header(HEADER_PROTOCOL, PROTOCOL_VERSION.to_string())
            .json(&ChallengeRequest {
                login: login.clone(),
                device_id: device.to_string(),
            })
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(error_from_response(response).await);
        }
        let challenge: ChallengeResponse = response.json().await.map_err(unreachable)?;

        // Сервер нашёл аккаунт по логину. Если он не совпал с выведенным из
        // логина и мастер-ключа — это другой пользователь или чужой сервер.
        if challenge.account_id != account.to_string() {
            return Err(RemoteError::BadRequest(
                "логин на сервере принадлежит другому ключу".to_string(),
            ));
        }

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
            device,
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
            // 404 бывает двух видов: «сейфа ещё нет» и «такого метода нет» у
            // прокси. Первый — это пустой аккаунт, второй — ошибка адреса.
            reqwest::StatusCode::NOT_FOUND => {
                // Ограничиваем тело ошибки: враждебный/битый сервер не должен раздувать
                // память приложения своим ответом.
                let body = response.text().await.unwrap_or_default();
                let body = if body.len() > 4096 {
                    body.chars().take(4096).collect::<String>()
                } else {
                    body
                };
                match serde_json::from_str::<WireError>(&body) {
                    Ok(wire) if wire.code == ErrorCode::VaultMissing => Ok(None),
                    Ok(wire) => Err(RemoteError::from_wire(wire.code, 404, wire.message)),
                    Err(_) => Err(RemoteError::Server {
                        status: 404,
                        message: "по этому адресу нет PMS-сервера".to_string(),
                    }),
                }
            }
            _ => Err(error_from_response(response).await),
        }
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

fn storage(error: std::io::Error) -> RemoteError {
    RemoteError::Storage(error.to_string())
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
            || text.contains("tls")
            || text.contains("отпечаток")
            || text.contains("alert")
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
    // Ограничиваем тело ошибки: враждебный/битый сервер не должен раздувать
    // память приложения своим ответом.
    let body = response.text().await.unwrap_or_default();
    let body = if body.len() > 4096 {
        body.chars().take(4096).collect::<String>()
    } else {
        body
    };

    // На 409 сервер отдаёт сами метаданные сейфа: клиенту нужна версия, чтобы
    // решить конфликт без второго запроса.
    if status == reqwest::StatusCode::CONFLICT {
        if let Ok(meta) = serde_json::from_str::<VaultMeta>(&body) {
            return RemoteError::Conflict(Box::new(meta));
        }
    }
    if let Ok(wire) = serde_json::from_str::<WireError>(&body) {
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
