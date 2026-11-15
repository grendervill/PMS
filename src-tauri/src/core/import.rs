use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io::{Cursor, Read};

use base64::Engine as _;
use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use serde::Serialize;

use super::error::VaultError;
use super::models::TotpConfig;
use super::totp;

/// Imported data borrows from the parsed file wherever possible: strings point
/// straight into the export bytes, so a 5000-entry import costs a handful of
/// allocations instead of a few hundred thousand.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportBundle<'a> {
    pub format: Cow<'a, str>,
    pub items: Vec<ImportedItem<'a>>,
    pub warnings: Vec<Cow<'a, str>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedItem<'a> {
    pub kind: Cow<'a, str>,
    pub name: Cow<'a, str>,
    pub login: Cow<'a, str>,
    pub password: Cow<'a, str>,
    pub urls: Vec<Cow<'a, str>>,
    pub comment: Cow<'a, str>,
    pub totp: Option<TotpConfig>,
    pub attachments: Vec<ImportedAttachment<'a>>,
    pub ssh_key: Option<ImportedSshKey<'a>>,
    pub password_history: Vec<ImportedHistoryItem<'a>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedHistoryItem<'a> {
    pub value: Cow<'a, str>,
    pub changed_at: Option<Cow<'a, str>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedAttachment<'a> {
    pub name: Cow<'a, str>,
    pub mime: Cow<'a, str>,
    pub b64: Cow<'a, str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSshKey<'a> {
    pub name: Cow<'a, str>,
    pub public_key: Cow<'a, str>,
    pub private_key: Cow<'a, str>,
    pub passphrase: Option<Cow<'a, str>>,
    pub comment: Cow<'a, str>,
}

impl<'a> ImportBundle<'a> {
    /// Detaches the bundle from its source buffer. Only needed for formats the
    /// parser has to decrypt or decompress first, where the plaintext is a
    /// temporary buffer and cannot be borrowed from.
    pub fn into_owned(self) -> ImportBundle<'static> {
        ImportBundle {
            format: Cow::Owned(self.format.into_owned()),
            items: self.items.into_iter().map(ImportedItem::into_owned).collect(),
            warnings: self
                .warnings
                .into_iter()
                .map(|warning| Cow::Owned(warning.into_owned()))
                .collect(),
        }
    }
}

impl<'a> ImportedItem<'a> {
    pub(crate) fn new(name: impl Into<Cow<'a, str>>) -> Self {
        Self {
            kind: Cow::Borrowed("login"),
            name: name.into(),
            login: Cow::Borrowed(""),
            password: Cow::Borrowed(""),
            urls: Vec::new(),
            comment: Cow::Borrowed(""),
            totp: None,
            attachments: Vec::new(),
            ssh_key: None,
            password_history: Vec::new(),
        }
    }

    pub fn into_owned(self) -> ImportedItem<'static> {
        ImportedItem {
            kind: Cow::Owned(self.kind.into_owned()),
            name: Cow::Owned(self.name.into_owned()),
            login: Cow::Owned(self.login.into_owned()),
            password: Cow::Owned(self.password.into_owned()),
            urls: self
                .urls
                .into_iter()
                .map(|url| Cow::Owned(url.into_owned()))
                .collect(),
            comment: Cow::Owned(self.comment.into_owned()),
            totp: self.totp,
            attachments: self
                .attachments
                .into_iter()
                .map(ImportedAttachment::into_owned)
                .collect(),
            ssh_key: self.ssh_key.map(ImportedSshKey::into_owned),
            password_history: self
                .password_history
                .into_iter()
                .map(ImportedHistoryItem::into_owned)
                .collect(),
        }
    }

    /// Adds a previous password to the imported history, preserving the order
    /// of the export. Repeats and the current password are ignored.
    pub(crate) fn push_password_history(
        &mut self,
        value: impl Into<Cow<'a, str>>,
        changed_at: Option<Cow<'a, str>>,
    ) {
        let value = value.into();
        if value.is_empty() || value == self.password {
            return;
        }
        if self.password_history.iter().any(|item| item.value == value) {
            return;
        }
        self.password_history
            .push(ImportedHistoryItem { value, changed_at });
    }
}

impl ImportedHistoryItem<'_> {
    pub fn into_owned(self) -> ImportedHistoryItem<'static> {
        ImportedHistoryItem {
            value: Cow::Owned(self.value.into_owned()),
            changed_at: self.changed_at.map(|value| Cow::Owned(value.into_owned())),
        }
    }
}

