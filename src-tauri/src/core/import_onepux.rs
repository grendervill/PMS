use base64::Engine as _;
use std::collections::HashMap;
use std::io::{Cursor, Read};

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
) -> Option<ImportedAttachment> {
    let path = files
        .keys()
        .find(|path| path.starts_with(&format!("files/{document_id}")) || path.ends_with(name))?;
    Some(ImportedAttachment {
        name: name.to_string(),
        mime: "application/octet-stream".to_string(),
        b64: base64::engine::general_purpose::STANDARD.encode(&files[path]),
    })
}

pub fn parse(bytes: &[u8]) -> Result<ImportBundle, VaultError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| VaultError::ImportCorrupt)?;
    let mut files = HashMap::new();
    let mut data = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| VaultError::ImportCorrupt)?;
        let name = entry
            .name()
            .map_err(|_| VaultError::ImportCorrupt)?
            .to_string();
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|_| VaultError::ImportCorrupt)?;
        if name == "export.data" || name.ends_with("/export.data") {
            data = Some(content);
        } else if name.starts_with("files/") {
            files.insert(name, content);
        }
    }
    let root: Value = serde_json::from_slice(&data.ok_or(VaultError::ImportCorrupt)?)
        .map_err(|_| VaultError::ImportCorrupt)?;
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
                    warnings.push("1Password: запись без имени".to_string());
                }
                let mut entry = ImportedItem::new(if name.is_empty() {
                    "Без названия".to_string()
                } else {
                    name.clone()
                });
                if let Some(url) = overview.get("url").and_then(Value::as_str) {
                    if !url.trim().is_empty() {
                        entry.urls.push(url.trim().to_string());
                    }
                }
                if let Some(urls) = overview.get("urls").and_then(Value::as_array) {
                    for url in urls {
                        let value = text(url.get("url"));
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
                            entry.login = value;
                        } else if designation == "password" || field_name == "password" {
                            entry.password = value;
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
                                    Err(_) => warnings.push(format!("{name}: TOTP не распознан")),
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
                        if !value.is_empty() {
                            comment.push_str(&format!("\nСтарый пароль: {value}"));
                        }
                    }
                }

                if let Some(document) = details.get("documentAttributes") {
                    let document_id = text(document.get("documentId"));
                    let file_name = text(document.get("fileName"));
                    if !file_name.is_empty() {
                        if let Some(attachment) = attachment(&files, &document_id, &file_name) {
                            entry.attachments.push(attachment);
                        } else {
                            warnings.push(format!("{name}: файл «{file_name}» отсутствует"));
                        }
                    }
                    entry.kind = "note".to_string();
                }

                entry.comment = comment.trim().to_string();
                items.push(entry);
            }
        }
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "1password-1pux".to_string(),
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
}
