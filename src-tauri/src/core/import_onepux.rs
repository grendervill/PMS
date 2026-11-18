use base64::Engine as _;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;

use serde_json::Value;
use zip::ZipArchive;

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem};
use super::totp;

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn field_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Object(map) => map
            .get("concealed")
            .or_else(|| map.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn attachment(
    files: &HashMap<String, Vec<u8>>,
    document_id: &str,
    name: &str,
) -> Option<ImportedAttachment<'static>> {
    let path = files
        .keys()
        .find(|path| path.starts_with(&format!("files/{document_id}")) || path.ends_with(name))?;
    Some(ImportedAttachment {
        name: Cow::Owned(name.to_string()),
        mime: Cow::Borrowed("application/octet-stream"),
        b64: Cow::Owned(base64::engine::general_purpose::STANDARD.encode(&files[path])),
    })
}

pub fn parse(bytes: &[u8]) -> Result<ImportBundle<'static>, VaultError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| VaultError::ImportCorrupt)?;
    let mut files = HashMap::new();
    let mut budget = super::import::UnpackBudget::new();
    let mut data = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let name = entry
            .name()
            .map_err(|_| VaultError::ImportCorrupt)?
            .to_string();
        let size = entry.size();
        let content = budget.read_entry(&mut entry, size)?;
        if name == "export.data" || name.ends_with("/export.data") {
            data = Some(content);
        } else if name.starts_with("files/") {
            files.insert(name, content);
        }
    }
    parse_json(&data.ok_or(VaultError::ImportCorrupt)?, &files)
}

