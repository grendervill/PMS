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

/// Допустимый логин: 3–64 символа, латиница в нижнем регистре, цифры, точка,
/// дефис и подчёркивание. Логин — часть соли identity-ключа, поэтому менять его
/// правила после релиза нельзя.
pub fn normalize_login(value: &str) -> Option<String> {
    let login = value.trim().to_ascii_lowercase();
    let valid = (3..=64).contains(&login.len())
        && login.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && !login.starts_with('.')
        && !login.starts_with('-');
    valid.then_some(login)
}

pub const LOGIN_MIN_LEN: usize = 3;
pub const LOGIN_MAX_LEN: usize = 64;

/// Соль identity-ключа — детерминированная функция мастер-ключа и логина.
///
/// Публичная соль не ослабляет модель: параметры KDF и завёрнутый ключ сейфа и
/// так лежат открытым текстом в заголовке файла, поэтому офлайн-перебор
/// мастер-ключа возможен и без сервера.
pub fn identity_salt(canonical_secret: &[u8], login: &str) -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(None, canonical_secret);
    let mut info = Vec::with_capacity(SALT_INFO.len() + 1 + login.len());
    info.extend_from_slice(SALT_INFO);
    info.push(0);
    info.extend_from_slice(login.as_bytes());

    let mut salt = [0u8; 32];
    hkdf.expand(&info, &mut salt)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    salt
}

/// Полный вывод: `IK = Argon2id(secret, salt)`, `AUTH = HKDF(IK, "ed25519-seed")`.
pub fn derive_identity(canonical_secret: &[u8], login: &str, kdf: &IdentityKdf) -> Identity {
    derive_identity_checked(canonical_secret, login, kdf)
        .expect("identity KDF parameters are out of range")
}

/// То же, но с ошибкой вместо паники — для проверки конфигурации.
pub fn derive_identity_checked(
    canonical_secret: &[u8],
    login: &str,
    kdf: &IdentityKdf,
) -> Result<Identity, IdentityError> {
    if canonical_secret.is_empty() {
        return Err(IdentityError::EmptySecret);
    }
    let login = normalize_login(login).ok_or(IdentityError::BadLogin)?;

    let params =
        Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32)).map_err(|_| IdentityError::BadKdfParams)?;
    let salt = identity_salt(canonical_secret, &login);
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

/// 16 байт → 32 hex-символа. Не секрет: им адресуются аккаунты в API и на диске.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId([u8; 16]);

impl AccountId {
    /// `BLAKE3(public_key)[0..16]`.
    pub fn from_public_key(public: &[u8; 32]) -> Self {
        let digest = blake3::hash(public);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }

    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        let bytes = hex::decode(value).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes);
        Some(Self(id))
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", hex::encode(self.0))
    }
}

impl std::str::FromStr for AccountId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_hex(value).ok_or(())
    }
}

/// Идентификатор установки приложения. Привязывается к подписи и к сессии,
/// чтобы перехваченный токен нельзя было перевесить на другое устройство.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId([u8; 16]);

impl DeviceId {
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        let bytes = hex::decode(value).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes);
        Some(Self(id))
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", hex::encode(self.0))
    }
}

const CHALLENGE_LABEL: &[u8] = b"PMS-AUTH-v1";
/// `label || account (16) || device (16) || nonce (32)`.
pub const CHALLENGE_MESSAGE_LEN: usize = CHALLENGE_LABEL.len() + 64;

pub fn challenge_message(
    account: &AccountId,
    device: &DeviceId,
    nonce: &[u8; 32],
) -> [u8; CHALLENGE_MESSAGE_LEN] {
    let mut message = [0u8; CHALLENGE_MESSAGE_LEN];
    let (label, rest) = message.split_at_mut(CHALLENGE_LABEL.len());
    label.copy_from_slice(CHALLENGE_LABEL);
    rest[..16].copy_from_slice(account.as_bytes());
    rest[16..32].copy_from_slice(device.as_bytes());
    rest[32..].copy_from_slice(nonce);
    message
}

/// Подпись Ed25519 над [`challenge_message`].
pub fn sign_challenge(
    identity: &Identity,
    account: &AccountId,
    device: &DeviceId,
    nonce: &[u8; 32],
) -> [u8; 64] {
    use ed25519_dalek::Signer;
    let signing_key = SigningKey::from_bytes(&identity.secret);
    signing_key
        .sign(&challenge_message(account, device, nonce))
        .to_bytes()
}

