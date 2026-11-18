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
    /// True when the stored key can only be read after the user authenticates
    /// (macOS data-protection keychain with an access control object). False
    /// means the older, weaker "generic password" storage is used.
    pub keychain_protected: bool,
}

#[tauri::command]
pub async fn biometric_status(
    app: AppHandle,
    vault_id: Option<Uuid>,
) -> ApiResult<BiometricStatus> {
    let _ = (&app, vault_id);
    #[cfg(target_os = "macos")]
    let (available, keychain_protected) = (
        imp::available(),
        vault_id.is_some_and(imp::is_protected) && imp::available(),
    );
    #[cfg(target_os = "android")]
    let (available, keychain_protected) = (android::available(&app), true);
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    let (available, keychain_protected) = (false, false);

    Ok(BiometricStatus {
        available,
        keychain_protected,
    })
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
    // Biometric failures count against the same cool-down as passphrase
    // failures: the five-attempt limit used to live only in the renderer.
    state.attempts.check(vault_id)?;

    #[cfg(target_os = "macos")]
    let key = imp::read(vault_id)?;
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

    let (session, data) = match unlock_vault_with_key(&file, &key) {
        Ok(result) => {
            state.attempts.record_success(vault_id);
            result
        }
        Err(error) => {
            if matches!(error, crate::core::error::VaultError::InvalidCredentials) {
                state.attempts.record_failure(vault_id);
            }
            return Err(error.into());
        }
    };
    let session_id = session.id;
    crate::commands::secure_screen::apply(&app, data.settings.secure_screen);
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
        delete_generic_password, generic_password, set_generic_password_options, PasswordOptions,
    };
    use security_framework_sys::base::{
        errSecAuthFailed as ERR_SEC_AUTH_FAILED, errSecItemNotFound as ERR_SEC_ITEM_NOT_FOUND,
    };
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
            ERR_SEC_AUTH_FAILED => {
                ApiError::new("biometric_failed", "Не удалось прочитать сохранённый ключ")
            }
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

    fn account(vault_id: Uuid) -> String {
        vault_id.to_string()
    }

    /// Stores the vault key behind an access control object, so reading it
    /// requires the user to authenticate (Touch ID, or the device passcode).
    ///
    /// Returns `Ok(false)` when the data-protection keychain is unavailable
    /// (unsigned build, no device passcode): the caller then falls back to the
    /// generic password item used by earlier versions.
    pub fn store(vault_id: Uuid, key: &[u8; 32]) -> ApiResult<bool> {
        match protected::store(vault_id, key) {
            Ok(()) => {
                // Remove the unprotected copy left by older versions, otherwise
                // the access control would be pointless.
                let _ = delete_generic_password(SERVICE, &account(vault_id));
                mark_protected(vault_id, true);
                Ok(true)
            }
            Err(_) => {
                let options = PasswordOptions::new_generic_password(SERVICE, &account(vault_id));
                let secret = Zeroizing::new(key.to_vec());
                set_generic_password_options(&secret, options).map_err(map_keychain_error)?;
                mark_protected(vault_id, false);
                Ok(false)
            }
        }
    }

    pub fn enroll(vault_id: Uuid, key: &[u8; 32]) -> ApiResult<()> {
        if !available() {
            return Err(ApiError::new(
                "biometric_unavailable",
                "Биометрия на этом устройстве недоступна",
            ));
        }
        // User presence before storing: adding a keychain item does not prompt
        // by itself.
        evaluate()?;
        store(vault_id, key).map(|_| ())
    }

    pub fn disable(vault_id: Uuid) -> ApiResult<()> {
        let _ = protected::delete(vault_id);
        mark_protected(vault_id, false);
        match delete_generic_password(SERVICE, &account(vault_id)) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(error) => Err(map_keychain_error(error)),
        }
    }

    /// Reads the vault key. A protected item makes the *system* ask for
    /// authentication; the legacy item still needs the explicit prompt.
    pub fn read(vault_id: Uuid) -> ApiResult<[u8; 32]> {
        match protected::read(vault_id) {
            Ok(value) => into_key(value),
            Err(protected::ReadError::NotFound) | Err(protected::ReadError::Unsupported) => {
                evaluate()?;
                let value = Zeroizing::new(
                    generic_password(PasswordOptions::new_generic_password(
                        SERVICE,
                        &account(vault_id),
                    ))
                    .map_err(map_keychain_error)?,
                );
                into_key(value.to_vec())
            }
            Err(protected::ReadError::Denied(code)) => Err(map_la_error(code)),
            Err(protected::ReadError::Other(code)) => Err(ApiError::new(
                "biometric_failed",
                format!("Не удалось прочитать сохранённый ключ ({code})"),
            )),
        }
    }

    fn into_key(value: Vec<u8>) -> ApiResult<[u8; 32]> {
        let value = Zeroizing::new(value);
        value.as_slice().try_into().map_err(|_| {
            ApiError::new(
                "biometric_key_missing",
                "Сохранённый ключ биометрии повреждён",
            )
        })
    }

    /// Whether the key for this vault lives behind an access control object.
    pub fn is_protected(vault_id: Uuid) -> bool {
        marker(vault_id).is_file()
    }

    fn mark_protected(vault_id: Uuid, protected: bool) {
        if !protected {
            let _ = std::fs::remove_file(marker(vault_id));
            return;
        }
        if let Some(parent) = marker(vault_id).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(marker(vault_id), b"1");
    }

    /// Holds no secret: only "this vault uses an access-controlled item", so the
    /// status can be reported without triggering an authentication prompt.
    fn marker(vault_id: Uuid) -> std::path::PathBuf {
        crate::commands::app_data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("biometric")
            .join(format!("{vault_id}.protected"))
    }

    /// Keychain item protected by user presence.
    ///
    /// The item is created in the data-protection keychain: `SecAccessControl`
    /// is only honoured there, the legacy file-based keychain ignores it. Add,
    /// query and delete all pass `kSecUseDataProtectionKeychain`.
    mod protected {
        use std::ptr;

        use super::{account, SERVICE};
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::boolean::CFBoolean;
        use core_foundation::data::CFData;
        use core_foundation::dictionary::CFMutableDictionary;
        use core_foundation::string::CFString;
        use security_framework::access_control::{ProtectionMode, SecAccessControl};
        use security_framework_sys::base::errSecItemNotFound;

        type OSStatus = i32;
        use security_framework_sys::item::{
            kSecAttrAccessControl, kSecAttrAccount, kSecAttrService, kSecClass,
            kSecClassGenericPassword, kSecMatchLimit, kSecReturnData,
            kSecUseDataProtectionKeychain, kSecValueData,
        };
        use security_framework_sys::keychain_item::{
            SecItemAdd, SecItemCopyMatching, SecItemDelete,
        };

        extern "C" {
            static kSecMatchLimitOne: core_foundation_sys::string::CFStringRef;
            static kSecUseOperationPrompt: core_foundation_sys::string::CFStringRef;
        }

        const AUTH_PROMPT: &str = "Разблокировать хранилище PMS";

        #[derive(Debug)]
        pub enum ReadError {
            /// No protected item: fall back to the legacy storage.
            NotFound,
            /// The data-protection keychain is not usable here.
            Unsupported,
            /// The user cancelled or authentication failed.
            Denied(i32),
            Other(OSStatus),
        }

        #[allow(non_local_definitions)]
        impl From<OSStatus> for ReadError {
            fn from(code: OSStatus) -> Self {
                if code == errSecItemNotFound {
                    return Self::NotFound;
                }
                match code {
                    // errSecMissingEntitlement / errSecParam / errSecDecode:
                    // unsigned or unentitled build.
                    -34018 | -50 | -26275 => Self::Unsupported,
                    // userCanceled / authFailed / uiRequired / interactionNotAllowed
                    -128 | -25293 | -25308 | -25315 => Self::Denied(code),
                    other => Self::Other(other),
                }
            }
        }

        fn cf(value: core_foundation_sys::string::CFStringRef) -> CFString {
            unsafe { CFString::wrap_under_get_rule(value) }
        }

        fn access_control() -> Option<SecAccessControl> {
            use security_framework_sys::access_control::{
                kSecAccessControlBiometryCurrentSet, kSecAccessControlDevicePasscode,
                kSecAccessControlOr,
            };
            SecAccessControl::create_with_protection(
                Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
                kSecAccessControlBiometryCurrentSet
                    | kSecAccessControlOr
                    | kSecAccessControlDevicePasscode,
            )
            .ok()
        }

        fn base_query(vault_id: uuid::Uuid) -> CFMutableDictionary<CFString, CFType> {
            let mut query: CFMutableDictionary<CFString, CFType> = CFMutableDictionary::new();
            query.add(
                &cf(unsafe { kSecClass }),
                &cf(unsafe { kSecClassGenericPassword }).as_CFType(),
            );
            query.add(
                &cf(unsafe { kSecAttrService }),
                &CFString::new(SERVICE).as_CFType(),
            );
            query.add(
                &cf(unsafe { kSecAttrAccount }),
                &CFString::new(&account(vault_id)).as_CFType(),
            );
            query.add(
                &cf(unsafe { kSecUseDataProtectionKeychain }),
                &CFBoolean::true_value().as_CFType(),
            );
            query
        }

        pub fn store(vault_id: uuid::Uuid, key: &[u8; 32]) -> Result<(), OSStatus> {
            let control = access_control().ok_or(-50)?;
            let _ = delete(vault_id);

            let mut attributes = base_query(vault_id);
            attributes.add(
                &cf(unsafe { kSecValueData }),
                &CFData::from_buffer(key).as_CFType(),
            );
            attributes.add(&cf(unsafe { kSecAttrAccessControl }), &control.as_CFType());

            let status = unsafe { SecItemAdd(attributes.as_concrete_TypeRef(), ptr::null_mut()) };
            if status == 0 {
                Ok(())
            } else {
                Err(status)
            }
        }

        pub fn read(vault_id: uuid::Uuid) -> Result<Vec<u8>, ReadError> {
            let mut query = base_query(vault_id);
            query.add(
                &cf(unsafe { kSecReturnData }),
                &CFBoolean::true_value().as_CFType(),
            );
            query.add(
                &cf(unsafe { kSecMatchLimit }),
                &cf(unsafe { kSecMatchLimitOne }).as_CFType(),
            );
            query.add(
                &cf(unsafe { kSecUseOperationPrompt }),
                &CFString::new(AUTH_PROMPT).as_CFType(),
            );

            let mut result: core_foundation_sys::base::CFTypeRef = ptr::null();
            let status = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) };
            if status != 0 {
                return Err(ReadError::from(status));
            }
            if result.is_null() {
                return Err(ReadError::NotFound);
            }
            let data = unsafe {
                CFData::wrap_under_create_rule(result as core_foundation_sys::data::CFDataRef)
            };
            Ok(data.bytes().to_vec())
        }

        pub fn delete(vault_id: uuid::Uuid) -> Result<(), OSStatus> {
            let query = base_query(vault_id);
            let status = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
            if status == 0 || status == errSecItemNotFound {
                Ok(())
            } else {
                Err(status)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn maps_cancel_and_lockout_errors() {
            assert_eq!(map_la_error(ERR_USER_CANCELED).code, "biometric_cancelled");
            assert_eq!(map_la_error(ERR_BIOMETRY_LOCKOUT).code, "biometric_lockout");
            assert_eq!(map_la_error(-999).code, "biometric_failed");
        }

        #[test]
        fn availability_query_is_safe() {
            let _ = available();
        }

        /// The protected query must answer "not found"/"unsupported" instead of
        /// crashing or blocking when there is no item for this vault.
        #[test]
        fn protected_read_reports_a_missing_item() {
            let vault_id = Uuid::new_v4();
            match protected::read(vault_id) {
                Err(protected::ReadError::NotFound) | Err(protected::ReadError::Unsupported) => {}
                other => panic!("unexpected result for a missing item: {other:?}"),
            }
        }

        /// Full round trip through the access-controlled keychain. Requires a
        /// real user session with Touch ID, so it is not part of `cargo test`:
        ///
        /// ```sh
        /// cargo test --lib biometric -- --ignored --nocapture
        /// ```
        /// Expected: the system asks for Touch ID once, then the key is read.
        #[test]
        #[ignore = "requires Touch ID and a user at the machine"]
        fn protected_round_trip_after_biometric_prompt() {
            use crate::core::crypto::UnlockSecretInput;
            use crate::core::models::CipherSuite;
            use crate::core::vault::{create_vault, unlock_vault_with_key, CreateVaultConfig};

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
            let protected = store(vault_id, session.vault_key()).unwrap();
            println!("keychain item is access controlled: {protected}");
            assert!(
                protected,
                "the access-controlled keychain must be used here"
            );
            assert!(is_protected(vault_id));

            let key = read(vault_id).unwrap();
            let (_, unlocked) = unlock_vault_with_key(&file, &key).unwrap();
            assert_eq!(unlocked, data);

            disable(vault_id).unwrap();
            assert!(!is_protected(vault_id));
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
            ApiError::new(
                "biometric_failed",
                "Отпечаток не распознан. Попробуйте ещё раз.",
            )
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
        bytes
            .try_into()
            .map_err(|_| ApiError::new("biometric_failed", "Повреждён ключ биометрии"))
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
