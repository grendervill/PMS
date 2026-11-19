//! Полезная нагрузка биометрического хранилища.
//!
//! Версия 1 — только ключ сейфа (как было). Версия 2 добавляет семя
//! identity-ключа: после биометрической разблокировки серверную сессию можно
//! поднять, не спрашивая пароль-фразу.

use zeroize::Zeroizing;

pub const VERSION_KEY_ONLY: u8 = 1;
pub const VERSION_KEY_AND_SEED: u8 = 2;

pub struct Payload {
    pub vault_key: Zeroizing<[u8; 32]>,
    pub auth_seed: Option<Zeroizing<[u8; 32]>>,
}

pub fn encode(vault_key: &[u8; 32], auth_seed: Option<&[u8; 32]>) -> Zeroizing<Vec<u8>> {
    match auth_seed {
        Some(seed) => {
            let mut bytes = Zeroizing::new(Vec::with_capacity(65));
            bytes.push(VERSION_KEY_AND_SEED);
            bytes.extend_from_slice(vault_key);
            bytes.extend_from_slice(seed);
            bytes
        }
        None => {
            let mut bytes = Zeroizing::new(Vec::with_capacity(33));
            bytes.push(VERSION_KEY_ONLY);
            bytes.extend_from_slice(vault_key);
            bytes
        }
    }
}

/// Разбирает нагрузку любой поддерживаемой версии. Всё незнакомое — ошибка,
/// а не «попробуем как ключ»: иначе мусор станет ключом сейфа.
pub fn decode(bytes: &[u8]) -> Option<Payload> {
    match bytes.first().copied()? {
        VERSION_KEY_ONLY if bytes.len() == 33 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[1..]);
            Some(Payload {
                vault_key: Zeroizing::new(key),
                auth_seed: None,
            })
        }
        VERSION_KEY_AND_SEED if bytes.len() == 65 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[1..33]);
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes[33..]);
            Some(Payload {
                vault_key: Zeroizing::new(key),
                auth_seed: Some(Zeroizing::new(seed)),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_only_payload_round_trips() {
        let key = [7u8; 32];
        let bytes = encode(&key, None);
        assert_eq!(bytes.len(), 33);

        let payload = decode(&bytes).expect("разбирается");
        assert_eq!(&*payload.vault_key, &key);
        assert!(payload.auth_seed.is_none());
    }

    #[test]
    fn key_and_seed_payload_round_trips() {
        let key = [3u8; 32];
        let seed = [9u8; 32];
        let bytes = encode(&key, Some(&seed));
        assert_eq!(bytes.len(), 65);

        let payload = decode(&bytes).expect("разбирается");
        assert_eq!(&*payload.vault_key, &key);
        assert_eq!(payload.auth_seed.as_ref().map(|value| **value), Some(seed));
    }

    #[test]
    fn garbage_is_refused_instead_of_becoming_a_key() {
        assert!(decode(&[]).is_none());
        assert!(decode(&[1u8; 32]).is_none());
        assert!(decode(&[7u8; 33]).is_none());
        assert!(decode(&[1u8; 34]).is_none());
        let mut short = vec![2u8];
        short.extend_from_slice(&[0u8; 40]);
        assert!(decode(&short).is_none());
    }
}
