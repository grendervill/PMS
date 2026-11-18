use base64::Engine as _;
use std::borrow::Cow;
use std::io::Cursor;

use keepass::{Database, DatabaseKey};

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem};
use super::totp;

/// Upper bounds for the key-derivation parameters found in a KDBX header.
///
/// Both values come from the file, i.e. from a potential attacker: without a
/// cap a 2 KiB "database" can ask for 8 GiB of Argon2 memory or 2^32 PBKDF2
/// rounds. `keepass` 0.17 passes them straight into `argon2`, which aborts the
/// process when the allocation fails.
const MAX_KDBX_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_KDBX_ITERATIONS: u64 = 10_000_000;
/// `memory (KiB) * iterations` for the Argon2 KDF: a KDBX header may claim both
/// a gigabyte of memory and millions of passes, which is not a matter of seconds
/// but of years.
const MAX_KDBX_KDF_WORK: u64 = 2_000_000;
const KDBX_SIGNATURE_1: u32 = 0x9AA2_D903;
const KDBX_SIGNATURE_2: u32 = 0xB54B_FB67;
const HEADER_FIELD_END: u8 = 0;
const HEADER_FIELD_ROUNDS_KDBX3: u8 = 6;
const HEADER_FIELD_KDF_KDBX4: u8 = 11;
const VARIANT_DICT_END: u8 = 0x00;
const VARIANT_TYPE_UINT32: u8 = 0x04;
const VARIANT_TYPE_UINT64: u8 = 0x05;

fn read_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn read_u64(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let end = at.checked_add(8)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn read_slice<'a>(bytes: &'a [u8], at: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = at.checked_add(len)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(slice)
}

fn check_limits(memory: Option<u64>, iterations: Option<u64>) -> Result<(), VaultError> {
    if memory.is_some_and(|value| value > MAX_KDBX_MEMORY_BYTES)
        || iterations.is_some_and(|value| value > MAX_KDBX_ITERATIONS)
    {
        return Err(VaultError::ImportCorrupt);
    }
    // Argon2 (KDBX4) reports memory in bytes; the classic KDBX3 transform has
    // no memory cost and only rounds.
    if let (Some(memory), Some(iterations)) = (memory, iterations) {
        if memory > 0 && memory / 1024 * iterations > MAX_KDBX_KDF_WORK {
            return Err(VaultError::ImportCorrupt);
        }
    }
    Ok(())
}

/// Walks the KDBX header (not encrypted) and rejects absurd KDF parameters
/// before `keepass` gets to run them. Anything that cannot be parsed is left to
/// `keepass`: it produces the proper error instead of a resource exhaustion.
fn guard_kdf_params(bytes: &[u8]) -> Result<(), VaultError> {
    let mut at = 0usize;
    let signature_1 = read_u32(bytes, &mut at).ok_or(VaultError::ImportCorrupt)?;
    let signature_2 = read_u32(bytes, &mut at).ok_or(VaultError::ImportCorrupt)?;
    if signature_1 != KDBX_SIGNATURE_1 || signature_2 != KDBX_SIGNATURE_2 {
        return Ok(());
    }
    let version = read_u32(bytes, &mut at).ok_or(VaultError::ImportCorrupt)?;
    let minor = (version >> 16) as u16;

    // The header is small; the bound keeps a malformed file from making us walk
    // the whole body.
    let header_end = at.saturating_add(64 * 1024).min(bytes.len());
    while at + 5 <= header_end {
        let field_id = bytes[at];
        at += 1;
        let size = read_u32(bytes, &mut at).ok_or(VaultError::ImportCorrupt)? as usize;
        let data = read_slice(bytes, &mut at, size).ok_or(VaultError::ImportCorrupt)?;
        if field_id == HEADER_FIELD_END {
            break;
        }
        if minor < 1 && field_id == HEADER_FIELD_ROUNDS_KDBX3 && data.len() == 8 {
            let rounds =
                u64::from_le_bytes(data.try_into().map_err(|_| VaultError::ImportCorrupt)?);
            check_limits(None, Some(rounds))?;
        }
        if minor >= 1 && field_id == HEADER_FIELD_KDF_KDBX4 {
            check_variant_dictionary(data)?;
        }
    }
    Ok(())
}

