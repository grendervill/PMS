//! Команды серверного режима: вход, синхронизация сейфа, вложения и квота.
//!
//! Токены и семя identity-ключа живут здесь, в памяти Rust. В рендерер уходят
//! только идентификатор аккаунта, метаданные и цифры квоты.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pms_proto::dto::{BlobInfo, Usage, VaultMeta};
use pms_proto::identity::{AccountId, DeviceId, Identity};
use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::commands::fs_scope::Access;
use crate::commands::vault::UnlockSecretDto;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::crypto::UnlockSecretInput;
use crate::core::remote::client::{Login, RemoteClient};
use crate::core::remote::identity::{device_id, identity_from_secret};
use crate::core::remote::{RemoteError, RemoteProfile};

/// Одна активная сессия на аккаунт.
pub struct RemoteSession {
    pub profile: RemoteProfile,
    pub account: AccountId,
    pub device: DeviceId,
    /// Семя identity-ключа: нужно, чтобы войти заново без мастер-ключа,
    /// пока сессия жива (биометрия, повторный challenge через 12 часов).
    pub auth_seed: Zeroizing<[u8; 32]>,
    pub token: Zeroizing<String>,
    pub expires_at: String,
}

impl std::fmt::Debug for RemoteSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteSession")
            .field("account", &self.account.to_string())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct RemoteRegistry {
    sessions: Mutex<HashMap<AccountId, RemoteSession>>,
}

impl RemoteRegistry {
    pub fn put(&self, session: RemoteSession) {
        let mut sessions = self.sessions.lock().expect("remote registry poisoned");
        sessions.insert(session.account, session);
    }

    pub fn token(&self, account: AccountId) -> ApiResult<String> {
        let sessions = self.sessions.lock().expect("remote registry poisoned");
        sessions
            .get(&account)
            .map(|session| session.token.to_string())
            .ok_or_else(|| RemoteError::Unauthorized.as_api_error())
    }

    pub fn profile(&self, account: AccountId) -> ApiResult<RemoteProfile> {
        let sessions = self.sessions.lock().expect("remote registry poisoned");
        sessions
            .get(&account)
            .map(|session| session.profile.clone())
            .ok_or_else(|| RemoteError::Unauthorized.as_api_error())
    }

    pub fn remove(&self, account: AccountId) -> bool {
        let mut sessions = self.sessions.lock().expect("remote registry poisoned");
        sessions.remove(&account).is_some()
    }

