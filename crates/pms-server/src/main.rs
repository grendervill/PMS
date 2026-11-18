use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pms_proto::identity::AccountId;
use tracing_subscriber::EnvFilter;

use pms_server::config::Config;
use pms_server::store::Store;
use pms_server::tls;

#[derive(Debug, Parser)]
#[command(name = "pms-server", version, about = "Сервер синхронизации PMS")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Запустить сервер с указанным конфигом.
    Run {
        #[arg(long, value_name = "PATH")]
        config: PathBuf,
    },
    /// Разобрать конфиг, проверить TLS и выйти (для проверки перед рестартом).
    CheckConfig {
        #[arg(long, value_name = "PATH")]
        config: PathBuf,
    },
    /// Изменить квоту аккаунта.
    QuotaSet {
        #[arg(long, value_name = "PATH")]
        config: PathBuf,
        /// account_id (32 hex).
        #[arg(long)]
        account: String,
        #[arg(long)]
        bytes: u64,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    match Cli::parse().command {
        Command::CheckConfig { config } => check_config(&config),
        Command::Run { config } => run_server(&config).await,
        Command::QuotaSet {
            config,
            account,
            bytes,
        } => quota_set(&config, &account, bytes),
    }
}

fn check_config(path: &std::path::Path) -> ExitCode {
    let config = match Config::load(path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("конфиг не принят: {error}");
            return ExitCode::FAILURE;
        }
    };
    match tls::resolve(&config) {
        Ok(tls::TlsMode::Files { cert, key }) => {
            println!("tls: files ({} / {})", cert.display(), key.display());
        }
        Ok(tls::TlsMode::Off) => {
            println!("tls: off (незашифрованный режим, tls.required = false)");
        }
        Err(error) => {
            eprintln!("tls не готов: {error}");
            return ExitCode::FAILURE;
        }
    }
    println!("listen: {}", config.server.listen);
    println!("data_dir: {}", config.server.data_dir.display());
    println!("quota: {} байт", config.quota.default_bytes);
    ExitCode::SUCCESS
}

async fn run_server(path: &std::path::Path) -> ExitCode {
    let config = match Config::load(path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("конфиг не принят: {error}");
            return ExitCode::FAILURE;
        }
    };

    let server = match pms_server::run(config).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("сервер не запущен: {error}");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(addr = %server.addr(), "pms-server слушает");
    let _ = tokio::signal::ctrl_c().await;
    server.shutdown();
    tracing::info!("остановка по сигналу");
    ExitCode::SUCCESS
}

fn quota_set(path: &std::path::Path, account: &str, bytes: u64) -> ExitCode {
    let config = match Config::load(path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("конфиг не принят: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(account) = AccountId::from_hex(account) else {
        eprintln!("account_id должен быть 32 hex-символа");
        return ExitCode::FAILURE;
    };
    let store = match Store::open(&config.server.data_dir, config.quota.default_bytes) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("хранилище не открылось: {error}");
            return ExitCode::FAILURE;
        }
    };
    match store.set_quota(account, bytes) {
        Ok(true) => {
            println!("квота аккаунта {account} = {bytes} байт");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("аккаунт {account} не найден");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("не удалось обновить квоту: {error}");
            ExitCode::FAILURE
        }
    }
}
