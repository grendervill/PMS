//! Аутентификация: одноразовый nonce, подпись Ed25519, сессии и торможение
//! перебора. Состояние — только в индексе плюс счётчики неудач в памяти.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pms_proto::identity::{verify_challenge, AccountId, DeviceId};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::config::Auth as AuthConfig;
use crate::store::state::NonceStatus;
use crate::store::{Store, StoreError};

/// Сколько неудач подряд допускается до торможения.
const FAILURES_BEFORE_BACKOFF: u32 = 5;
const MAX_BACKOFF_SECONDS: u64 = 60;

pub struct Auth {
    store: Arc<Store>,
    config: AuthConfig,
    limiter: RateLimiter,
    /// Когда последний раз чистили просроченные сессии (unix seconds).
    last_prune: std::sync::atomic::AtomicI64,
}

/// Выданный nonce. Живёт до первого использования.
#[derive(Clone, Copy)]
pub struct Challenge {
    pub nonce: [u8; 32],
    pub expires_at: i64,
}

impl std::fmt::Debug for Challenge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Challenge {{ expires_at: {} }}", self.expires_at)
    }
}

/// Токен сессии. `expose()` — единственный способ получить hex для клиента:
/// так случайная запись токена в лог становится заметной в диффе.
pub struct SessionToken(String);

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Токен не должен появляться в логах и сообщениях об ошибках.
        formatter.write_str("SessionToken(<скрыт>)")
    }
}

impl SessionToken {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SessionToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authenticated {
    pub account: AccountId,
    pub device: DeviceId,
}

impl Auth {
    pub fn new(store: Arc<Store>, config: AuthConfig) -> Self {
        Self {
            store,
            config,
            limiter: RateLimiter::default(),
            last_prune: std::sync::atomic::AtomicI64::new(0),
        }
    }

    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    pub fn issue_challenge(
        &self,
        account: AccountId,
        device: DeviceId,
    ) -> Result<Challenge, AuthError> {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let expires_at = unix_now() + self.config.challenge_ttl_seconds as i64;
        self.store.insert_nonce(account, &nonce, expires_at)?;
        // Nonce привязан к аккаунту; устройство попадает в подпись, поэтому
        // переиграть чужой nonce на своё устройство всё равно нельзя.
        let _ = device;
        Ok(Challenge { nonce, expires_at })
    }

    /// Проверяет подпись и выдаёт токен. Nonce забирается из индекса до проверки,
    /// поэтому повторное использование невозможно независимо от исхода.
    pub fn verify(
        &self,
        account: AccountId,
        device: DeviceId,
        nonce: &[u8; 32],
        signature: &[u8; 64],
        public_key: &[u8; 32],
    ) -> Result<SessionToken, AuthError> {
        if nonce.iter().all(|byte| *byte == 0) {
            return Err(AuthError::BadNonce);
        }

        match self.store.take_nonce(account, nonce)? {
            NonceStatus::Valid => {}
            // Просроченный nonce — не попытка подбора: у пользователя мог
            // уехать телефон или он просто думал дольше минуты.
            NonceStatus::Expired => return Err(AuthError::Unauthorized),
            NonceStatus::Unknown => {
                self.limiter.record_failure(&account.to_string());
                return Err(AuthError::Unauthorized);
            }
        }

        if !verify_challenge(public_key, &account, &device, nonce, signature) {
            self.limiter.record_failure(&account.to_string());
            return Err(AuthError::Unauthorized);
        }

        let token = random_hex(32);
        let hash = hash_token(&token);
        let expires_at = unix_now() + i64::from(self.config.token_ttl_hours) * 3600;
        self.store.insert_session(
            account,
            device,
            &hash,
            expires_at,
            self.config.max_sessions_per_account,
        )?;
        Ok(SessionToken(token))
    }

    pub fn authenticate(&self, token: &str) -> Result<Authenticated, AuthError> {
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AuthError::Unauthorized);
        }
        let hash = hash_token(token);
        let session = self.store.session(&hash)?.ok_or(AuthError::Unauthorized)?;
        if session.expires_at <= unix_now() {
            self.store.delete_session(&hash)?;
            return Err(AuthError::Unauthorized);
        }
        self.prune_expired();

        Ok(Authenticated {
            account: session.account,
            device: session.device,
        })
    }

    /// Раз в минуту убираем просроченные сессии. Раньше это делалось на каждом
    /// входе полным сканом таблицы — с ростом числа сессий вход деградировал.
    fn prune_expired(&self) {
        use std::sync::atomic::Ordering;
        let now = unix_now();
        let last = self.last_prune.load(Ordering::Relaxed);
        if now - last < 60 {
            return;
        }
        if self
            .last_prune
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        if let Err(error) = self.store.delete_expired_sessions(now) {
            tracing::warn!(%error, "не удалось почистить просроченные сессии");
        }
    }

    /// Отзывает ровно этот токен (кнопка «Отключиться» на устройстве).
    pub fn revoke(&self, token: &str) -> Result<bool, AuthError> {
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(false);
        }
        let hash = hash_token(token);
        let existed = self.store.session(&hash)?.is_some();
        self.store.delete_session(&hash)?;
        Ok(existed)
    }

    pub fn revoke_all(&self, account: AccountId) -> Result<(), AuthError> {
        self.store.delete_sessions(account)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct RateLimiter {
    state: Mutex<HashMap<String, FailureState>>,
}

#[derive(Default)]
struct FailureState {
    failures: u32,
    blocked_until: i64,
}

impl RateLimiter {
    pub fn check(&self, key: &str) -> Result<(), AuthError> {
        let mut state = self.state.lock().expect("rate limiter mutex poisoned");
        let Some(entry) = state.get_mut(key) else {
            return Ok(());
        };
        let now = unix_now();
        if entry.blocked_until > now {
            return Err(AuthError::RateLimited {
                retry_after_seconds: (entry.blocked_until - now) as u64,
            });
        }
        if entry.blocked_until != 0 {
            *entry = FailureState::default();
        }
        Ok(())
    }

    pub fn record_failure(&self, key: &str) {
        let mut state = self.state.lock().expect("rate limiter mutex poisoned");
        let entry = state.entry(key.to_string()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        if entry.failures >= FAILURES_BEFORE_BACKOFF {
            let over = entry.failures - FAILURES_BEFORE_BACKOFF;
            let delay = 5u64
                .saturating_mul(1u64 << over.min(4))
                .min(MAX_BACKOFF_SECONDS);
            entry.blocked_until = unix_now() + delay as i64;
        }
    }

    pub fn forget(&self, key: &str) {
        let mut state = self.state.lock().expect("rate limiter mutex poisoned");
        state.remove(key);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("nonce пустой или испорчен")]
    BadNonce,
    #[error("подпись не подходит")]
    Unauthorized,
    #[error("слишком много попыток, повторите через {retry_after_seconds} с")]
    RateLimited { retry_after_seconds: u64 },
    #[error("ошибка хранилища: {0}")]
    Store(String),
}

impl From<StoreError> for AuthError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Db(message) => Self::Store(message),
            other => Self::Store(other.to_string()),
        }
    }
}

pub(crate) fn hash_token(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

pub(crate) fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub(crate) fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}
