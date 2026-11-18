use base64::Engine as _;
use std::borrow::Cow;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::error::VaultError;
use super::import::{ImportBundle, ImportedAttachment, ImportedItem};
use super::totp;

struct PendingEntry {
    item: ImportedItem<'static>,
    group: String,
}

/// Where a text run belongs, derived from the element stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    Group,
    FieldKey,
    FieldValue,
    BinaryKey,
    BinaryValue,
    None,
}

fn text_target(group_name_depth: usize, stack: &[String], is_binary: bool) -> TextTarget {
    if group_name_depth > 0 {
        return TextTarget::Group;
    }
    let in_tag = |tag: &str| stack.iter().any(|name| name == tag);
    if in_tag("Key") {
        if is_binary {
            TextTarget::BinaryKey
        } else {
            TextTarget::FieldKey
        }
    } else if in_tag("Value") {
        if is_binary {
            TextTarget::BinaryValue
        } else {
            TextTarget::FieldValue
        }
    } else {
        TextTarget::None
    }
}

struct Buffers<'a> {
    groups: &'a mut Vec<String>,
    field_key: &'a mut String,
    field_value: &'a mut String,
    binary_key: &'a mut String,
    binary_value: &'a mut String,
}

impl Buffers<'_> {
    fn append(&mut self, target: TextTarget, value: &str) {
        match target {
            TextTarget::Group => {
                if let Some(group) = self.groups.last_mut() {
                    group.push_str(value);
                }
            }
            TextTarget::FieldKey => self.field_key.push_str(value),
            TextTarget::FieldValue => self.field_value.push_str(value),
            TextTarget::BinaryKey => self.binary_key.push_str(value),
            TextTarget::BinaryValue => self.binary_value.push_str(value),
            TextTarget::None => {}
        }
    }
}