/// Reads a KDBX4 `VariantDictionary` and checks the Argon2 parameters
/// (`M` = memory in bytes, `I` = iterations).
fn check_variant_dictionary(data: &[u8]) -> Result<(), VaultError> {
    let mut at = 0usize;
    let _version = data
        .get(at..at.saturating_add(2))
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(VaultError::ImportCorrupt)?;
    at += 2;

    let mut memory = None;
    let mut iterations = None;
    while at < data.len() {
        let entry_type = data[at];
        at += 1;
        if entry_type == VARIANT_DICT_END {
            break;
        }
        let key_len = read_u32(data, &mut at).ok_or(VaultError::ImportCorrupt)? as usize;
        let key = read_slice(data, &mut at, key_len).ok_or(VaultError::ImportCorrupt)?;
        let value_len = read_u32(data, &mut at).ok_or(VaultError::ImportCorrupt)? as usize;
        let value = read_slice(data, &mut at, value_len).ok_or(VaultError::ImportCorrupt)?;

        match (entry_type, key) {
            (VARIANT_TYPE_UINT64, b"M") | (VARIANT_TYPE_UINT32, b"M") => {
                memory = Some(read_variant_number(entry_type, value)?);
            }
            (VARIANT_TYPE_UINT64, b"I") | (VARIANT_TYPE_UINT32, b"I") => {
                iterations = Some(read_variant_number(entry_type, value)?);
            }
            _ => {}
        }
    }

    check_limits(memory, iterations)
}

fn read_variant_number(entry_type: u8, value: &[u8]) -> Result<u64, VaultError> {
    match (entry_type, value.len()) {
        (VARIANT_TYPE_UINT32, 4) => Ok(u64::from(u32::from_le_bytes(
            value.try_into().map_err(|_| VaultError::ImportCorrupt)?,
        ))),
        (VARIANT_TYPE_UINT64, 8) => Ok(u64::from_le_bytes(
            value.try_into().map_err(|_| VaultError::ImportCorrupt)?,
        )),
        _ => Err(VaultError::ImportCorrupt),
    }
}

