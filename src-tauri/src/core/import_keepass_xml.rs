use base64::Engine as _;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem};
use super::totp;

struct PendingEntry {
    item: ImportedItem,
    group: String,
}

pub fn parse(bytes: &[u8]) -> Result<ImportBundle, VaultError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    let mut group_name_depth = 0usize;
    let mut current: Option<PendingEntry> = None;
    let mut in_history = 0u32;
    let mut field_key = String::new();
    let mut field_value = String::new();
    let mut binary_key = String::new();
    let mut binary_value = String::new();
    let mut is_binary = false;
    let mut items = Vec::new();
    let mut warnings = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                stack.push(name.clone());
                match name.as_str() {
                    "Group" => groups.push(String::new()),
                    "Name" if stack.iter().any(|value| value == "Group") => {
                        group_name_depth += 1;
                    }
                    "History" => in_history += 1,
                    "Entry" if in_history == 0 => {
                        current = Some(PendingEntry {
                            item: ImportedItem::new("Без названия".to_string()),
                            group: if groups.len() > 1 {
                                groups.last().cloned().unwrap_or_default()
                            } else {
                                String::new()
                            },
                        });
                    }
                    "String" => {
                        field_key.clear();
                        field_value.clear();
                    }
                    "Binary" => {
                        binary_key.clear();
                        binary_value.clear();
                        is_binary = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                let value = text
                    .decode()
                    .map_err(|_| VaultError::ImportCorrupt)?
                    .into_owned();
                if group_name_depth > 0 {
                    if let Some(group) = groups.last_mut() {
                        group.push_str(&value);
                    }
                } else if stack.iter().any(|name| name == "Key") {
                    if is_binary {
                        binary_key.push_str(&value);
                    } else {
                        field_key.push_str(&value);
                    }
                } else if stack.iter().any(|name| name == "Value") {
                    if is_binary {
                        binary_value.push_str(&value);
                    } else {
                        field_value.push_str(&value);
                    }
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if name == "Name" && group_name_depth > 0 {
                    group_name_depth = group_name_depth.saturating_sub(1);
                } else if name == "String" {
                    if let Some(entry) = current.as_mut() {
                        let value = field_value.trim().to_string();
                        match field_key.as_str() {
                            "Title" => entry.item.name = value,
                            "UserName" => entry.item.login = value,
                            "Password" => entry.item.password = value,
                            "URL" => {
                                if !value.is_empty() {
                                    entry.item.urls.push(value);
                                }
                            }
                            "Notes" => entry.item.comment = value,
                            "otp" | "TOTP Seed" => match totp::inspect(&value) {
                                Ok(config) => entry.item.totp = Some(config),
                                Err(_) => warnings
                                    .push(format!("{}: TOTP не распознан", entry.item.name)),
                            },
                            _ if !field_key.is_empty() && !value.is_empty() => {
                                entry
                                    .item
                                    .comment
                                    .push_str(&format!("\n{}: {value}", field_key));
                            }
                            _ => {}
                        }
                    }
                } else if name == "Binary" {
                    if let Some(entry) = current.as_mut() {
                        if !binary_key.is_empty() && !binary_value.trim().is_empty() {
                            match base64::engine::general_purpose::STANDARD
                                .decode(binary_value.trim())
                            {
                                Ok(bytes) => entry.item.attachments.push(ImportedAttachment {
                                    name: binary_key.clone(),
                                    mime: "application/octet-stream".to_string(),
                                    b64: base64::engine::general_purpose::STANDARD.encode(bytes),
                                }),
                                Err(_) => warnings.push(format!(
                                    "{}: вложение {} повреждено",
                                    entry.item.name, binary_key
                                )),
                            }
                        }
                    }
                    is_binary = false;
                } else if name == "Entry" && in_history == 0 {
                    if let Some(mut entry) = current.take() {
                        if entry.item.name.trim().is_empty() {
                            entry.item.name = "Без названия".to_string();
                            warnings.push("KeePass XML: запись без имени".to_string());
                        }
                        if !entry.group.is_empty() {
                            entry.item.comment =
                                format!("Папка: {}\n{}", entry.group, entry.item.comment)
                                    .trim()
                                    .to_string();
                        }
                        items.push(entry.item);
                    }
                } else if name == "History" {
                    in_history = in_history.saturating_sub(1);
                } else if name == "Group" {
                    groups.pop();
                }
                stack.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(VaultError::ImportCorrupt),
        }
        buffer.clear();
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "keepass-xml".to_string(),
        items,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keepass_xml_with_attachment_and_totp() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile><Root><Group><Name>Root</Name><Entry>
<String><Key>Title</Key><Value>Mail</Value></String>
<String><Key>UserName</Key><Value>alex</Value></String>
<String><Key>Password</Key><Value>s3cret</Value></String>
<String><Key>URL</Key><Value>https://mail.example</Value></String>
<String><Key>Notes</Key><Value>note</Value></String>
<String><Key>otp</Key><Value>JBSWY3DPEHPK3PXP</Value></String>
<Binary><Key>doc.txt</Key><Value>aGVsbG8=</Value></Binary>
</Entry></Group></Root></KeePassFile>"#;
        let bundle = parse(xml).unwrap();
        assert_eq!(bundle.items[0].name, "Mail");
        assert_eq!(bundle.items[0].login, "alex");
        assert_eq!(bundle.items[0].urls, vec!["https://mail.example"]);
        assert!(bundle.items[0].totp.is_some());
        assert_eq!(bundle.items[0].attachments.len(), 1);
    }
}
