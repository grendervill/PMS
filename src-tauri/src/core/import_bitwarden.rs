use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use aes::Aes256;
use base64::Engine as _;
use cbc::Decryptor;
use cipher::block_padding::Pkcs7;
use cipher::{BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;

use super::error::VaultError;
use super::import::{
    cow_str, trim_cow, ImportBundle, ImportedAttachment, ImportedItem, ImportedSshKey, Text,
};
use super::totp;

type HmacSha256 = Hmac<Sha256>;

/// KDF limits for imported Bitwarden exports. The values below are read from
/// the file, i.e. from a potential attacker: an export claiming 8 GiB of Argon2
/// memory or 10^10 PBKDF2 iterations would hang or abort the app.
const MAX_KDF_MEMORY_KIB: u64 = 1024 * 1024; // 1 GiB
const MAX_PBKDF2_ITERATIONS: u64 = 10_000_000;
const MAX_ARGON2_ITERATIONS: u64 = 64;
/// `memory (KiB) * iterations`: Bitwarden's default (64 MiB, 3 passes) is ~196k,
/// so this leaves an order of magnitude of headroom. Checking the product
/// matters because `t` alone says nothing: 1 GiB x 10^7 passes is years of work.
const MAX_KDF_WORK: u64 = 2_000_000;

/// Validates the KDF parameters taken from the export and reports whether they
/// are for Argon2 (`true`) or PBKDF2 (`false`), because the iteration budget
/// differs by three orders of magnitude.
fn checked_kdf_params(
    kdf_type: u64,
    iterations: Option<u64>,
    memory_kib: Option<u64>,
) -> Result<(u32, u32), VaultError> {
    let iterations = iterations.unwrap_or(600_000);
    let memory_kib = memory_kib.unwrap_or(64);
    if iterations == 0 || memory_kib > MAX_KDF_MEMORY_KIB {
        return Err(VaultError::ImportCorrupt);
    }
    let too_much = match kdf_type {
        0 => iterations > MAX_PBKDF2_ITERATIONS,
        1 => {
            iterations > MAX_ARGON2_ITERATIONS
                || memory_kib.saturating_mul(iterations) > MAX_KDF_WORK
        }
        _ => true,
    };
    if too_much {
        return Err(VaultError::ImportCorrupt);
    }
    // Reject values that do not fit instead of truncating them silently: a
    // wrapped cast would turn a wrong password into "corrupt export".
    let iterations = u32::try_from(iterations).map_err(|_| VaultError::ImportCorrupt)?;
    let memory_kib = u32::try_from(memory_kib).map_err(|_| VaultError::ImportCorrupt)?;
    Ok((iterations, memory_kib))
}

// ---------------------------------------------------------------- decryption

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
    let mut parts = rest.split('|');
    let (Some(iv), Some(ciphertext), Some(mac_value)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(VaultError::ImportCorrupt);
    };
    let decode = |part: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(part)
            .map_err(|_| VaultError::ImportCorrupt)
    };
    Ok((decode(iv)?, decode(ciphertext)?, decode(mac_value)?))
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

