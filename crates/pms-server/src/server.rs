//! Сборка и запуск сервера: TLS-режим, листенер, graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;

use crate::api::{router, AppState};
use crate::auth::Auth;
use crate::config::{Config, ConfigError};
use crate::store::Store;
use crate::tls::{self, TlsMode};

/// Запущенный сервер. `shutdown` останавливает его без обрыва текущих запросов.
pub struct ServerHandle {
    addr: SocketAddr,
    quota_default: u64,
    handle: Handle<SocketAddr>,
    join: tokio::task::JoinHandle<()>,
}

impl ServerHandle {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn quota_default(&self) -> u64 {
        self.quota_default
    }

    pub fn shutdown(&self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(2)));
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.handle.graceful_shutdown(Some(Duration::ZERO));
        self.join.abort();
    }
}

pub async fn run(config: Config) -> Result<ServerHandle, ServerError> {
    config.validate()?;
    let tls_mode = tls::resolve(&config)?;

    let store = Arc::new(Store::open(
        &config.server.data_dir,
        config.quota.default_bytes,
    )?);
    store.set_fsync(config.limits.fsync);
    let flusher = store.spawn_flusher();
    tracing::info!(policy = ?config.limits.fsync, "политика fsync");
    let quota_default = store.default_quota();
    let auth = Arc::new(Auth::new(Arc::clone(&store), config.auth.clone()));
    let state = AppState::new(store, auth, Arc::new(config.clone()));
    let app = router(state);

    let address: SocketAddr = config
        .server
        .listen
        .parse()
        .map_err(|_| ServerError::BadListen(config.server.listen.clone()))?;
    let listener = std::net::TcpListener::bind(address).map_err(ServerError::Bind)?;
    listener.set_nonblocking(true).map_err(ServerError::Bind)?;
    let addr = listener.local_addr().map_err(ServerError::Bind)?;

    // Дедлайны соединения: без них клиент, приславший половину заголовков,
    // держит слот и файловый дескриптор бесконечно (slowloris).
    let header_timeout = Duration::from_secs(config.limits.header_read_timeout_secs);

    let handle: Handle<SocketAddr> = Handle::new();
    let join = match tls_mode {
        TlsMode::Off => {
            tracing::warn!(
                listen = %addr,
                "TLS выключен: трафик идёт открытым текстом"
            );
            let mut server = axum_server::from_tcp(listener)
                .map_err(ServerError::Bind)?
                .handle(handle.clone());
            server
                .http_builder()
                .http1()
                .header_read_timeout(header_timeout);
            let server = server.first_request_timeout(header_timeout);
            tokio::spawn(async move {
                if let Err(error) = server.serve(app.into_make_service()).await {
                    tracing::error!(error = %error, "сервер остановлен с ошибкой");
                }
            })
        }
        TlsMode::Files { cert, key } => {
            let rustls_config = build_rustls(&cert, &key, &config.tls.min_version).await?;
            tracing::info!(listen = %addr, cert = %cert.display(), "TLS включён");
            let mut server = axum_server::from_tcp_rustls(listener, rustls_config)
                .map_err(ServerError::Bind)?
                .handle(handle.clone());
            server
                .http_builder()
                .http1()
                .header_read_timeout(header_timeout);
            let server = server.first_request_timeout(header_timeout);
            tokio::spawn(async move {
                if let Err(error) = server.serve(app.into_make_service()).await {
                    tracing::error!(error = %error, "сервер остановлен с ошибкой");
                }
            })
        }
    };

    // Флашер живёт столько же, сколько сервер.
    tokio::spawn(async move {
        let _ = flusher.await;
    });

    Ok(ServerHandle {
        addr,
        quota_default,
        handle,
        join,
    })
}

async fn build_rustls(
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
    min_version: &str,
) -> Result<RustlsConfig, ServerError> {
    let cert_pem = tokio::fs::read(cert_path)
        .await
        .map_err(|error| ServerError::Tls(error.to_string()))?;
    let key_pem = tokio::fs::read(key_path)
        .await
        .map_err(|error| ServerError::Tls(error.to_string()))?;

    let versions = versions_for(min_version)?;
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

    let mut certs = Vec::new();
    for item in rustls_pemfile::certs(&mut std::io::Cursor::new(cert_pem.as_slice())) {
        certs.push(item.map_err(|error| ServerError::Tls(error.to_string()))?);
    }
    let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_pem.as_slice()))
        .map_err(|error| ServerError::Tls(error.to_string()))?
        .ok_or_else(|| ServerError::Tls("в файле ключа нет PEM-ключа".to_string()))?;

    let mut server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_protocol_versions(versions)
    .map_err(|error| ServerError::Tls(error.to_string()))?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|error| ServerError::Tls(error.to_string()))?;

    // Тикеты сессии: телефон после паузы возобновляет соединение без полного
    // рукопожатия с сертификатом RSA-3072.
    match rustls::crypto::aws_lc_rs::Ticketer::new() {
        Ok(ticketer) => server_config.ticketer = ticketer,
        Err(error) => tracing::warn!(%error, "TLS-тикеты недоступны, рукопожатие каждый раз"),
    }

    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

static TLS13_ONLY: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13];

fn versions_for(
    min_version: &str,
) -> Result<&'static [&'static rustls::SupportedProtocolVersion], ServerError> {
    match min_version {
        "1.2" => Ok(rustls::ALL_VERSIONS),
        "1.3" => Ok(TLS13_ONLY),
        other => Err(ServerError::Tls(format!(
            "min_version = \"{other}\": поддерживаются 1.2 и 1.3"
        ))),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("конфиг: {0}")]
    Config(#[from] ConfigError),
    #[error("хранилище: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("адрес {0} не разбирается")]
    BadListen(String),
    #[error("не удалось занять порт: {0}")]
    Bind(std::io::Error),
    #[error("tls: {0}")]
    Tls(String),
}
