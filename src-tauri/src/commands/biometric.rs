use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::commands::vault::UnlockVaultResponse;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::vault::unlock_vault_with_key;

#[cfg(target_os = "macos")]
const SERVICE: &str = "dev.pms.vault.biometric";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricStatus {
    pub available: bool,
}

#[tauri::command]
pub async fn biometric_status(app: AppHandle) -> ApiResult<BiometricStatus> {
    let _ = &app;
    #[cfg(target_os = "macos")]
    let available = imp::available();
    #[cfg(target_os = "android")]
    let available = android::available(&app);
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    let available = false;

    Ok(BiometricStatus { available })
}

#[tauri::command]
pub async fn biometric_enroll(
    session_id: Uuid,
    app: AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<()> {
    let _ = &app;
    let (vault_id, key) = state.sessions.with(session_id, |session| {
        Ok((session.vault_id(), *session.vault_key()))
    })?;

    #[cfg(target_os = "macos")]
    {
        imp::enroll(vault_id, &key)
    }
    #[cfg(target_os = "android")]
    {
        android::enroll(&app, vault_id, &key)
    }
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    {
        let _ = (app, vault_id, key);
        Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия на этом устройстве недоступна",
        ))
    }
}

#[tauri::command]
pub async fn biometric_disable(vault_id: Uuid, app: AppHandle) -> ApiResult<()> {
    let _ = &app;
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        imp::disable(vault_id)
    }
    #[cfg(target_os = "android")]
    {
        android::disable(&app, vault_id)
    }
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    {
        let _ = (app, vault_id);
        Ok(())
    }
}

