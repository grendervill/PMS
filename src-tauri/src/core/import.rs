use std::collections::HashMap;
use std::io::{Cursor, Read};

use base64::Engine as _;
use serde::Serialize;

use super::error::VaultError;
use super::models::TotpConfig;
use super::totp;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportBundle {
    pub format: String,
    pub items: Vec<ImportedItem>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedItem {
    pub kind: String,
    pub name: String,
    pub login: String,
    pub password: String,
    pub urls: Vec<String>,
    pub comment: String,
    pub totp: Option<TotpConfig>,
    pub attachments: Vec<ImportedAttachment>,
    pub ssh_key: Option<ImportedSshKey>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedAttachment {
    pub name: String,
    pub mime: String,
    pub b64: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSshKey {
    pub name: String,
    pub public_key: String,
    pub private_key: String,
    pub passphrase: Option<String>,
    pub comment: String,
}

impl ImportedItem {
    pub(crate) fn new(name: String) -> Self {
        Self {
            kind: "login".to_string(),
            name,
            login: String::new(),
            password: String::new(),
            urls: Vec::new(),
            comment: String::new(),
            totp: None,
            attachments: Vec::new(),
            ssh_key: None,
        }
    }
}

fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn parse(
    bytes: &[u8],
    password: Option<&str>,
    keyfile: Option<&[u8]>,
) -> Result<ImportBundle, VaultError> {
    if bytes.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }

    if bytes.starts_with(&[0x03, 0xD9, 0xA2, 0x9A]) {
        return super::import_keepass::parse(bytes, password, keyfile);
    }
    if text_contains_keepass_xml(bytes) {
        return super::import_keepass_xml::parse(bytes);
    }

    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) {
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| VaultError::ImportCorrupt)?;
            names.push(
                entry
                    .name()
                    .map_err(|_| VaultError::ImportCorrupt)?
                    .to_string(),
            );
        }
        if names.iter().any(|name| name == "export.data") {
            return super::import_onepux::parse(bytes);
        }
        if names.iter().any(|name| name == "data.json") {
            let data = {
                let mut entry = archive
                    .by_name("data.json")
                    .map_err(|_| VaultError::ImportCorrupt)?;
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|_| VaultError::ImportCorrupt)?;
                content
            };
            let mut attachments = HashMap::new();
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|_| VaultError::ImportCorrupt)?;
                let name = entry
                    .name()
                    .map_err(|_| VaultError::ImportCorrupt)?
                    .to_string();
                if name == "data.json" || !name.starts_with("attachments/") {
                    continue;
                }
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|_| VaultError::ImportCorrupt)?;
                attachments.insert(name, content);
            }
            return super::import_bitwarden::parse(&data, password, &attachments);
        }
        return Err(VaultError::ImportUnsupported);
    }

    let text = decode_text(bytes);
    if text.trim_start().starts_with('{') {
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| VaultError::ImportCorrupt)?;
        if value.get("accounts").is_some() {
            return super::import_onepux::parse(bytes);
        }
        if value.get("items").is_some() || value.get("encrypted").is_some() {
            return super::import_bitwarden::parse(bytes, password, &HashMap::new());
        }
        return Err(VaultError::ImportUnsupported);
    }
    if looks_like_kaspersky(&text) {
        return parse_kaspersky(&text);
    }
    if looks_like_csv(&text) {
        return parse_csv(&text);
    }
    Err(VaultError::ImportUnsupported)
}

fn text_contains_keepass_xml(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    head.starts_with(b"<?xml") || head.windows(12).any(|window| window == b"<KeePassFile")
}

fn looks_like_csv(text: &str) -> bool {
    let first = text.lines().next().unwrap_or("").to_ascii_lowercase();
    (first.contains(',') || first.contains(';') || first.contains('\t'))
        && (first.contains("password") || first.contains("url") || first.contains("login"))
}

fn looks_like_kaspersky(text: &str) -> bool {
    let head = text.lines().take(20).collect::<Vec<_>>().join("\n");
    head.contains("Website name:") || (head.contains("Websites") && head.contains("Login name:"))
}

