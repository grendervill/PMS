use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use pms_server::config::Config;
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::CheckConfig { config } => {
            let config = match Config::load(&config) {
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
        Command::Run { config } => {
            let _ = config;
            eprintln!("запуск сервера появится вместе с HTTP-слоем");
            ExitCode::FAILURE
        }
    }
}
