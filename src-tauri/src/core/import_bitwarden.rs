use base64::Engine as _;
use std::collections::HashMap;

use aes::Aes256;
use cbc::Decryptor;
use cipher::block_padding::Pkcs7;
use cipher::{BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde_json::Value;
use sha2::Sha256;

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem, ImportedSshKey};
use super::totp;

type HmacSha256 = Hmac<Sha256>;

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Object(map) => map
            .get("concealed")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn hkdf_expand(prk: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, VaultError> {
    let mut output = Vec::with_capacity(length);
    let mut previous = Vec::new();
    let mut counter = 1u8;
    while output.len() < length {
        let mut mac = HmacSha256::new_from_slice(prk).map_err(|_| VaultError::ImportCorrupt)?;
        mac.update(&previous);
        mac.update(info);
        mac.update(&[counter]);
        previous = mac.finalize().into_bytes().to_vec();
        output.extend_from_slice(&previous);
        counter = counter.wrapping_add(1);
    }
    output.truncate(length);
    Ok(output)
}

type EncParts = (Vec<u8>, Vec<u8>, Vec<u8>);

fn parse_enc_string(value: &str) -> Result<EncParts, VaultError> {
    let (kind, rest) = value.split_once('.').ok_or(VaultError::ImportCorrupt)?;
    if kind != "2" {
        return Err(VaultError::ImportUnsupported);
    }
    let parts = rest.split('|').collect::<Vec<_>>();
    if parts.len() < 3 {
        return Err(VaultError::ImportCorrupt);
    }
    let decode = |part: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(part)
            .map_err(|_| VaultError::ImportCorrupt)
    };
    Ok((decode(parts[0])?, decode(parts[1])?, decode(parts[2])?))
}

fn decrypt_enc(value: &str, enc_key: &[u8], mac_key: &[u8]) -> Result<Vec<u8>, VaultError> {
    let (iv, ciphertext, mac_value) = parse_enc_string(value)?;
    let mut mac = HmacSha256::new_from_slice(mac_key).map_err(|_| VaultError::ImportCorrupt)?;
    mac.update(&iv);
    mac.update(&ciphertext);
    mac.verify_slice(&mac_value)
        .map_err(|_| VaultError::ImportPasswordInvalid)?;

    let decryptor = Decryptor::<Aes256>::new_from_slices(enc_key, &iv)
        .map_err(|_| VaultError::ImportCorrupt)?;
    decryptor
        .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
        .map_err(|_| VaultError::ImportPasswordInvalid)
}

fn decrypt_export(bytes: &[u8], password: Option<&str>) -> Result<Vec<u8>, VaultError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| VaultError::ImportCorrupt)?;
    if root.get("encrypted").and_then(Value::as_bool) != Some(true) {
        return Ok(bytes.to_vec());
    }
    if root.get("passwordProtected").and_then(Value::as_bool) != Some(true) {
        return Err(VaultError::ImportUnsupported);
    }
    let password = password.ok_or(VaultError::ImportPasswordRequired)?;
    let salt = root
        .get("salt")
        .and_then(Value::as_str)
        .ok_or(VaultError::ImportCorrupt)?;
    let kdf_type = root.get("kdfType").and_then(Value::as_u64).unwrap_or(0);
    let iterations = root
        .get("kdfIterations")
        .and_then(Value::as_u64)
        .unwrap_or(600_000) as u32;

    let mut master = [0u8; 32];
    if kdf_type == 0 {
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iterations, &mut master);
    } else if kdf_type == 1 {
        let memory = root.get("kdfMemory").and_then(Value::as_u64).unwrap_or(64) as u32;
        let parallelism = root
            .get("kdfParallelism")
            .and_then(Value::as_u64)
            .unwrap_or(4) as u32;
        let params = argon2::Params::new(memory * 1024, iterations, parallelism, Some(32))
            .map_err(|_| VaultError::ImportCorrupt)?;
        let argon = argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            params,
        );
        argon
            .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut master)
            .map_err(|_| VaultError::ImportPasswordInvalid)?;
    } else {
        return Err(VaultError::ImportUnsupported);
    }

    let enc_key = hkdf_expand(&master, b"enc", 32)?;
    let mac_key = hkdf_expand(&master, b"mac", 32)?;
    if let Some(validation) = root.get("encKeyValidation_DO_NOT_EDIT").and_then(Value::as_str) {
        decrypt_enc(validation, &enc_key, &mac_key)?;
    }
    let data = root
        .get("data")
        .and_then(Value::as_str)
        .ok_or(VaultError::ImportCorrupt)?;
    decrypt_enc(data, &enc_key, &mac_key)
}