fn parse_csv(text: &str) -> Result<ImportBundle, VaultError> {
    let first = text.lines().next().unwrap_or("");
    let delimiter = if first.matches(';').count() > first.matches(',').count() {
        b';'
    } else if first.matches('\t').count() > first.matches(',').count() {
        b'\t'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| VaultError::ImportCorrupt)?
        .iter()
        .map(|header| header.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    let mut warnings = Vec::new();

    for (row, record) in reader.records().enumerate() {
        let record = record.map_err(|_| VaultError::ImportCorrupt)?;
        let value = |names: &[&str]| -> String {
            headers
                .iter()
                .position(|header| names.iter().any(|name| header == name))
                .and_then(|index| record.get(index))
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let name = {
            let name = value(&["name", "title", "account", "website name"]);
            if name.is_empty() {
                let url = value(&["url", "uri", "login_uri", "website", "website url"]);
                if url.is_empty() {
                    warnings.push(format!("Строка {}: пустое имя", row + 2));
                    "Без названия".to_string()
                } else {
                    url.split('/').nth(2).unwrap_or("Без названия").to_string()
                }
            } else {
                name
            }
        };
        let url = value(&["url", "uri", "login_uri", "website", "website url"]);
        let mut urls = Vec::new();
        if !url.is_empty() {
            for item in url
                .split([',', ' '])
                .filter(|value| !value.trim().is_empty())
            {
                urls.push(item.trim().to_string());
            }
        }
        let login = value(&["username", "login", "login_username", "user"]);
        let password = value(&["password", "login_password", "pass"]);
        let mut comment = value(&["notes", "note", "comment", "login_notes"]);
        let folder = value(&["folder", "group", "collection"]);
        if !folder.is_empty() {
            comment = format!("Папка: {folder}\n{comment}");
        }
        let totp_raw = value(&["totp", "login_totp", "otpauth", "otp"]);
        let totp_value = if totp_raw.is_empty() {
            None
        } else {
            match totp::inspect(&totp_raw) {
                Ok(config) => Some(config),
                Err(_) => {
                    warnings.push(format!("Строка {}: TOTP не распознан", row + 2));
                    None
                }
            }
        };
        let kind = value(&["type"]);
        let kind = if kind == "note" || kind == "secure note" || kind == "securenote" {
            "note"
        } else if kind == "card" {
            "card"
        } else if kind == "identity" {
            "identity"
        } else {
            "login"
        };
        // Preserve unmapped non-empty columns as labelled comment lines.
        for (index, header) in headers.iter().enumerate() {
            if let Some(value) = record.get(index) {
                let value = value.trim();
                if value.is_empty()
                    || matches!(
                        header.as_str(),
                        "name"
                            | "title"
                            | "account"
                            | "website name"
                            | "url"
                            | "uri"
                            | "login_uri"
                            | "website"
                            | "website url"
                            | "username"
                            | "login"
                            | "login_username"
                            | "user"
                            | "login name"
                            | "password"
                            | "login_password"
                            | "pass"
                            | "notes"
                            | "note"
                            | "comment"
                            | "login_notes"
                            | "folder"
                            | "group"
                            | "collection"
                            | "totp"
                            | "login_totp"
                            | "otpauth"
                            | "otp"
                            | "type"
                            | "favorite"
                            | "reprompt"
                    )
                {
                    continue;
                }
                comment.push_str(&format!("\n{header}: {value}"));
            }
        }
        let mut item = ImportedItem::new(name);
        item.kind = kind.to_string();
        item.login = login;
        item.password = password;
        item.urls = urls;
        item.comment = comment.trim().to_string();
        item.totp = totp_value;
        items.push(item);
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "csv".to_string(),
        items,
        warnings,
    })
}

fn parse_kaspersky(text: &str) -> Result<ImportBundle, VaultError> {
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut current: Option<ImportedItem> = None;
    let mut field: Option<&str> = None;

    let mut finish = |current: &mut Option<ImportedItem>, items: &mut Vec<ImportedItem>| {
        if let Some(mut item) = current.take() {
            if item.name.trim().is_empty() {
                warnings.push("Пропущена запись Kaspersky без имени".to_string());
                return;
            }
            item.comment = item.comment.trim().to_string();
            items.push(item);
        }
    };

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" {
            field = None;
            if trimmed == "---" {
                finish(&mut current, &mut items);
            }
            continue;
        }
        if trimmed == "Websites" || trimmed == "Applications" || trimmed == "Notes" {
            finish(&mut current, &mut items);
            field = None;
            continue;
        }
        let Some((label, value)) = trimmed.split_once(':') else {
            if let (Some(item), Some(field)) = (current.as_mut(), field) {
                let target = match field {
                    "comment" => &mut item.comment,
                    "text" => &mut item.comment,
                    _ => continue,
                };
                if !target.is_empty() {
                    target.push('\n');
                }
                target.push_str(trimmed);
            }
            continue;
        };
        let label = label.trim();
        let value = value.trim();
        match label {
            "Website name" | "Application" => {
                finish(&mut current, &mut items);
                current = Some(ImportedItem::new(value.to_string()));
                field = Some("comment");
            }
            "Name" => {
                finish(&mut current, &mut items);
                current = Some(ImportedItem::new(value.to_string()));
                field = Some("comment");
                if let Some(item) = current.as_mut() {
                    item.kind = "note".to_string();
                }
            }
            "Website URL" => {
                if let Some(item) = current.as_mut() {
                    if !value.is_empty() {
                        item.urls.push(value.to_string());
                    }
                }
            }
            "Login name" | "Login" => {
                if let Some(item) = current.as_mut() {
                    if label == "Login" || item.login.is_empty() {
                        item.login = value.to_string();
                    }
                }
            }
            "Password" => {
                if let Some(item) = current.as_mut() {
                    item.password = value.to_string();
                }
            }
            "Comment" | "Text" => {
                if let Some(item) = current.as_mut() {
                    item.comment = value.to_string();
                    field = Some(if label == "Comment" {
                        "comment"
                    } else {
                        "text"
                    });
                }
            }
            _ => {}
        }
    }
    finish(&mut current, &mut items);

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: "kaspersky".to_string(),
        items,
        warnings,
    })
}

