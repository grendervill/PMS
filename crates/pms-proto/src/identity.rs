//! Вывод ключа аккаунта из мастер-ключа.
//!
//! Соль и параметры KDF детерминированы: любое устройство, знающее мастер-ключ,
//! выводит тот же ключ и тот же идентификатор аккаунта, не спрашивая сервер.
//!
//! Разделение доменов: из одного `IK` получаются соль identity-KDF и семя
//! Ed25519-ключа подписи. Ключ сейфа (`vault_key`) сюда не входит — он живёт
//! внутри запечатанного файла, как и раньше.

use argon2::{Algorithm, Argon2, Params, Version};
use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Константы протокола: менять нельзя, иначе у двух устройств разойдутся ключи.
const SALT_INFO: &[u8] = b"PMS-IDENTITY-SALT-v1";
const AUTH_INFO: &[u8] = b"PMS-AUTH-v1";
const ED25519_SEED_INFO: &[u8] = b"ed25519-seed";

/// Параметры Argon2id для вывода identity-ключа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityKdf {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl IdentityKdf {
    /// Единственный разрешённый профиль в протоколе версии 1.
    pub const V1: Self = Self {
        m_kib: 131_072,
        t: 4,
        p: 4,
    };

    /// Быстрый профиль для тестов: те же шаги, но без секунды на Argon2id.
    pub const FAST_FOR_TESTS: Self = Self {
        m_kib: 8,
        t: 1,
        p: 1,
    };
}

/// Секрет ключа аккаунта и его публичная половина (она же личность).
pub struct Identity {
    pub secret: Zeroizing<[u8; 32]>,
    pub public: [u8; 32],
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Identity")
            .field("public", &hex::encode(self.public))
            .finish_non_exhaustive()
    }
}

/// Соль identity-ключа — детерминированная функция мастер-ключа.
///
/// Публичная соль не ослабляет модель: параметры KDF и завёрнутый ключ сейфа и
/// так лежат открытым текстом в заголовке файла, поэтому офлайн-перебор
/// мастер-ключа возможен и без сервера.
pub fn identity_salt(canonical_secret: &[u8]) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(None, canonical_secret);
    let mut salt = [0u8; 32];
    hkdf.expand(SALT_INFO, &mut salt)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    salt
}

/// Полный вывод: `IK = Argon2id(secret, salt)`, `AUTH = HKDF(IK, "ed25519-seed")`.
pub fn derive_identity(canonical_secret: &[u8], kdf: &IdentityKdf) -> Identity {
    derive_identity_checked(canonical_secret, kdf).expect("identity KDF parameters are out of range")
}

/// То же, но с ошибкой вместо паники — для проверки конфигурации.
pub fn derive_identity_checked(
    canonical_secret: &[u8],
    kdf: &IdentityKdf,
) -> Result<Identity, IdentityError> {
    if canonical_secret.is_empty() {
        return Err(IdentityError::EmptySecret);
    }

    let params = Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32))
        .map_err(|_| IdentityError::BadKdfParams)?;
    let salt = identity_salt(canonical_secret);
    let mut identity_key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(canonical_secret, &salt, identity_key.as_mut())
        .map_err(|_| IdentityError::Kdf)?;

    let hkdf = Hkdf::<Sha256>::new(Some(AUTH_INFO), identity_key.as_ref());
    let mut seed = Zeroizing::new([0u8; 32]);
    hkdf.expand(ED25519_SEED_INFO, &mut seed[..])
        .expect("32 bytes is a valid HKDF-SHA256 output length");

    let signing_key = SigningKey::from_bytes(&seed);
    let public = signing_key.verifying_key().to_bytes();

    Ok(Identity {
        secret: seed,
        public,
    })
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("мастер-ключ пуст")]
    EmptySecret,
    #[error("параметры Argon2id вне допустимого диапазона")]
    BadKdfParams,
    #[error("не удалось вывести identity-ключ")]
    Kdf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_secret_same_identity_and_salt_differs_from_other_secret() {
        let kdf = IdentityKdf::FAST_FOR_TESTS;
        let a = derive_identity(b"correct horse", &kdf);
        let b = derive_identity(b"correct horse", &kdf);
        let c = derive_identity(b"correct horse!", &kdf);

        assert_eq!(a.public, b.public);
        assert_ne!(a.public, c.public);
        assert_ne!(
            identity_salt(b"correct horse"),
            identity_salt(b"correct horse!")
        );
    }

    #[test]
    fn identity_v1_is_pinned() {
        let identity = derive_identity(b"pms-test-vector", &IdentityKdf::FAST_FOR_TESTS);
        assert_eq!(
            hex::encode(identity.public),
            "820416881aff3da70678aa88f12af5bf44dde6e7b4f76497182d9b38b33fc3f8"
        );
    }

    #[test]
    fn empty_secret_is_refused() {
        assert_eq!(
            derive_identity_checked(b"", &IdentityKdf::FAST_FOR_TESTS).unwrap_err(),
            IdentityError::EmptySecret
        );
    }
}