/// Returns the plain export. Unencrypted exports are borrowed straight from the
/// input, so the parser never copies the file.
fn decrypt_export<'a>(
    bytes: &'a [u8],
    password: Option<&str>,
) -> Result<Cow<'a, [u8]>, VaultError> {
    #[derive(Deserialize)]
    struct Envelope<'a> {
        #[serde(default)]
        encrypted: bool,
        #[serde(default, rename = "passwordProtected")]
        password_protected: bool,
        #[serde(default)]
        salt: Option<Cow<'a, str>>,
        #[serde(default, rename = "kdfType")]
        kdf_type: u64,
        #[serde(default, rename = "kdfIterations")]
        kdf_iterations: Option<u64>,
        #[serde(default, rename = "kdfMemory")]
        kdf_memory: Option<u64>,
        #[serde(default, rename = "kdfParallelism")]
        kdf_parallelism: Option<u64>,
        #[serde(default, rename = "encKeyValidation_DO_NOT_EDIT")]
        validation: Option<Cow<'a, str>>,
        #[serde(default)]
        data: Option<Cow<'a, str>>,
    }

    let envelope: Envelope<'a> =
        serde_json::from_slice(bytes).map_err(|_| VaultError::ImportCorrupt)?;
    if !envelope.encrypted {
        return Ok(Cow::Borrowed(bytes));
    }
    if !envelope.password_protected {
        return Err(VaultError::ImportUnsupported);
    }
    let password = password.ok_or(VaultError::ImportPasswordRequired)?;
    let salt = envelope.salt.as_deref().ok_or(VaultError::ImportCorrupt)?;
    let (iterations, memory_kib) = checked_kdf_params(
        envelope.kdf_type,
        envelope.kdf_iterations,
        envelope.kdf_memory,
    )?;

    let mut master = [0u8; 32];
    match envelope.kdf_type {
        0 => pbkdf2_hmac::<Sha256>(
            password.as_bytes(),
            salt.as_bytes(),
            iterations,
            &mut master,
        ),
        1 => {
            let parallelism = envelope.kdf_parallelism.unwrap_or(4).clamp(1, 8) as u32;
            let params = argon2::Params::new(
                memory_kib.saturating_mul(1024),
                iterations,
                parallelism,
                Some(32),
            )
            .map_err(|_| VaultError::ImportCorrupt)?;
            let argon =
                argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
            argon
                .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut master)
                .map_err(|_| VaultError::ImportPasswordInvalid)?;
        }
        _ => return Err(VaultError::ImportUnsupported),
    }

    let enc_key = hkdf_expand(&master, b"enc", 32)?;
    let mac_key = hkdf_expand(&master, b"mac", 32)?;
    if let Some(validation) = envelope.validation.as_deref() {
        decrypt_enc(validation, &enc_key, &mac_key)?;
    }
    let data = envelope.data.as_deref().ok_or(VaultError::ImportCorrupt)?;
    decrypt_enc(data, &enc_key, &mac_key).map(Cow::Owned)
}

// ------------------------------------------------------------ export schema

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Export<'a> {
    #[serde(default)]
    folders: Vec<Folder<'a>>,
    items: Vec<Item<'a>>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Folder<'a> {
    #[serde(default, deserialize_with = "cow_str")]
    id: Cow<'a, str>,
    #[serde(default, deserialize_with = "cow_str")]
    name: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Item<'a> {
    #[serde(rename = "type", default)]
    item_type: u64,
    #[serde(default, deserialize_with = "cow_str")]
    id: Cow<'a, str>,
    #[serde(default, deserialize_with = "cow_str")]
    name: Cow<'a, str>,
    #[serde(default, deserialize_with = "cow_str")]
    notes: Cow<'a, str>,
    #[serde(default, rename = "folderId", deserialize_with = "cow_str")]
    folder_id: Cow<'a, str>,
    #[serde(default)]
    fields: Vec<Field<'a>>,
    #[serde(default)]
    login: Option<Login<'a>>,
    #[serde(default, rename = "passwordHistory")]
    password_history: Vec<PasswordHistory<'a>>,
    #[serde(default)]
    attachments: Vec<AttachmentRef<'a>>,
    #[serde(default, rename = "sshKey")]
    ssh_key: Option<SshKeyMaterial<'a>>,
    /// Card and identity payloads are free-form objects; only these two item
    /// types pay for a `serde_json::Map`.
    #[serde(default)]
    card: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    identity: Option<serde_json::Map<String, Value>>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Field<'a> {
    #[serde(default, deserialize_with = "cow_str")]
    name: Cow<'a, str>,
    #[serde(default)]
    value: FieldValue<'a>,
}

#[derive(Default)]
enum FieldValue<'a> {
    #[default]
    Empty,
    Text(Cow<'a, str>),
    Bool(bool),
    Number(f64),
}