impl ImportedAttachment<'_> {
    pub fn into_owned(self) -> ImportedAttachment<'static> {
        ImportedAttachment {
            name: Cow::Owned(self.name.into_owned()),
            mime: Cow::Owned(self.mime.into_owned()),
            b64: Cow::Owned(self.b64.into_owned()),
        }
    }
}

impl ImportedSshKey<'_> {
    pub fn into_owned(self) -> ImportedSshKey<'static> {
        ImportedSshKey {
            name: Cow::Owned(self.name.into_owned()),
            public_key: Cow::Owned(self.public_key.into_owned()),
            private_key: Cow::Owned(self.private_key.into_owned()),
            passphrase: self.passphrase.map(|value| Cow::Owned(value.into_owned())),
            comment: Cow::Owned(self.comment.into_owned()),
        }
    }
}

/// A JSON string that stays borrowed when the parser can hand out a slice of
/// the input (no escapes) and is copied only when it has to be unescaped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Text<'a>(pub Cow<'a, str>);

impl<'a> Text<'a> {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn trimmed(&self) -> &str {
        self.0.trim()
    }

    pub(crate) fn into_cow(self) -> Cow<'a, str> {
        self.0
    }
}

impl<'a> From<&'a str> for Text<'a> {
    fn from(value: &'a str) -> Self {
        Text(Cow::Borrowed(value))
    }
}

impl<'a> From<String> for Text<'a> {
    fn from(value: String) -> Self {
        Text(Cow::Owned(value))
    }
}

impl<'de: 'a, 'a> serde::Deserialize<'de> for Text<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TextVisitor;

        impl<'de> Visitor<'de> for TextVisitor {
            type Value = Text<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Borrowed(value)))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value.to_owned())))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value)))
            }
        }

        deserializer.deserialize_string(TextVisitor)
    }
}

/// Trims a borrowed/owned string without losing the borrow when possible.
pub(crate) fn trim_cow(value: Cow<'_, str>) -> Cow<'_, str> {
    match value {
        Cow::Borrowed(value) => Cow::Borrowed(value.trim()),
        Cow::Owned(value) => {
            let trimmed_len = value.trim().len();
            if trimmed_len == value.len() {
                Cow::Owned(value)
            } else {
                Cow::Owned(value.trim().to_string())
            }
        }
    }
}

/// Accepts a string, `null` or a missing field and yields a borrowed `Cow`.
pub(crate) fn cow_str<'de, D>(deserializer: D) -> Result<Cow<'de, str>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Text<'de>>::deserialize(deserializer)?
        .map(Text::into_cow)
        .unwrap_or_default())
}

fn decode_text(bytes: &[u8]) -> Cow<'_, str> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Cow::Owned(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        return Cow::Owned(String::from_utf16_lossy(&units));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<_>>();
        return Cow::Owned(String::from_utf16_lossy(&units));
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(String::from_utf8_lossy(bytes).into_owned()),
    }
}

/// Only looks at the top-level keys, without building a JSON tree.
#[derive(serde::Deserialize)]
struct ExportProbe {
    #[serde(default)]
    accounts: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    items: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    encrypted: Option<serde::de::IgnoredAny>,
}