#[tauri::command]
pub async fn biometric_unlock(
    file_b64: String,
    vault_id: Uuid,
    app: AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<UnlockVaultResponse> {
    let _ = &app;
    #[cfg(target_os = "macos")]
    if !imp::available() {
        return Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия на этом устройстве недоступна",
        ));
    }
    #[cfg(target_os = "android")]
    if !android::available(&app) {
        return Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия на этом устройстве недоступна",
        ));
    }

    let file = base64::engine::general_purpose::STANDARD
        .decode(file_b64)
        .map_err(|_| ApiError::bad_base64())?;

    #[cfg(target_os = "macos")]
    let key = {
        imp::unlock(vault_id)?;
        imp::read(vault_id)?
    };
    #[cfg(target_os = "android")]
    let key = android::authenticate_and_read(&app, vault_id)?;
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    let key: [u8; 32] = {
        let _ = (app, vault_id);
        return Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия на этом устройстве недоступна",
        ));
    };

    let (session, data) = unlock_vault_with_key(&file, &key).map_err(ApiError::from)?;
    let session_id = session.id;
    state.sessions.put(session);

    Ok(UnlockVaultResponse { session_id, data })
}

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use security_framework::base::Error as SecurityError;
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password, generic_password, set_generic_password_options,
    };
    use security_framework_sys::base::{errSecAuthFailed as ERR_SEC_AUTH_FAILED, errSecItemNotFound as ERR_SEC_ITEM_NOT_FOUND};
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::SERVICE;
    use crate::commands::{ApiError, ApiResult};

    const ERR_USER_CANCELED: i32 = -2;
    const ERR_USER_FALLBACK: i32 = -3;
    const ERR_BIOMETRY_NOT_AVAILABLE: i32 = -6;
    const ERR_BIOMETRY_NOT_ENROLLED: i32 = -7;
    const ERR_BIOMETRY_LOCKOUT: i32 = -8;

    pub fn available() -> bool {
        let context = unsafe { LAContext::new() };
        unsafe {
            context
                .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
                .is_ok()
        }
    }

    fn map_keychain_error(error: SecurityError) -> ApiError {
        match error.code() {
            ERR_SEC_ITEM_NOT_FOUND => ApiError::new(
                "biometric_key_missing",
                "Сохранённый ключ биометрии не найден",
            ),
            ERR_SEC_AUTH_FAILED => ApiError::new(
                "biometric_failed",
                "Не удалось прочитать сохранённый ключ",
            ),
            _ => ApiError::new("biometric_failed", "Не удалось прочитать сохранённый ключ"),
        }
    }

    fn map_la_error(code: i32) -> ApiError {
        match code {
            ERR_USER_CANCELED | ERR_USER_FALLBACK => {
                ApiError::new("biometric_cancelled", "Вход по отпечатку отменён")
            }
            ERR_BIOMETRY_LOCKOUT => ApiError::new(
                "biometric_lockout",
                "Биометрия временно заблокирована. Введите пароль-фразу",
            ),
            ERR_BIOMETRY_NOT_AVAILABLE | ERR_BIOMETRY_NOT_ENROLLED => ApiError::new(
                "biometric_unavailable",
                "Биометрия на этом устройстве недоступна",
            ),
            _ => ApiError::new("biometric_failed", "Отпечаток не распознан"),
        }
    }

    fn evaluate() -> ApiResult<()> {
        let context = unsafe { LAContext::new() };
        let (sender, receiver) = mpsc::channel::<(bool, i32)>();
        let block = RcBlock::new(move |success: Bool, error: *mut NSError| {
            let code = if error.is_null() {
                0
            } else {
                unsafe { (*error).code() as i32 }
            };
            let _ = sender.send((success.as_bool(), code));
        });
        let reason = NSString::from_str("Разблокировать хранилище PMS");
        unsafe {
            context.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
                &reason,
                &block,
            );
        }
        let (success, code) = receiver
            .recv_timeout(Duration::from_secs(120))
            .map_err(|_| ApiError::new("biometric_failed", "Биометрия не ответила"))?;
        if success {
            Ok(())
        } else {
            Err(map_la_error(code))
        }
    }

    fn store(vault_id: Uuid, key: &[u8; 32]) -> ApiResult<()> {
        let account = vault_id.to_string();
        let options = PasswordOptions::new_generic_password(SERVICE, &account);
        let secret = Zeroizing::new(key.to_vec());
        set_generic_password_options(&secret, options).map_err(map_keychain_error)
    }

    pub fn enroll(vault_id: Uuid, key: &[u8; 32]) -> ApiResult<()> {
        if !available() {
            return Err(ApiError::new(
                "biometric_unavailable",
                "Биометрия на этом устройстве недоступна",
            ));
        }
        evaluate()?;
        store(vault_id, key)
    }

    pub fn unlock(_vault_id: Uuid) -> ApiResult<()> {
        if !available() {
            return Err(ApiError::new(
                "biometric_unavailable",
                "Биометрия на этом устройстве недоступна",
            ));
        }
        evaluate()
    }

    pub fn disable(vault_id: Uuid) -> ApiResult<()> {
        match delete_generic_password(SERVICE, &vault_id.to_string()) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(error) => Err(map_keychain_error(error)),
        }
    }

    pub fn read(vault_id: Uuid) -> ApiResult<[u8; 32]> {
        let value = Zeroizing::new(
            generic_password(PasswordOptions::new_generic_password(
                SERVICE,
                &vault_id.to_string(),
            ))
            .map_err(map_keychain_error)?,
        );
        value.as_slice().try_into().map_err(|_| {
            ApiError::new(
                "biometric_key_missing",
                "Сохранённый ключ биометрии повреждён",
            )
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn maps_cancel_and_lockout_errors() {
            assert_eq!(
                map_la_error(ERR_USER_CANCELED).code,
                "biometric_cancelled"
            );
            assert_eq!(map_la_error(ERR_BIOMETRY_LOCKOUT).code, "biometric_lockout");
            assert_eq!(map_la_error(-999).code, "biometric_failed");
        }

        #[test]
        fn availability_query_is_safe() {
            let _ = available();
        }

        #[test]
        fn stored_key_unlocks_the_vault() {
            use crate::core::crypto::UnlockSecretInput;
            use crate::core::models::CipherSuite;
            use crate::core::vault::{CreateVaultConfig, create_vault, unlock_vault_with_key};

            let (file, session, data) = create_vault(
                &CreateVaultConfig {
                    name: "Bio".to_string(),
                    kdf_profile: crate::core::crypto::KdfProfile::Standard,
                    cipher: CipherSuite::XChaCha20Poly1305,
                },
                &UnlockSecretInput::Passphrase {
                    value: "correct horse".to_string(),
                },
            )
            .unwrap();
            let vault_id = session.vault_id();
            store(vault_id, session.vault_key()).unwrap();
            let key = read(vault_id).unwrap();
            let (_, unlocked) = unlock_vault_with_key(&file, &key).unwrap();
            assert_eq!(unlocked, data);
            disable(vault_id).unwrap();
        }

        #[test]
        fn stores_and_reads_key_without_a_biometric_prompt() {
            let vault_id = Uuid::new_v4();
            let key = [7u8; 32];
            store(vault_id, &key).unwrap();
            assert_eq!(read(vault_id).unwrap(), key);
            disable(vault_id).unwrap();
            assert!(matches!(
                read(vault_id),
                Err(ApiError { code, .. }) if code == "biometric_key_missing"
            ));
        }

        #[test]
        fn disabling_a_missing_item_is_idempotent() {
            assert!(disable(Uuid::new_v4()).is_ok());
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::fs;
    use std::path::PathBuf;

    use base64::Engine as _;
    use tauri::{AppHandle, Manager};
    use tauri_plugin_biometric::{AuthOptions, BiometricExt};
    use uuid::Uuid;

    use crate::commands::{ApiError, ApiResult};

    fn key_path(app: &AppHandle, vault_id: Uuid) -> ApiResult<PathBuf> {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| ApiError::new("biometric_storage", error.to_string()))?
            .join("biometric");
        fs::create_dir_all(&directory)
            .map_err(|error| ApiError::new("biometric_storage", error.to_string()))?;
        Ok(directory.join(format!("{vault_id}.bin")))
    }

    fn auth_options() -> AuthOptions {
        AuthOptions {
            allow_device_credential: true,
            cancel_title: Some("Отмена".to_string()),
            title: Some("Разблокировать PMS".to_string()),
            subtitle: Some("Подтвердите личность".to_string()),
            ..Default::default()
        }
    }

    fn map_error(error: tauri_plugin_biometric::Error) -> ApiError {
        let message = error.to_string().to_lowercase();
        if message.contains("cancel") || message.contains("отмен") {
            ApiError::new("biometric_cancelled", "Вход по отпечатку отменён")
        } else if message.contains("lockout") || message.contains("too many") {
            ApiError::new(
                "biometric_lockout",
                "Биометрия временно заблокирована. Введите пароль-фразу",
            )
        } else {
            ApiError::new("biometric_failed", "Отпечаток не распознан. Попробуйте ещё раз.")
        }
    }

    pub fn available(app: &AppHandle) -> bool {
        app.biometric()
            .status()
            .map(|status| status.is_available)
            .unwrap_or(false)
    }

    pub fn enroll(app: &AppHandle, vault_id: Uuid, key: &[u8; 32]) -> ApiResult<()> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(key);
        let decrypted = app
            .biometric()
            .biometric_cipher(
                "Включить вход по биометрии PMS".to_string(),
                AuthOptions {
                    data_to_encrypt: Some(encoded),
                    ..auth_options()
                },
            )
            .map_err(map_error)?;
        let path = key_path(app, vault_id)?;
        fs::write(path, decrypted.data)
            .map_err(|error| ApiError::new("biometric_storage", error.to_string()))?;
        Ok(())
    }

    pub fn disable(app: &AppHandle, vault_id: Uuid) -> ApiResult<()> {
        let path = key_path(app, vault_id)?;
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| ApiError::new("biometric_storage", error.to_string()))?;
        }
        Ok(())
    }

    pub fn authenticate_and_read(app: &AppHandle, vault_id: Uuid) -> ApiResult<[u8; 32]> {
        let path = key_path(app, vault_id)?;
        let encrypted = fs::read_to_string(path).map_err(|_| {
            ApiError::new(
                "biometric_key_missing",
                "Сохранённый ключ биометрии не найден",
            )
        })?;
        let decrypted = app
            .biometric()
            .biometric_cipher(
                "Разблокировать хранилище PMS".to_string(),
                AuthOptions {
                    data_to_decrypt: Some(encrypted),
                    ..auth_options()
                },
            )
            .map_err(map_error)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(decrypted.data)
            .map_err(|_| ApiError::new("biometric_failed", "Повреждён ключ биометрии"))?;
        bytes.try_into().map_err(|_| {
            ApiError::new("biometric_failed", "Повреждён ключ биометрии")
        })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "android")))]
mod imp {
    use uuid::Uuid;

    use crate::commands::{ApiError, ApiResult};

    pub fn available() -> bool {
        false
    }

    pub fn enroll(_vault_id: Uuid, _key: &[u8; 32]) -> ApiResult<()> {
        Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия пока поддерживается только на macOS",
        ))
    }

    pub fn disable(_vault_id: Uuid) -> ApiResult<()> {
        Ok(())
    }

    pub fn unlock(_vault_id: Uuid) -> ApiResult<()> {
        Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия пока поддерживается только на macOS",
        ))
    }

    pub fn read(_vault_id: Uuid) -> ApiResult<[u8; 32]> {
        Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия пока поддерживается только на macOS",
        ))
    }
}