impl FieldValue<'_> {
    fn append_to(&self, out: &mut String) {
        match self {
            FieldValue::Empty => {}
            FieldValue::Text(value) => out.push_str(value),
            FieldValue::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            FieldValue::Number(value) => {
                if value.fract() == 0.0 {
                    out.push_str(&format!("{}", *value as i64));
                } else {
                    out.push_str(&format!("{value}"));
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            FieldValue::Empty => true,
            FieldValue::Text(value) => value.trim().is_empty(),
            _ => false,
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for FieldValue<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FieldValueVisitor;

        impl<'de> Visitor<'de> for FieldValueVisitor {
            type Value = FieldValue<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string, number, boolean or { concealed } object")
            }

            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(FieldValue::Text(Cow::Borrowed(value)))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(FieldValue::Text(Cow::Owned(value.to_owned())))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(FieldValue::Text(Cow::Owned(value)))
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(FieldValue::Bool(value))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(FieldValue::Number(value as f64))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(FieldValue::Number(value as f64))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(FieldValue::Number(value))
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(FieldValue::Empty)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(FieldValue::Empty)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut value = FieldValue::Empty;
                while let Some(key) = map.next_key::<Cow<'de, str>>()? {
                    if key == "concealed" || key == "value" {
                        let text: Option<Text<'de>> = map.next_value()?;
                        if let Some(text) = text {
                            value = FieldValue::Text(text.into_cow());
                        }
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }

        deserializer.deserialize_any(FieldValueVisitor)
    }
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Login<'a> {
    #[serde(default, deserialize_with = "cow_str")]
    username: Cow<'a, str>,
    #[serde(default, deserialize_with = "cow_str")]
    password: Cow<'a, str>,
    #[serde(default, deserialize_with = "cow_str")]
    totp: Cow<'a, str>,
    #[serde(default)]
    uris: Vec<Uri<'a>>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct Uri<'a> {
    #[serde(default, deserialize_with = "cow_str")]
    uri: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct PasswordHistory<'a> {
    #[serde(default, deserialize_with = "cow_str")]
    password: Cow<'a, str>,
    #[serde(default, rename = "lastUsedDate", deserialize_with = "cow_str")]
    last_used_date: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct AttachmentRef<'a> {
    #[serde(default, rename = "fileName", deserialize_with = "cow_str")]
    file_name: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct SshKeyMaterial<'a> {
    #[serde(default, rename = "publicKey", deserialize_with = "cow_str")]
    public_key: Cow<'a, str>,
    #[serde(default, rename = "privateKey", deserialize_with = "cow_str")]
    private_key: Cow<'a, str>,
}

// ------------------------------------------------------------------- mapping

fn attachment_for<'a>(
    attachments: &HashMap<String, Vec<u8>>,
    item_id: &str,
    name: Cow<'a, str>,
) -> Option<ImportedAttachment<'a>> {
    if attachments.is_empty() {
        return None;
    }
    let normalized = name.to_ascii_lowercase();
    let item_lower = item_id.to_ascii_lowercase();
    let matches = |(path, _): &(&String, &Vec<u8>)| {
        let lower = path.to_ascii_lowercase();
        lower.ends_with(&normalized) && (item_lower.is_empty() || lower.contains(&item_lower))
    };
    let by_name =
        |(path, _): &(&String, &Vec<u8>)| path.to_ascii_lowercase().ends_with(&normalized);
    let bytes = attachments
        .iter()
        .find(matches)
        .or_else(|| attachments.iter().find(by_name))
        .map(|(_, bytes)| bytes)?;
    Some(ImportedAttachment {
        name,
        mime: Cow::Borrowed("application/octet-stream"),
        b64: Cow::Owned(base64::engine::general_purpose::STANDARD.encode(bytes)),
    })
}

pub fn parse<'a>(
    bytes: &'a [u8],
    password: Option<&str>,
    attachments: &HashMap<String, Vec<u8>>,
) -> Result<ImportBundle<'a>, VaultError> {
    match decrypt_export(bytes, password)? {
        Cow::Borrowed(plain) => parse_plain(plain, attachments),
        Cow::Owned(plain) => Ok(parse_plain(&plain, attachments)?.into_owned()),
    }
}