pub fn decode_attachment(b64: &str) -> Result<Vec<u8>, VaultError> {
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| VaultError::ImportCorrupt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_browser_csv_with_quoted_comment() {
        let text = "name,url,username,password,note\nGitHub,https://github.com,alex,s3cret,\"hello, world\"\n";
        let bundle = parse(text.as_bytes(), None, None).unwrap();
        assert_eq!(bundle.items.len(), 1);
        assert_eq!(bundle.items[0].urls, vec!["https://github.com"]);
        assert_eq!(bundle.items[0].comment, "hello, world");
    }

    #[test]
    fn parses_bitwarden_csv_totp_and_folder() {
        let text = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\nSocial,1,login,GitHub,note,,0,https://github.com,alex,s3cret,JBSWY3DPEHPK3PXP\n";
        let bundle = parse(text.as_bytes(), None, None).unwrap();
        let item = &bundle.items[0];
        assert_eq!(item.login, "alex");
        assert!(item.comment.contains("Папка: Social"));
        assert!(item.totp.is_some());
    }

    #[test]
    fn parses_1password_and_safari_csv() {
        let one = "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\nOne,https://one.example,alex,s3cret,JBSWY3DPEHPK3PXP,0,0,work,note\n";
        let bundle = parse(one.as_bytes(), None, None).unwrap();
        assert_eq!(bundle.items[0].name, "One");
        assert!(bundle.items[0].totp.is_some());

        let safari = "Title,URL,Username,Password,Notes,OTPAuth\nSafari,https://safari.example,alex,s3cret,note,\n";
        let bundle = parse(safari.as_bytes(), None, None).unwrap();
        assert_eq!(bundle.items[0].urls, vec!["https://safari.example"]);
    }

    #[test]
    fn parses_semicolon_kaspersky_csv() {
        let text = "Website name;Website URL;Login name;Login;Password;Comment\nExample;https://example.com;Example;alex;s3cret;hello\n";
        assert!(looks_like_csv(text));
        let bundle = parse(text.as_bytes(), None, None).unwrap();
        assert_eq!(bundle.items[0].name, "Example");
        assert_eq!(bundle.items[0].login, "alex");
        assert_eq!(bundle.items[0].comment, "hello");
    }

    #[test]
    fn parses_kaspersky_txt() {
        let text = "Websites\n\nWebsite name: Example\nWebsite URL: https://example.com\nLogin name: Example\nLogin: alex\nPassword: s3cret;\nComment: hello;\n\n---\n";
        let bundle = parse(text.as_bytes(), None, None).unwrap();
        assert_eq!(bundle.items.len(), 1);
        assert_eq!(bundle.items[0].urls[0], "https://example.com");
        assert_eq!(bundle.items[0].login, "alex");
        assert_eq!(bundle.items[0].password, "s3cret;");
    }
}
