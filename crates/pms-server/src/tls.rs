//! Режимы TLS. Встроенного ACME нет: сертификат берётся файлами, автовыпуск —
//! задача reverse proxy или certbot.

use std::io::Cursor;
use std::path::PathBuf;

use crate::config::{Config, ConfigError, TlsModeName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS с указанными файлами сертификатной цепочки и ключа.
    Files { cert: PathBuf, key: PathBuf },
    /// Без TLS. Допустим только при `tls.required = false`.
    Off,
}

/// Разрешает режим TLS и проверяет, что файлы существуют и содержат то, что нужно.
pub fn resolve(config: &Config) -> Result<TlsMode, ConfigError> {
    config.validate()?;

    match config.tls.mode {
        TlsModeName::Off => {
            if config.tls.required {
                return Err(ConfigError::TlsRequired);
            }
            Ok(TlsMode::Off)
        }
        TlsModeName::Files => {
            let (Some(cert), Some(key)) = (&config.tls.cert_path, &config.tls.key_path) else {
                return Err(if config.tls.required {
                    ConfigError::TlsRequired
                } else {
                    ConfigError::MissingCertificate(
                        "cert_path/key_path не заданы в [tls]".to_string(),
                    )
                });
            };

            verify_certificate_chain(cert)?;
            verify_private_key(key)?;
            Ok(TlsMode::Files {
                cert: cert.clone(),
                key: key.clone(),
            })
        }
    }
}

fn read(path: &std::path::Path) -> Result<Vec<u8>, ConfigError> {
    std::fs::read(path).map_err(|_| {
        ConfigError::MissingCertificate(format!("{} недоступен для чтения", path.display()))
    })
}

fn verify_certificate_chain(path: &std::path::Path) -> Result<(), ConfigError> {
    let bytes = read(path)?;
    let mut cursor = Cursor::new(bytes.as_slice());
    let mut found = false;
    for item in rustls_pemfile::certs(&mut cursor) {
        if item.is_err() {
            return Err(ConfigError::MissingCertificate(format!(
                "{} не похож на PEM-сертификат",
                path.display()
            )));
        }
        found = true;
    }
    if !found {
        return Err(ConfigError::MissingCertificate(format!(
            "в {} нет ни одного сертификата",
            path.display()
        )));
    }
    Ok(())
}

fn verify_private_key(path: &std::path::Path) -> Result<(), ConfigError> {
    let bytes = read(path)?;
    let mut cursor = Cursor::new(bytes.as_slice());
    match rustls_pemfile::private_key(&mut cursor) {
        Ok(Some(_)) => Ok(()),
        _ => Err(ConfigError::MissingCertificate(format!(
            "в {} нет PEM-ключа",
            path.display()
        ))),
    }
}
