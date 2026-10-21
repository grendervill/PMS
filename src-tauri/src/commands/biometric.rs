use base64::Engine as _;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::commands::vault::UnlockVaultResponse;
use crate::commands::{ApiError, ApiResult, AppState};
use crate::core::vault::unlock_vault_with_key;

const SERVICE: &str = "dev.pms.vault.biometric";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiometricStatus {
    pub available: bool,
}

#[tauri::command]
pub fn biometric_status() -> ApiResult<BiometricStatus> {
    Ok(BiometricStatus {
        available: imp::available(),
    })
}

#[tauri::command]
pub fn biometric_enroll(session_id: Uuid, state: State<'_, AppState>) -> ApiResult<()> {
    let key = state
        .sessions
        .with(session_id, |session| Ok(*session.vault_key()))?;
    imp::enroll(session_id, &key)
}

#[tauri::command]
pub fn biometric_disable(vault_id: Uuid) -> ApiResult<()> {
    imp::disable(vault_id)
}

#[tauri::command]
pub fn biometric_unlock(
    file_b64: String,
    vault_id: Uuid,
    state: State<'_, AppState>,
) -> ApiResult<UnlockVaultResponse> {
    if !imp::available() {
        return Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия на этом устройстве недоступна",
        ));
    }
    let file = base64::engine::general_purpose::STANDARD
        .decode(file_b64)
        .map_err(|_| ApiError::bad_base64())?;
    let key = imp::read(vault_id)?;
    let (session, data) = unlock_vault_with_key(&file, &key).map_err(ApiError::from)?;
    let session_id = session.id;
    state.sessions.put(session);

    Ok(UnlockVaultResponse { session_id, data })
}

#[cfg(target_os = "macos")]
mod imp {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::base::Error as SecurityError;
    use security_framework::passwords::{
        AccessControlOptions, PasswordOptions, delete_generic_password, generic_password,
        set_generic_password_options,
    };
    use security_framework_sys::base::{
        errSecAuthFailed as ERR_SEC_AUTH_FAILED, errSecItemNotFound as ERR_SEC_ITEM_NOT_FOUND,
    };

    const ERR_SEC_USER_CANCELED: i32 = -128;
    const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::SERVICE;
    use crate::commands::{ApiError, ApiResult};

    pub fn available() -> bool {
        use objc2_local_authentication::{LAContext, LAPolicy};
        let context = unsafe { LAContext::new() };
        unsafe {
            context
                .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
                .is_ok()
        }
    }

    fn map_keychain_error(error: SecurityError) -> ApiError {
        match error.code() {
            ERR_SEC_USER_CANCELED => ApiError::new("biometric_cancelled", "Вход по отпечатку отменён"),
            ERR_SEC_ITEM_NOT_FOUND => ApiError::new(
                "biometric_key_missing",
                "Сохранённый ключ биометрии не найден",
            ),
            ERR_SEC_INTERACTION_NOT_ALLOWED => ApiError::new(
                "biometric_lockout",
                "Биометрия временно заблокирована. Введите пароль-фразу",
            ),
            ERR_SEC_AUTH_FAILED => ApiError::new(
                "biometric_failed",
                "Отпечаток не распознан",
            ),
            _ => ApiError::new("biometric_failed", "Не удалось пройти биометрию"),
        }
    }

    fn access_control() -> ApiResult<SecAccessControl> {
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
            AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
        )
        .map_err(|_| {
            ApiError::new(
                "biometric_unavailable",
                "Не удалось настроить защиту ключа биометрией",
            )
        })
    }

    pub fn enroll(vault_id: Uuid, key: &[u8; 32]) -> ApiResult<()> {
        if !available() {
            return Err(ApiError::new(
                "biometric_unavailable",
                "Биометрия на этом устройстве недоступна",
            ));
        }
        let account = vault_id.to_string();
        let mut options = PasswordOptions::new_generic_password(SERVICE, &account);
        options.set_access_control(access_control()?);

        let secret = Zeroizing::new(key.to_vec());
        set_generic_password_options(&secret, options).map_err(map_keychain_error)?;

        // Force the first biometric check now, so enabling cannot silently store
        // a key that the user never confirmed.
        if let Err(error) = generic_password(PasswordOptions::new_generic_password(SERVICE, &account)) {
            let _ = delete_generic_password(SERVICE, &account);
            return Err(map_keychain_error(error));
        }
        Ok(())
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
        fn availability_query_is_safe() {
            let _ = available();
        }

        #[test]
        fn disabling_a_missing_item_is_idempotent() {
            assert!(disable(Uuid::new_v4()).is_ok());
        }

        #[test]
        fn maps_cancelled_and_failed_errors() {
            assert_eq!(
                map_keychain_error(SecurityError::from_code(ERR_SEC_USER_CANCELED)).code,
                "biometric_cancelled"
            );
            assert_eq!(
                map_keychain_error(SecurityError::from_code(ERR_SEC_AUTH_FAILED)).code,
                "biometric_failed"
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
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

    pub fn read(_vault_id: Uuid) -> ApiResult<[u8; 32]> {
        Err(ApiError::new(
            "biometric_unavailable",
            "Биометрия пока поддерживается только на macOS",
        ))
    }
}