    pub fn list(&self) -> Vec<RemoteStatus> {
        let sessions = self.sessions.lock().expect("remote registry poisoned");
        sessions
            .values()
            .map(|session| RemoteStatus {
                account_id: session.account.to_string(),
                base_url: session.profile.base_url.clone(),
                device_id: session.device.to_string(),
                expires_at: session.expires_at.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub account_id: String,
    pub base_url: String,
    pub device_id: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub protocol: u32,
    pub spki_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    pub account_id: String,
    pub device_id: String,
    pub expires_at: String,
    pub vault: Option<VaultMeta>,
}

// ---------------------------------------------------------------------------
// Реализация, отделённая от Tauri: её и проверяют тесты
// ---------------------------------------------------------------------------

pub(crate) async fn probe(profile: &RemoteProfile) -> ApiResult<ProbeInfo> {
    let client = RemoteClient::new(profile).map_err(to_api)?;
    let probe = client.probe().await.map_err(to_api)?;
    let spki_sha256 = crate::core::remote::tls::fetch_spki_sha256(&profile.base_url)
        .await
        .map_err(to_api)?;
    Ok(ProbeInfo {
        protocol: probe.protocol,
        spki_sha256,
    })
}

pub(crate) async fn fingerprint(base_url: &str) -> ApiResult<String> {
    crate::core::remote::tls::fetch_spki_sha256(base_url)
        .await
        .map_err(to_api)
}

pub(crate) async fn login(
    state: &AppState,
    profile: &RemoteProfile,
    secret: &UnlockSecretInput,
) -> ApiResult<LoginInfo> {
    let identity = identity_from_secret(secret).map_err(to_api)?;
    let device = device_id().map_err(to_api)?;
    let client = RemoteClient::new(profile).map_err(to_api)?;
    let session = client
        .login(&identity, DeviceId::new(device))
        .await
        .map_err(to_api)?;

    let vault = client
        .vault_meta(session.token.expose())
        .await
        .map_err(to_api)?;
    let info = store_session(
        state,
        profile.clone(),
        &identity,
        device,
        session,
        vault.clone(),
    );
    Ok(info)
}

pub(crate) async fn register(
    state: &AppState,
    profile: &RemoteProfile,
    secret: &UnlockSecretInput,
    vault_folder: Option<PathBuf>,
) -> ApiResult<LoginInfo> {
    let identity = identity_from_secret(secret).map_err(to_api)?;
    let device = device_id().map_err(to_api)?;
    let client = RemoteClient::new(profile).map_err(to_api)?;

    let vault = match &vault_folder {
        Some(folder) => {
            let path = folder.join("pms.vault");
            state.fs_scope.grant(&path, Access::Read)?;
            tokio::fs::read(&path)
                .await
                .map_err(|error| ApiError::new("storage", error.to_string()))?
        }
        None => Vec::new(),
    };

    client
        .register(&identity, DeviceId::new(device), &vault)
        .await
        .map_err(to_api)?;

    let session = client
        .login(&identity, DeviceId::new(device))
        .await
        .map_err(to_api)?;
    let meta = client
        .vault_meta(session.token.expose())
        .await
        .map_err(to_api)?;
    Ok(store_session(
        state,
        profile.clone(),
        &identity,
        device,
        session,
        meta,
    ))
}

fn store_session(
    state: &AppState,
    profile: RemoteProfile,
    identity: &Identity,
    device: [u8; 16],
    session: Login,
    vault: Option<VaultMeta>,
) -> LoginInfo {
    let info = LoginInfo {
        account_id: session.account.to_string(),
        device_id: DeviceId::new(device).to_string(),
        expires_at: session.expires_at.clone(),
        vault,
    };
    state.remote.put(RemoteSession {
        profile,
        account: session.account,
        device: DeviceId::new(device),
        auth_seed: Zeroizing::new(*identity.secret),
        token: Zeroizing::new(session.token.expose().to_string()),
        expires_at: session.expires_at,
    });
    info
}

fn client_for(state: &AppState, account: AccountId) -> ApiResult<(RemoteClient, String)> {
    let profile = state.remote.profile(account)?;
    let token = state.remote.token(account)?;
    Ok((RemoteClient::new(&profile).map_err(to_api)?, token))
}

pub(crate) async fn vault_meta(
    state: &AppState,
    account: AccountId,
) -> ApiResult<Option<VaultMeta>> {
    let (client, token) = client_for(state, account)?;
    client.vault_meta(&token).await.map_err(to_api)
}

pub(crate) async fn pull_vault(
    state: &AppState,
    account: AccountId,
    folder: &Path,
) -> ApiResult<VaultMeta> {
    let (client, token) = client_for(state, account)?;
    state.fs_scope.grant(folder, Access::Write)?;
    client.pull_vault(&token, folder).await.map_err(to_api)
}

pub(crate) async fn push_vault(
    state: &AppState,
    account: AccountId,
    folder: &Path,
    expected: Option<u64>,
) -> ApiResult<VaultMeta> {
    let (client, token) = client_for(state, account)?;
    let path = folder.join("pms.vault");
    state.fs_scope.grant(&path, Access::Read)?;
    client
        .push_vault(&token, expected, &path)
        .await
        .map_err(to_api)
}

pub(crate) async fn usage(state: &AppState, account: AccountId) -> ApiResult<Usage> {
    let (client, token) = client_for(state, account)?;
    client.usage(&token).await.map_err(to_api)
}

pub(crate) async fn list_blobs(state: &AppState, account: AccountId) -> ApiResult<Vec<BlobInfo>> {
    let (client, token) = client_for(state, account)?;
    client.list_blobs(&token).await.map_err(to_api)
}

pub(crate) async fn upload_blob(
    state: &AppState,
    account: AccountId,
    blob_id: String,
    path: PathBuf,
) -> ApiResult<BlobInfo> {
    let (client, token) = client_for(state, account)?;
    state.fs_scope.grant(&path, Access::Read)?;
    client
        .put_blob(&token, &blob_id, &path)
        .await
        .map_err(to_api)
}

pub(crate) async fn download_blob(
    state: &AppState,
    account: AccountId,
    blob_id: String,
    dest: PathBuf,
) -> ApiResult<BlobInfo> {
    let (client, token) = client_for(state, account)?;
    state.fs_scope.grant_target(&dest, Access::Write)?;
    client
        .get_blob(&token, &blob_id, &dest)
        .await
        .map_err(to_api)
}

pub(crate) async fn delete_blob(
    state: &AppState,
    account: AccountId,
    blob_id: String,
) -> ApiResult<bool> {
    let (client, token) = client_for(state, account)?;
    client.delete_blob(&token, &blob_id).await.map_err(to_api)
}

pub(crate) async fn delete_account(
    state: &AppState,
    account: AccountId,
    secret: &UnlockSecretInput,
) -> ApiResult<()> {
    let (client, token) = client_for(state, account)?;
    let identity = identity_from_secret(secret).map_err(to_api)?;
    let device = state
        .remote
        .list()
        .into_iter()
        .find(|status| status.account_id == account.to_string())
        .and_then(|status| DeviceId::from_hex(&status.device_id))
        .ok_or_else(|| RemoteError::Unauthorized.as_api_error())?;

    client
        .delete_account(&token, &identity, device)
        .await
        .map_err(to_api)?;
    let _ = state.remote.remove(account);
    Ok(())
}

/// Продлевает сессию сохранённым семенем: мастер-ключ для этого не нужен.
pub(crate) async fn refresh(state: &AppState, account: AccountId) -> ApiResult<LoginInfo> {
    let (profile, seed, device) = {
        let sessions = state
            .remote
            .sessions
            .lock()
            .expect("remote registry poisoned");
        let session = sessions
            .get(&account)
            .ok_or_else(|| RemoteError::Unauthorized.as_api_error())?;
        (
            session.profile.clone(),
            Zeroizing::new(*session.auth_seed),
            session.device,
        )
    };

    let identity = crate::core::remote::identity::identity_from_seed(&seed);
    let client = RemoteClient::new(&profile).map_err(to_api)?;
    let session = client.login(&identity, device).await.map_err(to_api)?;
    let vault = client
        .vault_meta(session.token.expose())
        .await
        .map_err(to_api)?;

    let info = LoginInfo {
        account_id: account.to_string(),
        device_id: device.to_string(),
        expires_at: session.expires_at.clone(),
        vault,
    };
    state.remote.put(RemoteSession {
        profile,
        account,
        device,
        auth_seed: seed,
        token: Zeroizing::new(session.token.expose().to_string()),
        expires_at: session.expires_at,
    });
    Ok(info)
}

pub(crate) fn logout(state: &AppState, account: AccountId) -> ApiResult<bool> {
    Ok(state.remote.remove(account))
}

pub(crate) fn status(state: &AppState) -> Vec<RemoteStatus> {
    state.remote.list()
}

fn to_api(error: RemoteError) -> ApiError {
    error.as_api_error()
}

// ---------------------------------------------------------------------------
// Тонкие обёртки команд
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn remote_probe(profile: RemoteProfile) -> ApiResult<ProbeInfo> {
    probe(&profile).await
}

#[tauri::command]
pub async fn remote_fingerprint(base_url: String) -> ApiResult<String> {
    fingerprint(&base_url).await
}

#[tauri::command]
pub async fn remote_register(
    profile: RemoteProfile,
    secret: UnlockSecretDto,
    vault_folder: Option<String>,
    state: State<'_, AppState>,
) -> ApiResult<LoginInfo> {
    let secret = UnlockSecretInput::try_from(secret)?;
    register(&state, &profile, &secret, vault_folder.map(PathBuf::from)).await
}

#[tauri::command]
pub async fn remote_login(
    profile: RemoteProfile,
    secret: UnlockSecretDto,
    state: State<'_, AppState>,
) -> ApiResult<LoginInfo> {
    let secret = UnlockSecretInput::try_from(secret)?;
    login(&state, &profile, &secret).await
}

#[tauri::command]
pub async fn remote_logout(account_id: String, state: State<'_, AppState>) -> ApiResult<bool> {
    let account = parse_account(&account_id)?;
    logout(&state, account)
}

#[tauri::command]
pub async fn remote_refresh(
    account_id: String,
    state: State<'_, AppState>,
) -> ApiResult<LoginInfo> {
    refresh(&state, parse_account(&account_id)?).await
}

#[tauri::command]
pub async fn remote_status(state: State<'_, AppState>) -> ApiResult<Vec<RemoteStatus>> {
    Ok(status(&state))
}

#[tauri::command]
pub async fn remote_vault_meta(
    account_id: String,
    state: State<'_, AppState>,
) -> ApiResult<Option<VaultMeta>> {
    vault_meta(&state, parse_account(&account_id)?).await
}

#[tauri::command]
pub async fn remote_pull_vault(
    account_id: String,
    folder: String,
    state: State<'_, AppState>,
) -> ApiResult<VaultMeta> {
    pull_vault(&state, parse_account(&account_id)?, Path::new(&folder)).await
}

#[tauri::command]
pub async fn remote_push_vault(
    account_id: String,
    folder: String,
    expected_version: Option<u64>,
    state: State<'_, AppState>,
) -> ApiResult<VaultMeta> {
    push_vault(
        &state,
        parse_account(&account_id)?,
        Path::new(&folder),
        expected_version,
    )
    .await
}

#[tauri::command]
pub async fn remote_usage(account_id: String, state: State<'_, AppState>) -> ApiResult<Usage> {
    usage(&state, parse_account(&account_id)?).await
}

#[tauri::command]
pub async fn remote_list_blobs(
    account_id: String,
    state: State<'_, AppState>,
) -> ApiResult<Vec<BlobInfo>> {
    list_blobs(&state, parse_account(&account_id)?).await
}

#[tauri::command]
pub async fn remote_upload_blob(
    account_id: String,
    blob_id: String,
    path: String,
    state: State<'_, AppState>,
) -> ApiResult<BlobInfo> {
    upload_blob(
        &state,
        parse_account(&account_id)?,
        blob_id,
        PathBuf::from(path),
    )
    .await
}

#[tauri::command]
pub async fn remote_download_blob(
    account_id: String,
    blob_id: String,
    dest: String,
    state: State<'_, AppState>,
) -> ApiResult<BlobInfo> {
    download_blob(
        &state,
        parse_account(&account_id)?,
        blob_id,
        PathBuf::from(dest),
    )
    .await
}

#[tauri::command]
pub async fn remote_delete_blob(
    account_id: String,
    blob_id: String,
    state: State<'_, AppState>,
) -> ApiResult<bool> {
    delete_blob(&state, parse_account(&account_id)?, blob_id).await
}

#[tauri::command]
pub async fn remote_delete_account(
    account_id: String,
    secret: UnlockSecretDto,
    state: State<'_, AppState>,
) -> ApiResult<()> {
    let secret = UnlockSecretInput::try_from(secret)?;
    delete_account(&state, parse_account(&account_id)?, &secret).await
}

fn parse_account(value: &str) -> ApiResult<AccountId> {
    AccountId::from_hex(value)
        .ok_or_else(|| ApiError::new("bad_account_id", "account_id должен быть 32 hex-символа"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pms_server::config::Config;

    struct Fixture {
        server: pms_server::ServerHandle,
        base_url: String,
        profile: RemoteProfile,
        state: AppState,
        dir: tempfile::TempDir,
    }

    async fn fixture(extra: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        crate::commands::set_data_dir(dir.path().join("data"));

        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
        let cert = dir.path().join("server.crt");
        let key = dir.path().join("server.key");
        std::fs::write(&cert, certified.cert.pem()).unwrap();
        std::fs::write(&key, certified.key_pair.serialize_pem()).unwrap();

        let text = format!(
            "[server]\ndata_dir = \"{}\"\nlisten = \"127.0.0.1:0\"\n\
             [tls]\nrequired = true\nmode = \"files\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n{extra}",
            dir.path().join("server-data").display(),
            cert.display(),
            key.display()
        );
        let server = pms_server::run(Config::parse(&text).unwrap())
            .await
            .unwrap();
        let base_url = format!("https://{}", server.addr());
        let pin = crate::core::remote::tls::fetch_spki_sha256(&base_url)
            .await
            .unwrap();
        let profile = RemoteProfile {
            base_url: base_url.clone(),
            tls: crate::core::remote::TlsPolicy::Pin { spki_sha256: pin },
        };

        let state = AppState::default();
        state.fs_scope.attach(None, vec![dir.path().to_path_buf()]);

        Fixture {
            server,
            base_url,
            profile,
            state,
            dir,
        }
    }

    fn passphrase(value: &str) -> UnlockSecretInput {
        UnlockSecretInput::Passphrase {
            value: value.to_string(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn register_push_and_pull_through_the_command_layer() {
        let fixture = fixture("").await;
        let secret = passphrase("мастер-ключ");

        let info = register(&fixture.state, &fixture.profile, &secret, None)
            .await
            .unwrap();
        assert_eq!(info.account_id.len(), 32);
        assert!(info.vault.is_none());

        let folder = fixture.dir.path().join("vault");
        std::fs::create_dir_all(&folder).unwrap();
        let body = b"sealed vault".to_vec();
        std::fs::write(folder.join("pms.vault"), &body).unwrap();

        let meta = push_vault(
            &fixture.state,
            AccountId::from_hex(&info.account_id).unwrap(),
            &folder,
            None,
        )
        .await
        .unwrap();
        assert_eq!(meta.version, 1);

        let pulled = fixture.dir.path().join("pulled");
        std::fs::create_dir_all(&pulled).unwrap();
        pull_vault(
            &fixture.state,
            AccountId::from_hex(&info.account_id).unwrap(),
            &pulled,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(pulled.join("pms.vault")).unwrap(), body);

        let sessions = status(&fixture.state);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].base_url, fixture.base_url);

        let account = AccountId::from_hex(&info.account_id).unwrap();
        assert!(logout(&fixture.state, account).unwrap());
        assert_eq!(status(&fixture.state).len(), 0);
        assert!(!logout(&fixture.state, account).unwrap());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn register_uploads_an_existing_local_vault() {
        let fixture = fixture("").await;
        let folder = fixture.dir.path().join("local");
        std::fs::create_dir_all(&folder).unwrap();
        let body = b"existing sealed vault".to_vec();
        std::fs::write(folder.join("pms.vault"), &body).unwrap();

        let info = register(
            &fixture.state,
            &fixture.profile,
            &passphrase("мастер-ключ"),
            Some(folder),
        )
        .await
        .unwrap();

        let meta = info.vault.expect("метаданные сейфа");
        assert_eq!(meta.version, 1);
        assert_eq!(meta.size, body.len() as u64);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn refresh_extends_the_session_without_the_master_key() {
        let fixture = fixture("").await;
        let info = register(
            &fixture.state,
            &fixture.profile,
            &passphrase("мастер-ключ"),
            None,
        )
        .await
        .unwrap();
        let account = AccountId::from_hex(&info.account_id).unwrap();

        let refreshed = refresh(&fixture.state, account).await.unwrap();
        assert_eq!(refreshed.account_id, info.account_id);
        // Токен после продления работает.
        assert!(vault_meta(&fixture.state, account).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn login_without_registration_reports_not_found() {
        let fixture = fixture("").await;
        let error = login(&fixture.state, &fixture.profile, &passphrase("никого нет"))
            .await
            .unwrap_err();
        assert_eq!(error.code, "remote_not_found");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commands_without_a_session_are_unauthorized() {
        let fixture = fixture("").await;
        let error = vault_meta(&fixture.state, AccountId::new([9u8; 16]))
            .await
            .unwrap_err();
        assert_eq!(error.code, "remote_unauthorized");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unreachable_server_is_reported_as_unreachable() {
        let profile = RemoteProfile {
            base_url: "https://127.0.0.1:1".to_string(),
            tls: crate::core::remote::TlsPolicy::Insecure,
        };
        let error = probe(&profile).await.unwrap_err();
        assert_eq!(error.code, "remote_unreachable");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn paths_outside_the_grant_are_refused() {
        let fixture = fixture("").await;
        let info = register(
            &fixture.state,
            &fixture.profile,
            &passphrase("мастер-ключ"),
            None,
        )
        .await
        .unwrap();

        let outside = std::env::temp_dir().join("pms-not-granted");
        let error = pull_vault(
            &fixture.state,
            AccountId::from_hex(&info.account_id).unwrap(),
            &outside,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "fs_scope");
        let _ = fixture.server.addr();
    }
}