pub fn parse(
    bytes: &[u8],
    password: Option<&str>,
    keyfile: Option<&[u8]>,
) -> Result<ImportBundle<'static>, VaultError> {
    guard_kdf_params(bytes)?;

    let mut key = DatabaseKey::new();
    if let Some(password) = password {
        key = key.with_password(password);
    }
    if let Some(keyfile) = keyfile {
        key = key
            .with_keyfile(&mut Cursor::new(keyfile))
            .map_err(|_| VaultError::ImportPasswordInvalid)?;
    }

    let database = Database::parse(bytes, key).map_err(|_| {
        if password.is_none() {
            VaultError::ImportPasswordRequired
        } else {
            VaultError::ImportPasswordInvalid
        }
    })?;

    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for entry in database.iter_all_entries() {
        let name = entry.get_title().unwrap_or("").trim().to_string();
        if name.is_empty() {
            warnings.push(Cow::Borrowed("KeePass: запись без имени"));
        }
        let mut item = ImportedItem::new(if name.is_empty() {
            Cow::Borrowed("Без названия")
        } else {
            Cow::Owned(name.clone())
        });
        item.login = Cow::Owned(entry.get_username().unwrap_or("").to_string());
        item.password = Cow::Owned(entry.get_password().unwrap_or("").to_string());
        if let Some(url) = entry.get_url().filter(|value| !value.trim().is_empty()) {
            item.urls.push(Cow::Owned(url.trim().to_string()));
        }
        let mut comment = entry.get("Notes").unwrap_or("").to_string();
        let parent = entry.parent();
        if parent.parent().is_some() && !parent.name.is_empty() {
            comment = format!("Папка: {}\n{comment}", parent.name);
        }
        if let Some(otp) = entry.get_raw_otp_value() {
            match totp::inspect(otp) {
                Ok(config) => item.totp = Some(config),
                Err(_) => warnings.push(Cow::Owned(format!("{name}: TOTP не распознан"))),
            }
        }
        for (file_name, attachment) in entry.attachments_named() {
            item.attachments.push(ImportedAttachment {
                name: Cow::Owned(file_name.to_string()),
                mime: Cow::Borrowed("application/octet-stream"),
                b64: Cow::Owned(
                    base64::engine::general_purpose::STANDARD.encode(attachment.data.get()),
                ),
            });
        }
        item.comment = Cow::Owned(comment.trim().to_string());
        items.push(item);
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: Cow::Borrowed("keepass-kdbx"),
        items,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant_dictionary(entries: &[(&str, u64)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0100u16.to_le_bytes());
        for (key, value) in entries {
            out.push(VARIANT_TYPE_UINT64);
            out.extend_from_slice(&(key.len() as u32).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&8u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.push(VARIANT_DICT_END);
        out
    }

    fn kdbx_header(minor: u16, fields: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&KDBX_SIGNATURE_1.to_le_bytes());
        out.extend_from_slice(&KDBX_SIGNATURE_2.to_le_bytes());
        let version = (u32::from(minor) << 16) | 4;
        out.extend_from_slice(&version.to_le_bytes());
        for (id, data) in fields {
            out.push(*id);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(data);
        }
        out.push(HEADER_FIELD_END);
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    #[test]
    fn accepts_reasonable_argon2_parameters() {
        let header = kdbx_header(
            1,
            &[(
                HEADER_FIELD_KDF_KDBX4,
                variant_dictionary(&[("M", 64 * 1024 * 1024), ("I", 3)]),
            )],
        );
        assert_eq!(guard_kdf_params(&header), Ok(()));
    }

    #[test]
    fn rejects_memory_bomb() {
        let header = kdbx_header(
            1,
            &[(
                HEADER_FIELD_KDF_KDBX4,
                variant_dictionary(&[("M", 8 * 1024 * 1024 * 1024), ("I", 3)]),
            )],
        );
        assert_eq!(guard_kdf_params(&header), Err(VaultError::ImportCorrupt));
    }

    #[test]
    fn rejects_work_bomb() {
        // Both limits pass individually, the product does not.
        let header = kdbx_header(
            1,
            &[(
                HEADER_FIELD_KDF_KDBX4,
                variant_dictionary(&[("M", 1024 * 1024 * 1024), ("I", 10_000_000)]),
            )],
        );
        assert_eq!(guard_kdf_params(&header), Err(VaultError::ImportCorrupt));
    }

    #[test]
    fn rejects_iteration_bomb() {
        let header = kdbx_header(
            1,
            &[(
                HEADER_FIELD_KDF_KDBX4,
                variant_dictionary(&[("M", 64 * 1024 * 1024), ("I", 5_000_000_000)]),
            )],
        );
        assert_eq!(guard_kdf_params(&header), Err(VaultError::ImportCorrupt));
    }

    #[test]
    fn rejects_kdbx3_iteration_bomb() {
        let header = kdbx_header(
            0,
            &[(
                HEADER_FIELD_ROUNDS_KDBX3,
                5_000_000_000u64.to_le_bytes().to_vec(),
            )],
        );
        assert_eq!(guard_kdf_params(&header), Err(VaultError::ImportCorrupt));
        let ok = kdbx_header(
            0,
            &[(HEADER_FIELD_ROUNDS_KDBX3, 600_000u64.to_le_bytes().to_vec())],
        );
        assert_eq!(guard_kdf_params(&ok), Ok(()));
    }

    #[test]
    fn malformed_headers_are_left_to_the_parser() {
        // Not a KDBX file at all: the guard stays out of the way.
        assert_eq!(guard_kdf_params(b"not a kdbx database"), Ok(()));
        // Truncated KDBX: the guard refuses instead of slicing out of bounds.
        assert!(guard_kdf_params(&KDBX_SIGNATURE_1.to_le_bytes()).is_err());
        // Declared field length longer than the buffer.
        let mut broken = KDBX_SIGNATURE_1.to_le_bytes().to_vec();
        broken.extend_from_slice(&KDBX_SIGNATURE_2.to_le_bytes());
        broken.extend_from_slice(&0x0004_0001u32.to_le_bytes());
        broken.push(HEADER_FIELD_KDF_KDBX4);
        broken.extend_from_slice(&1000u32.to_le_bytes());
        assert!(guard_kdf_params(&broken).is_err());
    }

    #[test]
    fn parses_argon2id_kdbx_fixture() {
        let bytes = include_bytes!("../../tests/fixtures/keepass_argon2id.kdbx");
        let bundle = parse(bytes, Some("demopass"), None).unwrap();
        assert_eq!(bundle.format, "keepass-kdbx");
        assert_eq!(bundle.items.len(), 2);
    }
}