/// Проверка подписи. Любая ошибка разбора ключа или подписи — `false`, без паник.
pub fn verify_challenge(
    public: &[u8; 32],
    account: &AccountId,
    device: &DeviceId,
    nonce: &[u8; 32],
    signature: &[u8; 64],
) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let Ok(verifying_key) = VerifyingKey::from_bytes(public) else {
        return false;
    };
    verifying_key
        .verify(
            &challenge_message(account, device, nonce),
            &Signature::from_bytes(signature),
        )
        .is_ok()
}

const REGISTER_LABEL: &[u8] = b"PMS-REGISTER-v1";
/// `label || account (16) || device (16) || BLAKE3(vault)`.
pub const REGISTER_MESSAGE_LEN: usize = REGISTER_LABEL.len() + 64;

pub fn register_message(
    account: &AccountId,
    device: &DeviceId,
    vault_digest: &[u8; 32],
) -> [u8; REGISTER_MESSAGE_LEN] {
    let mut message = [0u8; REGISTER_MESSAGE_LEN];
    let (label, rest) = message.split_at_mut(REGISTER_LABEL.len());
    label.copy_from_slice(REGISTER_LABEL);
    rest[..16].copy_from_slice(account.as_bytes());
    rest[16..32].copy_from_slice(device.as_bytes());
    rest[32..].copy_from_slice(vault_digest);
    message
}

pub fn sign_register(
    identity: &Identity,
    account: &AccountId,
    device: &DeviceId,
    vault_digest: &[u8; 32],
) -> [u8; 64] {
    use ed25519_dalek::Signer;
    SigningKey::from_bytes(&identity.secret)
        .sign(&register_message(account, device, vault_digest))
        .to_bytes()
}

pub fn verify_register(
    public: &[u8; 32],
    account: &AccountId,
    device: &DeviceId,
    vault_digest: &[u8; 32],
    signature: &[u8; 64],
) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let Ok(verifying_key) = VerifyingKey::from_bytes(public) else {
        return false;
    };
    verifying_key
        .verify(
            &register_message(account, device, vault_digest),
            &Signature::from_bytes(signature),
        )
        .is_ok()
}

const ACCOUNT_DELETE_LABEL: &[u8] = b"PMS-ACCOUNT-DELETE-v1";

pub fn account_delete_message(account: &AccountId, device: &DeviceId) -> [u8; 32] {
    let mut message = [0u8; 32];
    message[..16].copy_from_slice(account.as_bytes());
    message[16..].copy_from_slice(device.as_bytes());
    message
}

pub fn sign_account_delete(
    identity: &Identity,
    account: &AccountId,
    device: &DeviceId,
) -> [u8; 64] {
    use ed25519_dalek::Signer;
    let mut payload = Vec::with_capacity(ACCOUNT_DELETE_LABEL.len() + 32);
    payload.extend_from_slice(ACCOUNT_DELETE_LABEL);
    payload.extend_from_slice(&account_delete_message(account, device));
    SigningKey::from_bytes(&identity.secret)
        .sign(&payload)
        .to_bytes()
}

