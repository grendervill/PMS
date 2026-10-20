use chrono::{SecondsFormat, Utc};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::crypto::{
    KdfProfile, UnlockSecretInput, canonical_secret, derive_kek, generate_salt, header_core_aad,
    kdf_params, open_payload, seal_payload, slot_aad, unwrap_vault_key, wrap_vault_key,
};
use super::error::VaultError;
use super::models::{
    CipherSuite, KdfSlot, KdfSlotKind, VaultData, VaultHeader, VAULT_FORMAT_VERSION,
};
use super::vault_file::{VaultFile, parse_vault_file, serialize_vault_file};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultConfig {
    pub name: String,
    pub kdf_profile: KdfProfile,
    pub cipher: CipherSuite,
}

pub struct VaultSession {
    pub id: Uuid,
    pub header: VaultHeader,
    vault_key: Zeroizing<[u8; 32]>,
    payload: Vec<u8>,
}

impl VaultSession {
    pub fn vault_id(&self) -> Uuid {
        self.header.vault_id
    }

    pub fn cipher(&self) -> CipherSuite {
        self.header.cipher
    }
}

pub fn create_vault(
    config: &CreateVaultConfig,
    secret: &UnlockSecretInput,
) -> Result<(Vec<u8>, VaultSession, VaultData), VaultError> {
    let timestamp = now();
    let vault_id = Uuid::new_v4();
    let vault_key = Zeroizing::new(generate_salt());

    let mut header = VaultHeader {
        format: VAULT_FORMAT_VERSION,
        vault_id,
        created_at: timestamp.clone(),
        cipher: config.cipher,
        kdf_slots: Vec::new(),
    };
    let template = kdf_params(config.kdf_profile, generate_salt());
    let slot = make_slot(&header, secret.kind(), secret, &template, &vault_key)?;
    header.kdf_slots.push(slot);

    let data = empty_vault_data(&config.name, vault_id, &timestamp);
    let plaintext = serde_json::to_vec(&data).map_err(|_| VaultError::Serde)?;
    let payload = seal_payload(
        &vault_key,
        header.cipher,
        &header_core_aad(&header),
        &plaintext,
    )?;
    let file = serialize_file(&header, &payload)?;

    let session = VaultSession {
        id: Uuid::new_v4(),
        header,
        vault_key,
        payload,
    };

    Ok((file, session, data))
}

pub fn unlock_vault(
    file_bytes: &[u8],
    secret: &UnlockSecretInput,
) -> Result<(VaultSession, VaultData), VaultError> {
    let parsed = parse_vault_file(file_bytes)?;
    let canonical = canonical_secret(secret)?;
    let kind = secret.kind();

    let mut vault_key = None;
    for slot in parsed
        .header
        .kdf_slots
        .iter()
        .filter(|slot| slot.kind == kind)
    {
        let Ok(kek) = derive_kek(&canonical, &slot.kdf) else {
            continue;
        };
        let aad = slot_aad(&parsed.header, slot)?;
        if let Ok(key) = unwrap_vault_key(&kek, &slot.wrap, &aad) {
            vault_key = Some(key);
            break;
        }
    }

    let vault_key = vault_key.ok_or(VaultError::InvalidCredentials)?;
    let plaintext = open_payload(
        &vault_key,
        parsed.header.cipher,
        &header_core_aad(&parsed.header),
        &parsed.payload,
    )?;
    let data: VaultData = serde_json::from_slice(&plaintext).map_err(|_| VaultError::Serde)?;
    if data.id != parsed.header.vault_id {
        return Err(VaultError::VaultIdMismatch);
    }

    let session = VaultSession {
        id: Uuid::new_v4(),
        header: parsed.header,
        vault_key,
        payload: parsed.payload,
    };

    Ok((session, data))
}

pub fn save_vault(
    session: &mut VaultSession,
    data: &VaultData,
) -> Result<Vec<u8>, VaultError> {
    if data.id != session.header.vault_id {
        return Err(VaultError::VaultIdMismatch);
    }

    let plaintext = serde_json::to_vec(data).map_err(|_| VaultError::Serde)?;
    let payload = seal_payload(
        &session.vault_key,
        session.header.cipher,
        &header_core_aad(&session.header),
        &plaintext,
    )?;
    let file = serialize_file(&session.header, &payload)?;
    session.payload = payload;
    Ok(file)
}