pub fn parse<'a>(
    bytes: &'a [u8],
    password: Option<&str>,
    keyfile: Option<&[u8]>,
) -> Result<ImportBundle<'a>, VaultError> {
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
            // The JSON lives in the archive, so it is a temporary buffer:
            // detach the result from it.
            return Ok(
                super::import_bitwarden::parse(&data, password, &attachments)?.into_owned(),
            );
        }
        return Err(VaultError::ImportUnsupported);
    }

    match decode_text(bytes) {
        Cow::Borrowed(text) => parse_text(text, password),
        Cow::Owned(text) => Ok(parse_text(&text, password)?.into_owned()),
    }
}

fn parse_text<'a>(text: &'a str, password: Option<&str>) -> Result<ImportBundle<'a>, VaultError> {
    if text.trim_start().starts_with('{') {
        let probe: ExportProbe =
            serde_json::from_str(text).map_err(|_| VaultError::ImportCorrupt)?;
        if probe.accounts.is_some() {
            return super::import_onepux::parse_json(text.as_bytes(), &HashMap::new());
        }
        if probe.items.is_some() || probe.encrypted.is_some() {
            return super::import_bitwarden::parse(text.as_bytes(), password, &HashMap::new());
        }
        return Err(VaultError::ImportUnsupported);
    }
    if looks_like_kaspersky(text) {
        return parse_kaspersky(text);
    }
    if looks_like_csv(text) {
        return parse_csv(text);
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

fn parse_csv<'a>(text: &'a str) -> Result<ImportBundle<'a>, VaultError> {
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
    let mut items = Vec::with_capacity(256);
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
                    warnings.push(format!("Строка {}: пустое имя", row + 2).into());
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
                urls.push(Cow::Owned(item.trim().to_string()));
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
                    warnings.push(format!("Строка {}: TOTP не распознан", row + 2).into());
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
        let mut item = ImportedItem::new(Cow::Owned(name));
        item.kind = Cow::Borrowed(kind);
        item.login = Cow::Owned(login);
        item.password = Cow::Owned(password);
        item.urls = urls;
        item.comment = Cow::Owned(comment.trim().to_string());
        item.totp = totp_value;
        items.push(item);
    }

    if items.is_empty() {
        return Err(VaultError::ImportCorrupt);
    }
    Ok(ImportBundle {
        format: Cow::Borrowed("csv"),
        items,
        warnings,
    })
}

fn parse_kaspersky<'a>(text: &'a str) -> Result<ImportBundle<'a>, VaultError> {
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut current: Option<ImportedItem<'a>> = None;
    let mut field: Option<&str> = None;

    let mut finish = |current: &mut Option<ImportedItem<'a>>, items: &mut Vec<ImportedItem<'a>>| {
        if let Some(mut item) = current.take() {
            if item.name.trim().is_empty() {
                warnings.push(Cow::Borrowed("Пропущена запись Kaspersky без имени"));
                return;
            }
            let trimmed = item.comment.trim();
            if trimmed.len() != item.comment.len() {
                item.comment = Cow::Owned(trimmed.to_string());
            }
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
                let target = target.to_mut();
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
                current = Some(ImportedItem::new(value));
                field = Some("comment");
            }
            "Name" => {
                finish(&mut current, &mut items);
                current = Some(ImportedItem::new(value));
                field = Some("comment");
                if let Some(item) = current.as_mut() {
                    item.kind = Cow::Borrowed("note");
                }
            }
            "Website URL" => {
                if let Some(item) = current.as_mut() {
                    if !value.is_empty() {
                        item.urls.push(Cow::Borrowed(value));
                    }
                }
            }
            "Login name" | "Login" => {
                if let Some(item) = current.as_mut() {
                    if label == "Login" || item.login.is_empty() {
                        item.login = Cow::Borrowed(value);
                    }
                }
            }
            "Password" => {
                if let Some(item) = current.as_mut() {
                    item.password = Cow::Borrowed(value);
                }
            }
            "Comment" | "Text" => {
                if let Some(item) = current.as_mut() {
                    item.comment = Cow::Borrowed(value);
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
        format: Cow::Borrowed("kaspersky"),
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