pub fn verify_account_delete(
    public: &[u8; 32],
    account: &AccountId,
    device: &DeviceId,
    signature: &[u8; 64],
) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let Ok(verifying_key) = VerifyingKey::from_bytes(public) else {
        return false;
    };
    let mut payload = Vec::with_capacity(ACCOUNT_DELETE_LABEL.len() + 32);
    payload.extend_from_slice(ACCOUNT_DELETE_LABEL);
    payload.extend_from_slice(&account_delete_message(account, device));
    verifying_key
        .verify(&payload, &Signature::from_bytes(signature))
        .is_ok()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("мастер-ключ пуст")]
    EmptySecret,
    #[error("параметры Argon2id вне допустимого диапазона")]
    BadKdfParams,
    #[error("не удалось вывести identity-ключ")]
    Kdf,
    #[error("недопустимый логин: 3–64 символа, латиница, цифры, точка, дефис, подчёркивание")]
    BadLogin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_secret_same_identity_and_salt_differs_from_other_secret() {
        let kdf = IdentityKdf::FAST_FOR_TESTS;
        let a = derive_identity(b"correct horse", "alice", &kdf);
        let b = derive_identity(b"correct horse", "alice", &kdf);
        let c = derive_identity(b"correct horse!", "alice", &kdf);

        assert_eq!(a.public, b.public);
        assert_ne!(a.public, c.public);
        assert_ne!(
            identity_salt(b"correct horse", "alice"),
            identity_salt(b"correct horse!", "alice")
        );
    }

    #[test]
    fn login_changes_the_identity() {
        let kdf = IdentityKdf::FAST_FOR_TESTS;
        let alice = derive_identity(b"same phrase", "alice", &kdf);
        let bob = derive_identity(b"same phrase", "bob", &kdf);
        assert_ne!(
            alice.public, bob.public,
            "один мастер-ключ с разными логинами — разные пользователи"
        );
        assert_ne!(
            identity_salt(b"same phrase", "alice"),
            identity_salt(b"same phrase", "bob")
        );
    }

    #[test]
    fn logins_are_normalized_and_validated() {
        assert_eq!(normalize_login("  Alice ").as_deref(), Some("alice"));
        assert_eq!(normalize_login("a.b_c-d1").as_deref(), Some("a.b_c-d1"));
        for bad in [
            "ab",
            "",
            "  ",
            "алиса",
            "a b",
            "-alice",
            ".alice",
            &"a".repeat(65),
        ] {
            assert!(normalize_login(bad).is_none(), "приняли {bad:?}");
        }
    }

    #[test]
    fn identity_v1_is_pinned() {
        let identity = derive_identity(b"pms-test-vector", "vector", &IdentityKdf::FAST_FOR_TESTS);
        assert_eq!(
            hex::encode(identity.public),
            "d5811d595363e244464288956be76dc7229f05df145a89a709e5b2a378552429"
        );
    }

    #[test]
    fn signature_binds_account_device_and_nonce() {
        let identity = derive_identity(b"secret", "alice", &IdentityKdf::FAST_FOR_TESTS);
        let account = AccountId::from_public_key(&identity.public);
        let device = DeviceId::new([7u8; 16]);
        let nonce = [9u8; 32];
        let signature = sign_challenge(&identity, &account, &device, &nonce);

        assert!(verify_challenge(
            &identity.public,
            &account,
            &device,
            &nonce,
            &signature
        ));
        assert!(!verify_challenge(
            &identity.public,
            &account,
            &DeviceId::new([8u8; 16]),
            &nonce,
            &signature
        ));
        assert!(!verify_challenge(
            &identity.public,
            &account,
            &device,
            &[10u8; 32],
            &signature
        ));
        assert!(!verify_challenge(
            &identity.public,
            &AccountId::new([3u8; 16]),
            &device,
            &nonce,
            &signature
        ));
    }

    #[test]
    fn account_id_is_derived_from_public_key() {
        let public = [5u8; 32];
        let account = AccountId::from_public_key(&public);
        assert_eq!(account, AccountId::from_public_key(&public));
        assert_eq!(account.to_string().len(), 32);
        assert_eq!(AccountId::from_hex(&account.to_string()), Some(account));
        assert_eq!(AccountId::from_hex("zz"), None);
    }

    #[test]
    fn register_signature_binds_account_device_and_vault_digest() {
        let identity = derive_identity(b"secret", "alice", &IdentityKdf::FAST_FOR_TESTS);
        let account = AccountId::from_public_key(&identity.public);
        let device = DeviceId::new([1u8; 16]);
        let digest = *blake3::hash(b"vault-bytes").as_bytes();

        let signature = sign_register(&identity, &account, &device, &digest);
        assert!(verify_register(
            &identity.public,
            &account,
            &device,
            &digest,
            &signature
        ));

        let other = *blake3::hash(b"other-bytes").as_bytes();
        assert!(!verify_register(
            &identity.public,
            &account,
            &device,
            &other,
            &signature
        ));
        assert!(!verify_register(
            &identity.public,
            &account,
            &DeviceId::new([2u8; 16]),
            &digest,
            &signature
        ));
    }

    #[test]
    fn account_delete_signature_binds_account_and_device() {
        let identity = derive_identity(b"secret", "alice", &IdentityKdf::FAST_FOR_TESTS);
        let account = AccountId::from_public_key(&identity.public);
        let device = DeviceId::new([4u8; 16]);
        let signature = sign_account_delete(&identity, &account, &device);

        assert!(verify_account_delete(
            &identity.public,
            &account,
            &device,
            &signature
        ));
        assert!(!verify_account_delete(
            &identity.public,
            &account,
            &DeviceId::new([5u8; 16]),
            &signature
        ));
    }

    #[test]
    fn challenge_message_has_fixed_layout() {
        let message = challenge_message(
            &AccountId::new([1u8; 16]),
            &DeviceId::new([2u8; 16]),
            &[3u8; 32],
        );
        assert_eq!(message.len(), 11 + 16 + 16 + 32);
        assert_eq!(&message[..11], b"PMS-AUTH-v1");
    }

    #[test]
    fn empty_secret_is_refused() {
        assert_eq!(
            derive_identity_checked(b"", "alice", &IdentityKdf::FAST_FOR_TESTS).unwrap_err(),
            IdentityError::EmptySecret
        );
        assert_eq!(
            derive_identity_checked(b"secret", "a", &IdentityKdf::FAST_FOR_TESTS).unwrap_err(),
            IdentityError::BadLogin
        );
    }
}
