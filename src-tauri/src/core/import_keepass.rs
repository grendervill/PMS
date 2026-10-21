use base64::Engine as _;
use std::io::Cursor;

use keepass::{Database, DatabaseKey};

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem};
use super::totp;

pub fn parse(
    bytes: &[u8],
    password: Option<&str>,
    keyfile: Option<&[u8]>,
) -> Result<ImportBundle, VaultError> {
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
            warnings.push("KeePass: запись без имени".to_string());
        }
        let mut item = ImportedItem::new(if name.is_empty() {
            "Без названия".to_string()
        } else {
            name.clone()
        });
        item.login = entry.get_username().unwrap_or("").to_string();
        item.password = entry.get_password().unwrap_or("").to_string();
        if let Some(url) = entry.get_url().filter(|value| !value.trim().is_empty()) {
            item.urls.push(url.trim().to_string());
        }
        let mut comment = entry.get("Notes").unwrap_or("").to_string();
        let parent = entry.parent();
        if !parent.name.is_empty() {
            comment = format!("Папка: {}\n{comment}", parent.name);
        }
        if let Some(otp) = entry.get_raw_otp_value() {
            match totp::inspect(otp) {
                Ok(config) => item.totp = Some(config),
                Err(_) => warnings.push(format!("{name}: TOTP не распознан")),
            }
        }
        for (file_name, attachment) in entry.attachments_named() {
            item.attachments.push(ImportedAttachment {
                name: file_name.to_string(),
                mime: "application/octet-stream".to_string(),
                b64: base64::engine::general_purpose::STANDARD.encode(attachment.data.get()),
            });
        }
        item.comment = comment.trim().to_string();
        items.push(item);
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "keepass-kdbx".to_string(),
        items,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_argon2id_kdbx_fixture() {
        let bytes = include_bytes!("../../tests/fixtures/keepass_argon2id.kdbx");
        let bundle = parse(bytes, Some("demopass"), None).unwrap();
        assert_eq!(bundle.format, "keepass-kdbx");
        assert_eq!(bundle.items.len(), 2);
    }
}