/// Parses a bare `export.data` (the 1PUX container is a zip, but the same JSON
/// can be imported on its own).
pub fn parse_json(
    data: &[u8],
    files: &HashMap<String, Vec<u8>>,
) -> Result<ImportBundle<'static>, VaultError> {
    let root: Value = serde_json::from_slice(data).map_err(|_| VaultError::ImportCorrupt)?;
    let accounts = root
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or(VaultError::ImportCorrupt)?;
    let mut items = Vec::new();
    let mut warnings = Vec::new();

    for account in accounts {
        let Some(vaults) = account.get("vaults").and_then(Value::as_array) else {
            continue;
        };
        for vault in vaults {
            let Some(vault_items) = vault.get("items").and_then(Value::as_array) else {
                continue;
            };
            for item in vault_items {
                let overview = item.get("overview").cloned().unwrap_or(Value::Null);
                let details = item.get("details").cloned().unwrap_or(Value::Null);
                let name = text(overview.get("title"));
                if name.is_empty() {
                    warnings.push(Cow::Borrowed("1Password: запись без имени"));
                }
                let mut entry = ImportedItem::new(if name.is_empty() {
                    Cow::Borrowed("Без названия")
                } else {
                    Cow::Owned(name.clone())
                });
                if let Some(url) = overview.get("url").and_then(Value::as_str) {
                    if !url.trim().is_empty() {
                        entry.urls.push(Cow::Owned(url.trim().to_string()));
                    }
                }
                if let Some(urls) = overview.get("urls").and_then(Value::as_array) {
                    for url in urls {
                        let value = text(url.get("url"));
                        let value: Cow<'_, str> = Cow::Owned(value);
                        if !value.is_empty() && !entry.urls.contains(&value) {
                            entry.urls.push(value);
                        }
                    }
                }

                let mut comment = text(details.get("notesPlain"));
                if let Some(tags) = overview.get("tags").and_then(Value::as_array) {
                    let tags = tags
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|tag| !tag.is_empty())
                        .collect::<Vec<_>>();
                    if !tags.is_empty() {
                        comment = format!("Теги: {}\n{comment}", tags.join(", "));
                    }
                }
                if let Some(fields) = details.get("loginFields").and_then(Value::as_array) {
                    for field in fields {
                        let designation = text(field.get("designation"));
                        let field_name = text(field.get("name")).to_ascii_lowercase();
                        let value = text(field.get("value"));
                        if designation == "username" || field_name == "username" {
                            entry.login = Cow::Owned(value);
                        } else if designation == "password" || field_name == "password" {
                            entry.password = Cow::Owned(value);
                        }
                    }
                }

                if let Some(sections) = details.get("sections").and_then(Value::as_array) {
                    for section in sections {
                        let Some(fields) = section.get("fields").and_then(Value::as_array) else {
                            continue;
                        };
                        for field in fields {
                            let label = text(field.get("title"));
                            let id = text(field.get("id")).to_ascii_lowercase();
                            let value = field_text(field.get("value").unwrap_or(&Value::Null));
                            if value.is_empty() {
                                continue;
                            }
                            if label.to_ascii_lowercase().contains("one-time password")
                                || label.to_ascii_lowercase().contains("totp")
                                || id.contains("totp")
                            {
                                match totp::inspect(&value) {
                                    Ok(config) => entry.totp = Some(config),
                                    Err(_) => warnings
                                        .push(Cow::Owned(format!("{name}: TOTP не распознан"))),
                                }
                            } else {
                                comment.push_str(&format!("\n{label}: {value}"));
                            }
                        }
                    }
                }

                if let Some(history) = details.get("passwordHistory").and_then(Value::as_array) {
                    for old in history {
                        let value = text(old.get("value"));
                        if value.is_empty() {
                            continue;
                        }
                        let changed_at = text(old.get("createdAt"));
                        entry.push_password_history(
                            Cow::Owned(value),
                            (!changed_at.is_empty()).then_some(Cow::Owned(changed_at)),
                        );
                    }
                }

                if let Some(document) = details.get("documentAttributes") {
                    let document_id = text(document.get("documentId"));
                    let file_name = text(document.get("fileName"));
                    if !file_name.is_empty() {
                        if let Some(attachment) = attachment(files, &document_id, &file_name) {
                            entry.attachments.push(attachment);
                        } else {
                            warnings.push(Cow::Owned(format!(
                                "{name}: файл «{file_name}» отсутствует"
                            )));
                        }
                    }
                    entry.kind = Cow::Borrowed("note");
                }

                entry.comment = Cow::Owned(comment.trim().to_string());
                items.push(entry);
            }
        }
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: Cow::Borrowed("1password-1pux"),
        items,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    #[test]
    fn parses_1pux_zip_with_attachment() {
        let export = br#"{
          "accounts": [{"vaults": [{"items": [{
            "overview": {"title": "One", "urls": [{"url": "https://one.example"}]},
            "details": {
              "loginFields": [
                {"value": "alex", "designation": "username"},
                {"value": "s3cret", "designation": "password"}
              ],
              "notesPlain": "note",
              "sections": [{"fields": [{"title": "TOTP", "id": "TOTP", "value": "JBSWY3DPEHPK3PXP"}]}],
              "documentAttributes": {"fileName": "doc.txt", "documentId": "doc-id"}
            }
          }]}]}]
        }"#;
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buffer);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("export.data", options).unwrap();
            use std::io::Write;
            zip.write_all(export).unwrap();
            zip.start_file("files/doc-id___doc.txt", options).unwrap();
            zip.write_all(b"hello").unwrap();
            zip.finish().unwrap();
        }
        let bundle = parse(buffer.get_ref()).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.login, "alex");
        assert_eq!(item.password, "s3cret");
        assert_eq!(item.urls, vec!["https://one.example"]);
        assert!(item.totp.is_some());
        assert_eq!(item.attachments.len(), 1);
    }

    #[test]
    fn moves_1pux_password_history_out_of_notes() {
        let export = br#"{
          "accounts": [{"vaults": [{"items": [{
            "overview": {"title": "One"},
            "details": {
              "loginFields": [{"value": "s3cret", "designation": "password"}],
              "notesPlain": "note",
              "passwordHistory": [
                {"value": "old-1", "createdAt": "2024-01-01T00:00:00Z"},
                {"value": "old-1"},
                {"value": "s3cret"}
              ]
            }
          }]}]}]
        }"#;
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buffer);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("export.data", options).unwrap();
            use std::io::Write;
            zip.write_all(export).unwrap();
            zip.finish().unwrap();
        }
        let bundle = parse(buffer.get_ref()).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.comment, "note");
        assert_eq!(item.password_history.len(), 1);
        assert_eq!(item.password_history[0].value, "old-1");
        assert_eq!(
            item.password_history[0].changed_at.as_deref(),
            Some("2024-01-01T00:00:00Z")
        );
    }
}