fn attachment_for(
    attachments: &HashMap<String, Vec<u8>>,
    item_id: &str,
    name: &str,
) -> Option<ImportedAttachment> {
    let normalized = name.to_ascii_lowercase();
    for (path, bytes) in attachments {
        let lower = path.to_ascii_lowercase();
        let matches_name = lower.ends_with(&normalized);
        let matches_item = item_id.is_empty() || lower.contains(&item_id.to_ascii_lowercase());
        if matches_name && matches_item {
            return Some(ImportedAttachment {
                name: name.to_string(),
                mime: "application/octet-stream".to_string(),
                b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
        }
    }
    for (path, bytes) in attachments {
        if path.to_ascii_lowercase().ends_with(&normalized) {
            return Some(ImportedAttachment {
                name: name.to_string(),
                mime: "application/octet-stream".to_string(),
                b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
        }
    }
    None
}

pub fn parse(
    bytes: &[u8],
    password: Option<&str>,
    attachments: &HashMap<String, Vec<u8>>,
) -> Result<ImportBundle, VaultError> {
    let plaintext = decrypt_export(bytes, password)?;
    let root: Value = serde_json::from_slice(&plaintext).map_err(|_| VaultError::ImportCorrupt)?;
    let folders = root
        .get("folders")
        .and_then(Value::as_array)
        .map(|folders| {
            folders
                .iter()
                .filter_map(|folder| {
                    Some((
                        folder.get("id")?.as_str()?.to_string(),
                        folder.get("name")?.as_str()?.to_string(),
                    ))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let items = root
        .get("items")
        .and_then(Value::as_array)
        .ok_or(VaultError::ImportCorrupt)?;
    let mut imported = Vec::new();
    let mut warnings = Vec::new();

    for item in items {
        let item_type = item.get("type").and_then(Value::as_u64).unwrap_or(1);
        let name = text(item.get("name"));
        if name.is_empty() {
            warnings.push("Bitwarden: запись без имени".to_string());
        }
        let notes = text(item.get("notes"));
        let mut comment = notes;
        if let Some(folder_id) = item.get("folderId").and_then(Value::as_str) {
            if let Some(folder) = folders.get(folder_id) {
                comment = format!("Папка: {folder}\n{comment}");
            }
        }
        if let Some(fields) = item.get("fields").and_then(Value::as_array) {
            for field in fields {
                let label = text(field.get("name"));
                let value = value_text(field.get("value").unwrap_or(&Value::Null));
                if !label.is_empty() && !value.is_empty() {
                    comment.push_str(&format!("\n{label}: {value}"));
                }
            }
        }

        if item_type == 5 {
            if let Some(ssh) = item.get("sshKey") {
                let public_key = text(ssh.get("publicKey"));
                let private_key = text(ssh.get("privateKey"));
                if !public_key.is_empty() && !private_key.is_empty() {
                    let mut ssh_item = ImportedItem::new(if name.is_empty() {
                        "SSH-ключ".to_string()
                    } else {
                        name
                    });
                    ssh_item.kind = "ssh".to_string();
                    ssh_item.ssh_key = Some(ImportedSshKey {
                        name: ssh_item.name.clone(),
                        public_key,
                        private_key,
                        passphrase: None,
                        comment,
                    });
                    imported.push(ssh_item);
                    continue;
                }
            }
            warnings.push("Bitwarden: SSH-ключ без ключевого материала".to_string());
            continue;
        }

        let mut entry = ImportedItem::new(if name.is_empty() {
            "Без названия".to_string()
        } else {
            name.clone()
        });
        entry.comment = comment.trim().to_string();
        if item_type == 2 {
            entry.kind = "note".to_string();
        } else if item_type == 3 || item_type == 4 {
            entry.kind = if item_type == 3 { "card" } else { "identity" }.to_string();
            if let Some(object) = item.get(if item_type == 3 { "card" } else { "identity" }) {
                if let Some(map) = object.as_object() {
                    for (key, value) in map {
                        let value = value_text(value);
                        if !value.is_empty() {
                            entry.comment.push_str(&format!("\n{key}: {value}"));
                        }
                    }
                }
            }
        } else if let Some(login) = item.get("login") {
            entry.login = text(login.get("username"));
            entry.password = text(login.get("password"));
            if let Some(uris) = login.get("uris").and_then(Value::as_array) {
                for uri in uris {
                    let value = text(uri.get("uri"));
                    if !value.is_empty() {
                        entry.urls.push(value);
                    }
                }
            }
            let totp_raw = text(login.get("totp"));
            if !totp_raw.is_empty() {
                match totp::inspect(&totp_raw) {
                    Ok(config) => entry.totp = Some(config),
                    Err(_) => warnings.push(format!("{name}: TOTP не распознан")),
                }
            }
        }

        if let Some(history) = item.get("passwordHistory").and_then(Value::as_array) {
            for old in history {
                let password = text(old.get("password"));
                if !password.is_empty() {
                    entry.comment.push_str(&format!("\nСтарый пароль: {password}"));
                }
            }
        }

        if let Some(meta) = item.get("attachments").and_then(Value::as_array) {
            let item_id = text(item.get("id"));
            for attachment in meta {
                let file_name = text(attachment.get("fileName"));
                if file_name.is_empty() {
                    continue;
                }
                if let Some(file) = attachment_for(attachments, &item_id, &file_name) {
                    entry.attachments.push(file);
                } else {
                    warnings.push(format!("{name}: вложение «{file_name}» отсутствует в архиве"));
                }
            }
        }
        imported.push(entry);
    }

    if imported.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "bitwarden".to_string(),
        items: imported,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypts_password_protected_bitwarden_json() {
        let bytes = include_bytes!("../../tests/fixtures/bitwarden_encrypted.json");
        let bundle = parse(bytes, Some("test-pass"), &HashMap::new()).unwrap();
        assert_eq!(bundle.items[0].name, "Bitwarden import");
        assert_eq!(bundle.items[0].login, "alex");
        assert_eq!(bundle.items[0].urls, vec!["https://bitwarden.example"]);
        assert!(bundle.items[0].totp.is_some());
        assert!(matches!(
            parse(bytes, Some("wrong"), &HashMap::new()),
            Err(VaultError::ImportPasswordInvalid)
        ));
    }

    #[test]
    fn parses_plain_bitwarden_json() {
        let json = br#"{
          "folders": [{"id":"f1","name":"Social"}],
          "items": [{
            "id":"i1","type":1,"name":"GitHub","folderId":"f1",
            "notes":"note","fields":[{"name":"PIN","value":"1234"}],
            "login":{"username":"alex","password":"s3cret","totp":"JBSWY3DPEHPK3PXP",
              "uris":[{"uri":"https://github.com"},{"uri":"https://gist.github.com"}]}
          }]
        }"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.urls.len(), 2);
        assert_eq!(item.login, "alex");
        assert!(item.totp.is_some());
        assert!(item.comment.contains("Папка: Social"));
        assert!(item.comment.contains("PIN: 1234"));
    }
}