/// Expands `&amp;`-style references. quick-xml (>= 0.41) emits them as separate
/// `GeneralRef` events; ignoring those would silently corrupt passwords and
/// URLs. Unknown references are kept literally, losing no data.
fn resolve_entity(name: &str) -> String {
    match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ => {
            let code = name.strip_prefix('#').and_then(|digits| {
                if let Some(hex) = digits.strip_prefix(['x', 'X']) {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    digits.parse::<u32>().ok()
                }
            });
            match code.and_then(char::from_u32) {
                Some(character) => character.to_string(),
                None => format!("&{name};"),
            }
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<ImportBundle<'static>, VaultError> {
    let mut reader = Reader::from_reader(bytes);
    // Text is *not* trimmed per event: with entity references split into their
    // own events, `Bank &amp; Co` arrives as "Bank ", "&", " Co" and trimming
    // each chunk would eat the spaces. Values are trimmed when they are read.
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
                            item: ImportedItem::new(Cow::Borrowed("Без названия")),
                            group: if groups.len() > 1 {
                                groups
                                    .last()
                                    .map(|group| group.trim().to_string())
                                    .unwrap_or_default()
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
                    .map_err(|_| VaultError::ImportCorrupt)?;
                let target = text_target(group_name_depth, &stack, is_binary);
                let mut buffers = Buffers {
                    groups: &mut groups,
                    field_key: &mut field_key,
                    field_value: &mut field_value,
                    binary_key: &mut binary_key,
                    binary_value: &mut binary_value,
                };
                buffers.append(target, &value);
            }
            Ok(Event::CData(cdata)) => {
                let value = cdata
                    .decode()
                    .map_err(|_| VaultError::ImportCorrupt)?;
                let target = text_target(group_name_depth, &stack, is_binary);
                let mut buffers = Buffers {
                    groups: &mut groups,
                    field_key: &mut field_key,
                    field_value: &mut field_value,
                    binary_key: &mut binary_key,
                    binary_value: &mut binary_value,
                };
                buffers.append(target, &value);
            }
            Ok(Event::GeneralRef(reference)) => {
                let name = reference
                    .decode()
                    .map_err(|_| VaultError::ImportCorrupt)?;
                let value = resolve_entity(&name);
                let target = text_target(group_name_depth, &stack, is_binary);
                let mut buffers = Buffers {
                    groups: &mut groups,
                    field_key: &mut field_key,
                    field_value: &mut field_value,
                    binary_key: &mut binary_key,
                    binary_value: &mut binary_value,
                };
                buffers.append(target, &value);
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if name == "Name" && group_name_depth > 0 {
                    group_name_depth = group_name_depth.saturating_sub(1);
                } else if name == "String" {
                    if let Some(entry) = current.as_mut() {
                        let value = Cow::Owned(field_value.trim().to_string());
                        match field_key.trim() {
                            "Title" => entry.item.name = value,
                            "UserName" => entry.item.login = value,
                            "Password" => entry.item.password = value,
                            "URL" => {
                                if !value.is_empty() {
                                    entry.item.urls.push(value);
                                }
                            }
                            "Notes" => entry.item.comment = value,
                            "otp" | "TOTP Seed" => {
                                match totp::inspect(value.as_ref()) {
                                    Ok(config) => entry.item.totp = Some(config),
                                    Err(_) => warnings.push(Cow::Owned(format!(
                                        "{}: TOTP не распознан",
                                        entry.item.name
                                    ))),
                                }
                            }
                            _ if !field_key.trim().is_empty() && !value.is_empty() => {
                                entry
                                    .item
                                    .comment
                                    .to_mut()
                                    .push_str(&format!("\n{}: {value}", field_key.trim()));
                            }
                            _ => {}
                        }
                    }
                } else if name == "Binary" {
                    if let Some(entry) = current.as_mut() {
                        if !binary_key.trim().is_empty() && !binary_value.trim().is_empty() {
                            match base64::engine::general_purpose::STANDARD
                                .decode(binary_value.trim())
                            {
                                Ok(bytes) => entry.item.attachments.push(ImportedAttachment {
                                    name: Cow::Owned(binary_key.trim().to_string()),
                                    mime: Cow::Borrowed("application/octet-stream"),
                                    b64: Cow::Owned(
                                        base64::engine::general_purpose::STANDARD.encode(bytes),
                                    ),
                                }),
                                Err(_) => warnings.push(Cow::Owned(format!(
                                    "{}: вложение {} повреждено",
                                    entry.item.name,
                                    binary_key.trim()
                                ))),
                            }
                        }
                    }
                    is_binary = false;
                } else if name == "Entry" && in_history == 0 {
                    if let Some(mut entry) = current.take() {
                        if entry.item.name.trim().is_empty() {
                            entry.item.name = Cow::Borrowed("Без названия");
                            warnings.push(Cow::Borrowed("KeePass XML: запись без имени"));
                        }
                        if !entry.group.is_empty() {
                            entry.item.comment = Cow::Owned(
                                format!("Папка: {}\n{}", entry.group, entry.item.comment)
                                    .trim()
                                    .to_string(),
                            );
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
        format: Cow::Borrowed("keepass-xml"),
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

    /// KeePass escapes `&`, `<` and `>` in values. A parser that drops entity
    /// references would silently corrupt passwords, so the escaped forms must
    /// survive the round trip.
    #[test]
    fn unescapes_entities_in_field_values() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile><Root><Group><Entry>
<String><Key>Title</Key><Value>Bank &amp; Co &lt;main&gt;</Value></String>
<String><Key>Password</Key><Value>p&amp;a&#115;&#x73;word</Value></String>
<String><Key>URL</Key><Value>https://example.com/?a=1&amp;b=2</Value></String>
</Entry></Group></Root></KeePassFile>"#;
        let bundle = parse(xml).unwrap();
        assert_eq!(bundle.items[0].name, "Bank & Co <main>");
        assert_eq!(bundle.items[0].password, "p&assword");
        assert_eq!(bundle.items[0].urls, vec!["https://example.com/?a=1&b=2"]);
    }

    #[test]
    fn keeps_unknown_entities_literally() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile><Root><Group><Entry>
<String><Key>Notes</Key><Value>a&nbsp;b</Value></String>
</Entry></Group></Root></KeePassFile>"#;
        let bundle = parse(xml).unwrap();
        assert_eq!(bundle.items[0].comment, "a&nbsp;b");
    }
}
