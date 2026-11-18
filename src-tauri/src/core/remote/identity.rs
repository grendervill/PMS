//! Мастер-ключ → личность аккаунта, и стабильный идентификатор устройства.
//!
//! Identity-ключ выводится детерминированно (см. `pms-proto`), поэтому второе
//! устройство получает тот же аккаунт, введя ту же пароль-фразу или ключ-файл.

use std::path::{Path, PathBuf};

use pms_proto::identity::{derive_identity_checked, Identity, IdentityKdf};
use zeroize::Zeroizing;

use crate::core::crypto::{canonical_secret, UnlockSecretInput};

use super::RemoteError;

pub const DEVICE_FILE: &str = "remote-device.json";

/// Выводит ключ аккаунта из того же секрета, что открывает сейф.
pub fn identity_from_secret(secret: &UnlockSecretInput) -> Result<Identity, RemoteError> {
    let canonical = canonical_secret(secret)
        .map_err(|error| RemoteError::BadRequest(format!("мастер-ключ не принят: {error}")))?;
    derive_identity_checked(&canonical, &IdentityKdf::V1)
        .map_err(|error| RemoteError::BadRequest(error.to_string()))
}

/// Читает или создаёт файл с идентификатором устройства.
///
/// Идентификатор не секрет, но он привязан к подписи и к сессии: без него
/// перехваченный токен можно было бы перевесить на другое устройство.
pub fn device_id_at(directory: &Path) -> Result<[u8; 16], RemoteError> {
    std::fs::create_dir_all(directory).map_err(|error| RemoteError::Storage(error.to_string()))?;
    let path: PathBuf = directory.join(DEVICE_FILE);

    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(bytes) = hex::decode(text.trim()) {
            if let Ok(id) = <[u8; 16]>::try_from(bytes) {
                return Ok(id);
            }
        }
    }

    let mut id = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);
    std::fs::write(&path, hex::encode(id))
        .map_err(|error| RemoteError::Storage(error.to_string()))?;
    Ok(id)
}

/// Тот же идентификатор, но в каталоге приложения.
pub fn device_id() -> Result<[u8; 16], RemoteError> {
    let directory = crate::commands::app_data_dir()
        .ok_or_else(|| RemoteError::Storage("каталог приложения ещё не известен".to_string()))?;
    device_id_at(&directory)
}

/// Секрет, из которого выводится identity-ключ, живёт только на время входа.
pub fn wipe(mut secret: Zeroizing<Vec<u8>>) {
    use zeroize::Zeroize;
    secret.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_stable_and_survives_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = device_id_at(dir.path()).unwrap();
        let second = device_id_at(dir.path()).unwrap();
        assert_eq!(first, second);

        std::fs::write(dir.path().join(DEVICE_FILE), "не hex").unwrap();
        let recovered = device_id_at(dir.path()).unwrap();
        assert_ne!(recovered, [0u8; 16]);
        assert_eq!(device_id_at(dir.path()).unwrap(), recovered);
    }

    #[test]
    fn identity_is_derived_from_the_master_secret() {
        let secret = UnlockSecretInput::Passphrase {
            value: "correct horse battery".to_string(),
        };
        let first = identity_from_secret(&secret).unwrap();
        let second = identity_from_secret(&UnlockSecretInput::Passphrase {
            value: "correct horse battery".to_string(),
        })
        .unwrap();
        let other = identity_from_secret(&UnlockSecretInput::Passphrase {
            value: "another phrase".to_string(),
        })
        .unwrap();

        assert_eq!(first.public, second.public);
        assert_ne!(first.public, other.public);
        assert!(identity_from_secret(&UnlockSecretInput::Passphrase {
            value: String::new()
        })
        .is_err());
    }
}
