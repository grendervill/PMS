use std::sync::Mutex;

use uuid::Uuid;

use super::error::VaultError;
use super::vault::VaultSession;

#[derive(Default)]
pub struct SessionManager {
    inner: Mutex<Option<VaultSession>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, session: VaultSession) {
        if let Ok(mut inner) = self.inner.lock() {
            *inner = Some(session);
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.is_some())
            .unwrap_or(false)
    }

    pub fn with<T>(
        &self,
        session_id: Uuid,
        f: impl FnOnce(&VaultSession) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| VaultError::SessionNotFound)?;
        let session = inner
            .as_ref()
            .filter(|session| session.id == session_id)
            .ok_or(VaultError::SessionNotFound)?;
        f(session)
    }

    pub fn with_mut<T>(
        &self,
        session_id: Uuid,
        f: impl FnOnce(&mut VaultSession) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| VaultError::SessionNotFound)?;
        let session = inner
            .as_mut()
            .filter(|session| session.id == session_id)
            .ok_or(VaultError::SessionNotFound)?;
        f(session)
    }

    pub fn lock(&self, session_id: Uuid) -> Result<(), VaultError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| VaultError::SessionNotFound)?;
        match inner.as_ref() {
            Some(session) if session.id == session_id => {
                *inner = None;
                Ok(())
            }
            _ => Err(VaultError::SessionNotFound),
        }
    }

    pub fn lock_any(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            *inner = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{KdfProfile, UnlockSecretInput};
    use crate::core::models::CipherSuite;
    use crate::core::vault::{CreateVaultConfig, create_vault, save_vault};

    fn passphrase() -> UnlockSecretInput {
        UnlockSecretInput::Passphrase {
            value: "correct horse".to_string(),
        }
    }

    fn config() -> CreateVaultConfig {
        CreateVaultConfig {
            name: "Сессии".to_string(),
            kdf_profile: KdfProfile::Standard,
            cipher: CipherSuite::XChaCha20Poly1305,
        }
    }

    #[test]
    fn put_with_lock_removes_session() {
        let (_, session, _) = create_vault(&config(), &passphrase()).unwrap();
        let session_id = session.id;
        let vault_id = session.vault_id();
        let manager = SessionManager::new();

        manager.put(session);
        assert!(manager.is_unlocked());
        assert_eq!(
            manager.with(session_id, |session| Ok(session.vault_id())).unwrap(),
            vault_id
        );
        assert!(matches!(
            manager.with(Uuid::new_v4(), |session| Ok(session.vault_id())),
            Err(VaultError::SessionNotFound)
        ));

        manager.lock(session_id).unwrap();
        assert!(!manager.is_unlocked());
        assert!(matches!(
            manager.with(session_id, |session| Ok(session.vault_id())),
            Err(VaultError::SessionNotFound)
        ));
    }

    #[test]
    fn with_mut_saves_and_updates_payload() {
        let (_, session, data) = create_vault(&config(), &passphrase()).unwrap();
        let session_id = session.id;
        let manager = SessionManager::new();
        manager.put(session);

        let saved = manager
            .with_mut(session_id, |session| save_vault(session, &data))
            .unwrap();
        assert!(!saved.is_empty());

        manager.lock_any();
        assert!(!manager.is_unlocked());
    }

    #[test]
    fn lock_wrong_session_does_not_drop_active_session() {
        let (_, session, _) = create_vault(&config(), &passphrase()).unwrap();
        let session_id = session.id;
        let manager = SessionManager::new();
        manager.put(session);

        assert!(matches!(
            manager.lock(Uuid::new_v4()),
            Err(VaultError::SessionNotFound)
        ));
        assert!(manager.is_unlocked());
        manager.lock(session_id).unwrap();
    }
}
