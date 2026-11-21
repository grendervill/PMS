//! Конфигурация сервера: TOML, строгий разбор, безопасные значения по умолчанию.
//!
//! `deny_unknown_fields` намеренно: опечатка в ключе не должна молча оставлять
//! сервер с дефолтом вместо того, что имел в виду администратор.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Квота на аккаунт по умолчанию: 1 ГиБ.
pub const DEFAULT_QUOTA_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: Server,
    #[serde(default)]
    pub tls: Tls,
    #[serde(default)]
    pub quota: Quota,
    #[serde(default)]
    pub registration: Registration,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub data_dir: PathBuf,
    #[serde(default = "default_listen")]
    pub listen: String,
}

fn default_listen() -> String {
    "127.0.0.1:8443".to_string()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsModeName {
    #[default]
    Files,
    Off,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tls {
    /// true по умолчанию: сервер не стартует без пригодного сертификата.
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub mode: TlsModeName,
    #[serde(default)]
    pub cert_path: Option<PathBuf>,
    #[serde(default)]
    pub key_path: Option<PathBuf>,
    #[serde(default = "default_tls_version")]
    pub min_version: String,
}

fn default_true() -> bool {
    true
}

fn default_tls_version() -> String {
    "1.3".to_string()
}

impl Default for Tls {
    fn default() -> Self {
        Self {
            required: true,
            mode: TlsModeName::Files,
            cert_path: None,
            key_path: None,
            min_version: default_tls_version(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quota {
    #[serde(default = "default_quota")]
    pub default_bytes: u64,
}

fn default_quota() -> u64 {
    DEFAULT_QUOTA_BYTES
}

impl Default for Quota {
    fn default() -> Self {
        Self {
            default_bytes: DEFAULT_QUOTA_BYTES,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Потолок регистраций в минуту на весь сервер (0 — без ограничения).
    #[serde(default = "default_registration_rate")]
    pub max_per_minute: u32,
    /// Непустой токен закрывает регистрацию от посторонних.
    #[serde(default)]
    pub token: String,
}

fn default_registration_rate() -> u32 {
    30
}

impl Default for Registration {
    fn default() -> Self {
        Self {
            enabled: true,
            max_per_minute: default_registration_rate(),
            token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    #[serde(default = "default_token_ttl_hours")]
    pub token_ttl_hours: u32,
    #[serde(default = "default_max_sessions")]
    pub max_sessions_per_account: u32,
    #[serde(default = "default_challenge_ttl")]
    pub challenge_ttl_seconds: u64,
}

fn default_token_ttl_hours() -> u32 {
    12
}
fn default_max_sessions() -> u32 {
    // Два-три устройства у одного человека; запас нужен, чтобы переустановка
    // приложения или отладка не вытесняла телефон с сервера.
    10
}
fn default_challenge_ttl() -> u64 {
    60
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            token_ttl_hours: default_token_ttl_hours(),
            max_sessions_per_account: default_max_sessions(),
            challenge_ttl_seconds: default_challenge_ttl(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Limits {
    #[serde(default = "default_max_body")]
    pub max_body_bytes: u64,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_header_timeout")]
    pub header_read_timeout_secs: u64,
    #[serde(default = "default_body_timeout")]
    pub body_timeout_secs: u64,
}

fn default_max_body() -> u64 {
    DEFAULT_QUOTA_BYTES
}
fn default_max_connections() -> usize {
    512
}
fn default_header_timeout() -> u64 {
    10
}
fn default_body_timeout() -> u64 {
    300
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: default_max_body(),
            max_connections: default_max_connections(),
            header_read_timeout_secs: default_header_timeout(),
            body_timeout_secs: default_body_timeout(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text =
            std::fs::read_to_string(path).map_err(|error| ConfigError::Io(error.to_string()))?;
        let config = Self::parse(&text)?;
        config.validate()?;
        Ok(config)
    }

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|error| ConfigError::Parse(error.to_string()))
    }

    /// Проверки, не зависящие от файловой системы. Существование сертификатов
    /// проверяет [`crate::tls::resolve`].
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.quota.default_bytes == 0 {
            return Err(ConfigError::BadQuota);
        }
        if self.limits.max_body_bytes < 64 * 1024
            || self.limits.max_connections == 0
            || self.limits.header_read_timeout_secs == 0
            || self.limits.body_timeout_secs == 0
        {
            return Err(ConfigError::BadLimit);
        }
        if !matches!(self.tls.min_version.as_str(), "1.2" | "1.3") {
            return Err(ConfigError::BadTlsVersion(self.tls.min_version.clone()));
        }
        if self.auth.token_ttl_hours == 0
            || self.auth.max_sessions_per_account == 0
            || self.auth.challenge_ttl_seconds == 0
        {
            return Err(ConfigError::BadAuth);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("не удалось прочитать конфиг: {0}")]
    Io(String),
    #[error("не удалось разобрать конфиг: {0}")]
    Parse(String),
    #[error("tls.required = true, но пригодного сертификата нет")]
    TlsRequired,
    #[error("файл сертификата или ключа не найден: {0}")]
    MissingCertificate(String),
    #[error("квота должна быть больше нуля")]
    BadQuota,
    #[error(
        "лимиты вне допустимых значений (max_body_bytes не меньше 64 КиБ, остальные больше нуля)"
    )]
    BadLimit,
    #[error("параметры аутентификации должны быть больше нуля")]
    BadAuth,
    #[error("tls.min_version = \"{0}\": поддерживаются 1.2 и 1.3")]
    BadTlsVersion(String),
}