fn parse_plain<'a>(
    plain: &'a [u8],
    attachments: &HashMap<String, Vec<u8>>,
) -> Result<ImportBundle<'a>, VaultError> {
    let export: Export<'a> =
        serde_json::from_slice(plain).map_err(|_| VaultError::ImportCorrupt)?;
    let folders = export
        .folders
        .iter()
        .filter(|folder| !folder.id.is_empty())
        .map(|folder| (folder.id.as_ref(), folder.name.as_ref()))
        .collect::<HashMap<_, _>>();
    let mut imported = Vec::with_capacity(export.items.len());
    let mut warnings = Vec::new();

    for item in export.items {
        let item_type = item.item_type;
        let name = trim_cow(item.name);
        if name.is_empty() {
            warnings.push(Cow::Borrowed("Bitwarden: запись без имени"));
        }
        // The comment stays a borrow of the export until there is something to
        // add; when there is, it is built in one pre-sized buffer so the string
        // never reallocates while it grows.
        let mut comment = trim_cow(item.notes);
        let folder = folders
            .get(item.folder_id.as_ref())
            .filter(|folder| !folder.is_empty());
        let fields = item
            .fields
            .iter()
            .filter(|field| !field.name.trim().is_empty() && !field.value.is_empty());
        if folder.is_some() || fields.clone().next().is_some() {
            let mut buffer = String::with_capacity(comment.len() + 64 + item.fields.len() * 48);
            if let Some(folder) = folder {
                buffer.push_str("Папка: ");
                buffer.push_str(folder);
                if !comment.is_empty() {
                    buffer.push('\n');
                }
            }
            buffer.push_str(&comment);
            for field in fields {
                if !buffer.is_empty() {
                    buffer.push('\n');
                }
                buffer.push_str(field.name.trim());
                buffer.push_str(": ");
                field.value.append_to(&mut buffer);
            }
            comment = Cow::Owned(buffer);
        }

        if item_type == 5 {
            if let Some(ssh) = item.ssh_key {
                let public_key = trim_cow(ssh.public_key);
                let private_key = trim_cow(ssh.private_key);
                if !public_key.is_empty() && !private_key.is_empty() {
                    let mut ssh_item = ImportedItem::new(if name.is_empty() {
                        Cow::Borrowed("SSH-ключ")
                    } else {
                        name.clone()
                    });
                    ssh_item.kind = Cow::Borrowed("ssh");
                    ssh_item.ssh_key = Some(ImportedSshKey {
                        name: ssh_item.name.clone(),
                        public_key,
                        private_key,
                        passphrase: None,
                        comment: trim_cow(comment),
                    });
                    imported.push(ssh_item);
                    continue;
                }
            }
            warnings.push(Cow::Borrowed("Bitwarden: SSH-ключ без ключевого материала"));
            continue;
        }

        let mut entry = ImportedItem::new(if name.is_empty() {
            Cow::Borrowed("Без названия")
        } else {
            name.clone()
        });
        entry.comment = comment;
        match item_type {
            2 => entry.kind = Cow::Borrowed("note"),
            3 => {
                entry.kind = Cow::Borrowed("card");
                append_object(&mut entry, item.card);
            }
            4 => {
                entry.kind = Cow::Borrowed("identity");
                append_object(&mut entry, item.identity);
            }
            _ => {
                if let Some(login) = item.login {
                    entry.login = login.username;
                    entry.password = login.password;
                    entry.urls = Vec::with_capacity(login.uris.len());
                    for uri in login.uris {
                        let uri = trim_cow(uri.uri);
                        if !uri.is_empty() {
                            entry.urls.push(uri);
                        }
                    }
                    let totp_raw = login.totp.trim();
                    if !totp_raw.is_empty() {
                        match totp::inspect(totp_raw) {
                            Ok(config) => entry.totp = Some(config),
                            Err(_) => {
                                warnings.push(Cow::Owned(format!("{name}: TOTP не распознан")))
                            }
                        }
                    }
                }
            }
        }

        entry.password_history.reserve(item.password_history.len());
        for old in item.password_history {
            let password = trim_cow(old.password);
            if password.is_empty() {
                continue;
            }
            let changed_at = trim_cow(old.last_used_date);
            entry.push_password_history(password, (!changed_at.is_empty()).then_some(changed_at));
        }

        if !item.attachments.is_empty() {
            let item_id = item.id.as_ref();
            entry.attachments.reserve(item.attachments.len());
            for attachment in item.attachments {
                let file_name = trim_cow(attachment.file_name);
                if file_name.is_empty() {
                    continue;
                }
                match attachment_for(attachments, item_id, file_name.clone()) {
                    Some(file) => entry.attachments.push(file),
                    None => warnings.push(Cow::Owned(format!(
                        "{name}: вложение «{file_name}» отсутствует в архиве"
                    ))),
                }
            }
        }

        imported.push(entry);
    }

    if imported.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: Cow::Borrowed("bitwarden"),
        items: imported,
        warnings,
    })
}