pub fn rewrap_vault(
    session: &mut VaultSession,
    new_secret: &UnlockSecretInput,
    keep_existing: bool,
) -> Result<Vec<u8>, VaultError> {
    let template = session
        .header
        .kdf_slots
        .first()
        .ok_or(VaultError::NoSlots)?
        .kdf
        .clone();
    let kind = new_secret.kind();
    let new_slot = make_slot(
        &session.header,
        kind,
        new_secret,
        &template,
        &session.vault_key,
    )?;

    let mut header = session.header.clone();
    if keep_existing {
        if let Some(existing) = header.kdf_slots.iter_mut().find(|slot| slot.kind == kind) {
            *existing = new_slot;
        } else {
            header.kdf_slots.push(new_slot);
        }
    } else {
        header.kdf_slots = vec![new_slot];
    }

    if header.kdf_slots.is_empty() {
        return Err(VaultError::NoSlots);
    }

    let file = serialize_file(&header, &session.payload)?;
    session.header = header;
    Ok(file)
}

pub fn remove_slot(
    session: &mut VaultSession,
    kind: KdfSlotKind,
) -> Result<Vec<u8>, VaultError> {
    let mut header = session.header.clone();
    let before = header.kdf_slots.len();
    header.kdf_slots.retain(|slot| slot.kind != kind);

    if header.kdf_slots.len() == before {
        return Err(VaultError::SlotNotFound);
    }
    if header.kdf_slots.is_empty() {
        return Err(VaultError::LastUnlockMethod);
    }

    let file = serialize_file(&header, &session.payload)?;
    session.header = header;
    Ok(file)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn empty_vault_data(name: &str, vault_id: Uuid, timestamp: &str) -> VaultData {
    VaultData {
        schema_version: 1,
        id: vault_id,
        name: name.to_string(),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
        settings: Default::default(),
        entries: Vec::new(),
        ssh_keys: Vec::new(),
        attachments: Vec::new(),
        drafts: Vec::new(),
    }
}

fn serialize_file(header: &VaultHeader, payload: &[u8]) -> Result<Vec<u8>, VaultError> {
    serialize_vault_file(&VaultFile {
        header: header.clone(),
        payload: payload.to_vec(),
    })
}

fn make_slot(
    header: &VaultHeader,
    kind: KdfSlotKind,
    secret: &UnlockSecretInput,
    template: &super::models::KdfParams,
    vault_key: &[u8; 32],
) -> Result<KdfSlot, VaultError> {
    let salt = generate_salt();
    let kdf = super::models::KdfParams {
        algo: template.algo.clone(),
        m_kib: template.m_kib,
        t: template.t,
        p: template.p,
        salt: super::crypto::kdf_params(KdfProfile::Standard, salt).salt,
    };
    let canonical = canonical_secret(secret)?;
    let kek = derive_kek(&canonical, &kdf)?;

    let mut slot = KdfSlot {
        id: Uuid::new_v4(),
        kind,
        kdf,
        wrap: super::models::WrapData {
            nonce: String::new(),
            ciphertext: String::new(),
        },
    };
    let aad = slot_aad(header, &slot)?;
    slot.wrap = wrap_vault_key(&kek, vault_key, &aad)?;
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        CipherSuite, Draft, Entry, EntryKind, HistoryItem, SshKey, TotpAlgorithm, TotpConfig,
    };
    use crate::core::vault_file::{parse_vault_file, serialize_vault_file};

    fn passphrase(value: &str) -> UnlockSecretInput {
        UnlockSecretInput::Passphrase {
            value: value.to_string(),
        }
    }

    fn keyfile(bytes: &[u8]) -> UnlockSecretInput {
        UnlockSecretInput::Keyfile {
            bytes: bytes.to_vec(),
            passphrase: None,
        }
    }

    fn config(name: &str) -> CreateVaultConfig {
        CreateVaultConfig {
            name: name.to_string(),
            kdf_profile: KdfProfile::Standard,
            cipher: CipherSuite::XChaCha20Poly1305,
        }
    }

    fn sample_entry(name: &str) -> Entry {
        let now = now();
        Entry {
            id: Uuid::new_v4(),
            kind: EntryKind::Login,
            schema_version: 1,
            name: name.to_string(),
            login: "alex".to_string(),
            password: "s3cret".to_string(),
            url: "https://example.com".to_string(),
            comment: "заметка".to_string(),
            totp: Some(TotpConfig {
                secret: "JBSWY3DPEHPK3PXP".to_string(),
                algorithm: TotpAlgorithm::Sha1,
                digits: 6,
                period: 30,
                issuer: Some("Example".to_string()),
                account: Some("alex".to_string()),
            }),
            attachment_ids: Vec::new(),
            ssh_key_ids: Vec::new(),
            login_history: vec![HistoryItem {
                value: "old-login".to_string(),
                changed_at: now.clone(),
            }],
            password_history: vec![HistoryItem {
                value: "old-password".to_string(),
                changed_at: now.clone(),
            }],
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn create_then_unlock_returns_same_data() {
        let (file, session, data) = create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();

        assert_eq!(data.name, "Личное");
        assert_eq!(data.id, session.header.vault_id);
        assert_eq!(session.header.kdf_slots.len(), 1);
        assert_eq!(session.header.kdf_slots[0].kind, KdfSlotKind::Passphrase);
        assert_eq!(session.header.kdf_slots[0].kdf.m_kib, 65_536);

        let (unlocked_session, unlocked) =
            unlock_vault(&file, &passphrase("correct horse")).unwrap();
        assert_eq!(unlocked, data);
        assert_eq!(unlocked_session.header, session.header);
    }

    #[test]
    fn unlock_with_wrong_passphrase_fails() {
        let (file, _, _) = create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();

        assert!(matches!(
            unlock_vault(&file, &passphrase("wrong horse")),
            Err(VaultError::InvalidCredentials)
        ));
    }

    #[test]
    fn unlock_with_keyfile_works() {
        let secret = keyfile(b"my-keyfile-bytes");
        let (file, _, data) = create_vault(&config("Keyfile"), &secret).unwrap();

        let (_, unlocked) = unlock_vault(&file, &secret).unwrap();
        assert_eq!(unlocked, data);
        assert!(matches!(
            unlock_vault(&file, &keyfile(b"other-keyfile")),
            Err(VaultError::InvalidCredentials)
        ));
    }

    #[test]
    fn unlock_with_encrypted_ssh_keyfile_works() {
        use ssh_key::LineEnding;

        let mut rng = rand::rngs::OsRng;
        let ssh_key = ssh_key::PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();
        let encrypted = ssh_key
            .encrypt(&mut rng, "ssh-pass")
            .unwrap()
            .to_openssh(LineEnding::LF)
            .unwrap()
            .to_string();

        let secret = UnlockSecretInput::Keyfile {
            bytes: encrypted.into_bytes(),
            passphrase: Some("ssh-pass".to_string()),
        };
        let (file, _, data) = create_vault(&config("SSH keyfile"), &secret).unwrap();
        let (_, unlocked) = unlock_vault(&file, &secret).unwrap();
        assert_eq!(unlocked, data);

        assert!(matches!(
            unlock_vault(
                &file,
                &UnlockSecretInput::Keyfile {
                    bytes: match secret.clone() {
                        UnlockSecretInput::Keyfile { bytes, .. } => bytes,
                        _ => unreachable!(),
                    },
                    passphrase: Some("wrong".to_string()),
                }
            ),
            Err(VaultError::KeyfilePassphraseInvalid)
        ));
    }

    #[test]
    fn save_updates_payload_and_preserves_entries() {
        let (_, mut session, mut data) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        data.entries.push(sample_entry("GitHub"));
        data.updated_at = now();

        let saved = save_vault(&mut session, &data).unwrap();
        let (_, unlocked) = unlock_vault(&saved, &passphrase("correct horse")).unwrap();

        assert_eq!(unlocked, data);
    }

    #[test]
    fn rewrap_adds_keyfile_and_keeps_passphrase() {
        let (_, mut session, data) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        let keyfile_secret = keyfile(b"backup-key");

        let rewrapped = rewrap_vault(&mut session, &keyfile_secret, true).unwrap();
        assert_eq!(session.header.kdf_slots.len(), 2);

        let (_, by_keyfile) = unlock_vault(&rewrapped, &keyfile_secret).unwrap();
        let (_, by_pass) = unlock_vault(&rewrapped, &passphrase("correct horse")).unwrap();
        assert_eq!(by_keyfile, data);
        assert_eq!(by_pass, data);
    }

    #[test]
    fn rewrap_changes_passphrase_without_reencrypting_payload() {
        let (_, mut session, mut data) =
            create_vault(&config("Личное"), &passphrase("old pass")).unwrap();
        data.entries.push(sample_entry("GitHub"));
        let saved = save_vault(&mut session, &data).unwrap();
        let original_payload = parse_vault_file(&saved).unwrap().payload;

        let new_pass = passphrase("new pass");
        let rewrapped = rewrap_vault(&mut session, &new_pass, true).unwrap();
        let rewritten = parse_vault_file(&rewrapped).unwrap();

        assert_eq!(rewritten.payload, original_payload);
        assert!(matches!(
            unlock_vault(&rewrapped, &passphrase("old pass")),
            Err(VaultError::InvalidCredentials)
        ));
        let (_, unlocked) = unlock_vault(&rewrapped, &new_pass).unwrap();
        assert_eq!(unlocked, data);
    }

    #[test]
    fn rewrap_can_replace_all_slots() {
        let (_, mut session, _) =
            create_vault(&config("Личное"), &passphrase("old pass")).unwrap();
        let keyfile_secret = keyfile(b"backup-key");
        let with_keyfile = rewrap_vault(&mut session, &keyfile_secret, true).unwrap();
        assert_eq!(session.header.kdf_slots.len(), 2);

        let new_pass = passphrase("new pass");
        let only_pass = rewrap_vault(&mut session, &new_pass, false).unwrap();
        assert_eq!(session.header.kdf_slots.len(), 1);
        assert!(matches!(
            unlock_vault(&only_pass, &keyfile_secret),
            Err(VaultError::InvalidCredentials)
        ));
        let (_, unlocked) = unlock_vault(&only_pass, &new_pass).unwrap();
        assert_eq!(unlocked.name, "Личное");
        let _ = with_keyfile;
    }

    #[test]
    fn remove_last_slot_is_rejected() {
        let (_, mut session, _) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();

        assert_eq!(
            remove_slot(&mut session, KdfSlotKind::Passphrase),
            Err(VaultError::LastUnlockMethod)
        );
    }

    #[test]
    fn remove_one_of_two_slots_works() {
        let (_, mut session, _) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        let keyfile_secret = keyfile(b"backup-key");
        rewrap_vault(&mut session, &keyfile_secret, true).unwrap();

        let file = remove_slot(&mut session, KdfSlotKind::Keyfile).unwrap();
        assert_eq!(session.header.kdf_slots.len(), 1);
        assert!(matches!(
            unlock_vault(&file, &keyfile_secret),
            Err(VaultError::InvalidCredentials)
        ));
        unlock_vault(&file, &passphrase("correct horse")).unwrap();
    }

    #[test]
    fn vault_id_mismatch_is_rejected_on_save() {
        let (_, mut session, mut data) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        data.id = Uuid::new_v4();

        assert_eq!(
            save_vault(&mut session, &data),
            Err(VaultError::VaultIdMismatch)
        );
    }

    #[test]
    fn tampered_header_cipher_fails_unlock() {
        let (file, _, _) = create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        let mut parsed = parse_vault_file(&file).unwrap();
        parsed.header.cipher = CipherSuite::Aes256Gcm;
        let tampered = serialize_vault_file(&parsed).unwrap();

        assert!(unlock_vault(&tampered, &passphrase("correct horse")).is_err());
    }

    #[test]
    fn header_params_from_foreign_vault_are_used() {
        let (file, session, _) =
            create_vault(&config("Личное"), &passphrase("correct horse")).unwrap();
        let parsed = parse_vault_file(&file).unwrap();

        assert_eq!(parsed.header.kdf_slots[0].kdf.m_kib, 65_536);
        assert_eq!(parsed.header, session.header);
        unlock_vault(&file, &passphrase("correct horse")).unwrap();
    }

    #[test]
    fn empty_entry_list_and_settings_roundtrip() {
        let (file, _, data) = create_vault(&config("Пусто"), &passphrase("correct horse")).unwrap();
        let parsed = parse_vault_file(&file).unwrap();
        assert_eq!(parsed.header.vault_id, data.id);
        let (_, unlocked) = unlock_vault(&file, &passphrase("correct horse")).unwrap();
        assert!(unlocked.entries.is_empty());
        assert!(unlocked.ssh_keys.is_empty());
        assert!(unlocked.attachments.is_empty());
        assert!(unlocked.drafts.is_empty());
    }

    #[test]
    fn draft_roundtrip_is_preserved() {
        let (_, mut session, mut data) =
            create_vault(&config("Черновики"), &passphrase("correct horse")).unwrap();
        data.drafts.push(Draft {
            id: "new".to_string(),
            kind: crate::core::models::DraftKind::Entry,
            payload: serde_json::json!({ "name": "Черновик", "password": "secret" }),
            updated_at: now(),
        });
        let saved = save_vault(&mut session, &data).unwrap();
        let (_, unlocked) = unlock_vault(&saved, &passphrase("correct horse")).unwrap();
        assert_eq!(unlocked.drafts, data.drafts);
    }

    #[test]
    fn ssh_key_roundtrip_is_preserved() {
        let (_, mut session, mut data) =
            create_vault(&config("SSH"), &passphrase("correct horse")).unwrap();
        data.ssh_keys.push(SshKey {
            id: Uuid::new_v4(),
            name: "GitHub".to_string(),
            key_type: crate::core::models::SshKeyType::Ed25519,
            rsa_bits: None,
            curve: None,
            public_key: "ssh-ed25519 AAAA test".to_string(),
            private_key: "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
            passphrase: None,
            comment: "test".to_string(),
            attachment_ids: Vec::new(),
            created_at: now(),
            updated_at: now(),
        });
        let saved = save_vault(&mut session, &data).unwrap();
        let (_, unlocked) = unlock_vault(&saved, &passphrase("correct horse")).unwrap();
        assert_eq!(unlocked.ssh_keys.len(), 1);
    }
}