fn append_object(entry: &mut ImportedItem<'_>, object: Option<serde_json::Map<String, Value>>) {
    let Some(object) = object else { return };
    let mut buffer = String::with_capacity(entry.comment.len() + object.len() * 48);
    buffer.push_str(&entry.comment);
    for (key, value) in object {
        let text = match value {
            Value::String(value) => value,
            Value::Number(value) => value.to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Object(map) => map
                .get("concealed")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            _ => continue,
        };
        if text.is_empty() {
            continue;
        }
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&key);
        buffer.push_str(": ");
        buffer.push_str(&text);
    }
    entry.comment = Cow::Owned(buffer);
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_kdf_parameter_bombs() {
        // PBKDF2 with too many rounds would take hours.
        assert_eq!(
            super::checked_kdf_params(0, Some(1_000_000_000), None),
            Err(VaultError::ImportCorrupt)
        );
        // Argon2: individually acceptable values whose product is not.
        assert_eq!(
            super::checked_kdf_params(1, Some(10_000_000), Some(1024 * 1024)),
            Err(VaultError::ImportCorrupt)
        );
        assert_eq!(
            super::checked_kdf_params(1, Some(64), Some(1024 * 1024)),
            Err(VaultError::ImportCorrupt)
        );
        // Values that never fit a u32 must not wrap around silently either.
        assert_eq!(
            super::checked_kdf_params(0, Some(u64::from(u32::MAX) + 1), None),
            Err(VaultError::ImportCorrupt)
        );
        // The defaults and Bitwarden's own profiles are accepted.
        assert_eq!(super::checked_kdf_params(0, None, None), Ok((600_000, 64)));
        assert_eq!(
            super::checked_kdf_params(1, Some(3), Some(65_536)),
            Ok((3, 65_536))
        );
    }

    use super::*;

    #[test]
    fn attaches_bitwarden_zip_files_by_item_and_name() {
        let json = br#"{"items":[{"id":"i1","type":1,"name":"With file","login":{},"attachments":[{"id":"a1","fileName":"doc.txt"}]}]}"#;
        let mut files = HashMap::new();
        files.insert("attachments/i1/doc.txt".to_string(), b"hello".to_vec());
        let bundle = parse(json, None, &files).unwrap();
        assert_eq!(bundle.items[0].attachments.len(), 1);
        assert_eq!(bundle.items[0].attachments[0].name, "doc.txt");
    }

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
    fn borrows_strings_from_the_export() {
        let json =
            br#"{"items":[{"id":"i1","type":1,"name":"GitHub","login":{"username":"alex"}}]}"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        assert!(matches!(bundle.items[0].name, Cow::Borrowed(_)));
        assert!(matches!(bundle.items[0].login, Cow::Borrowed(_)));
    }

    #[test]
    fn parses_plain_bitwarden_json() {
        let json = br#"{
          "folders": [{"id":"f1","name":"Social"}],
          "items": [{
            "id":"i1","type":1,"name":"GitHub","folderId":"f1",
            "notes":"note","fields":[{"name":"PIN","value":"1234"}],
            "login":{"username":"alex","password":"s3cret","totp":"JBSWY3DPEHPK3PXP",
              "uris":[{"uri":"https://github.com"},{"uri":"https://gist.github.com"}]},
            "passwordHistory":[
              {"lastUsedDate":"2024-02-01T10:00:00.000Z","password":"old-1"},
              {"lastUsedDate":"2023-02-01T10:00:00.000Z","password":"old-2"},
              {"lastUsedDate":"2022-02-01T10:00:00.000Z","password":"old-1"},
              {"lastUsedDate":"2021-02-01T10:00:00.000Z","password":"s3cret"}
            ]
          }]
        }"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.urls.len(), 2);
        assert_eq!(item.login, "alex");
        assert!(item.totp.is_some());
        assert!(item.comment.contains("Папка: Social"));
        assert!(item.comment.contains("PIN: 1234"));
        assert!(!item.comment.contains("Старый пароль"));
        assert!(!item.comment.contains("old-1"));
        assert_eq!(item.password_history.len(), 2);
        assert_eq!(item.password_history[0].value, "old-1");
        assert_eq!(
            item.password_history[0].changed_at.as_deref(),
            Some("2024-02-01T10:00:00.000Z")
        );
        assert_eq!(item.password_history[1].value, "old-2");
    }

    #[test]
    fn keeps_notes_free_of_previous_passwords() {
        let json = br#"{"items":[{"id":"i1","type":1,"name":"Mail","notes":"my note",
          "login":{"username":"alex","password":"current"},
          "passwordHistory":[{"password":"previous"}]}]}"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.comment, "my note");
        assert_eq!(item.password_history.len(), 1);
        assert_eq!(item.password_history[0].value, "previous");
        assert_eq!(item.password_history[0].changed_at, None);
    }

    #[test]
    fn parses_card_and_identity_fields_into_the_comment() {
        let json = br#"{"items":[
          {"id":"c1","type":3,"name":"Visa","card":{"brand":"Visa","number":{"concealed":"4242","length":16}}},
          {"id":"d1","type":4,"name":"Passport","identity":{"firstName":"Alex","country":"RU"}}
        ]}"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        assert_eq!(bundle.items[0].kind, "card");
        assert!(bundle.items[0].comment.contains("brand: Visa"));
        assert!(bundle.items[0].comment.contains("number: 4242"));
        assert_eq!(bundle.items[1].kind, "identity");
        assert!(bundle.items[1].comment.contains("firstName: Alex"));
    }

    #[test]
    fn parses_numeric_and_concealed_custom_fields() {
        let json = br#"{"items":[{"id":"i1","type":1,"name":"N","login":{},"fields":[
            {"name":"PIN","value":1234},
            {"name":"Secret","value":{"concealed":"s3cret"}},
            {"name":"Flag","value":true},
            {"name":"Empty","value":null}
        ]}]}"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        let comment = bundle.items[0].comment.as_ref();
        assert!(comment.contains("PIN: 1234"), "{comment}");
        assert!(comment.contains("Secret: s3cret"), "{comment}");
        assert!(comment.contains("Flag: true"), "{comment}");
        assert!(!comment.contains("Empty"), "{comment}");
    }

    #[test]
    fn keeps_unnamed_items_and_warns() {
        let json = br#"{"items":[{"id":"i1","type":1,"login":{}}]}"#;
        let bundle = parse(json, None, &HashMap::new()).unwrap();
        assert_eq!(bundle.items[0].name, "Без названия");
        assert!(!bundle.warnings.is_empty());
    }
}
