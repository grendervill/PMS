use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::error::VaultError;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const DEFAULT_SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?/";
const AMBIGUOUS: &[char] = &['0', 'O', '1', 'l', 'I', '|'];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PasswordOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub custom_symbols: Option<String>,
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self {
            length: 16,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            custom_symbols: None,
            exclude_ambiguous: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub custom_symbols: Option<String>,
    pub exclude_ambiguous: bool,
}

impl Default for LoginOptions {
    fn default() -> Self {
        Self {
            length: 12,
            lowercase: true,
            uppercase: false,
            digits: true,
            symbols: false,
            custom_symbols: None,
            exclude_ambiguous: true,
        }
    }
}

pub fn generate_password(options: &PasswordOptions) -> Result<String, VaultError> {
    if !(8..=128).contains(&options.length) {
        return Err(VaultError::BadParams);
    }

    let groups = selected_groups(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
    )?;
    if groups.len() > options.length {
        return Err(VaultError::BadParams);
    }

    let mut rng = rand::rngs::OsRng;
    let mut result = Vec::with_capacity(options.length);

    for group in &groups {
        result.push(*group.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
    }

    let combined: Vec<char> = groups.iter().flatten().copied().collect();
    while result.len() < options.length {
        result.push(*combined.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
    }

    result.shuffle(&mut rng);
    Ok(result.into_iter().collect())
}

pub fn generate_login(options: &LoginOptions) -> Result<String, VaultError> {
    if !(8..=64).contains(&options.length) {
        return Err(VaultError::BadParams);
    }

    let groups = selected_groups(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
    )?;
    let combined: Vec<char> = groups.into_iter().flatten().collect();
    if combined.is_empty() {
        return Err(VaultError::EmptyCharset);
    }

    let mut rng = rand::rngs::OsRng;
    let mut result = Vec::with_capacity(options.length);
    for _ in 0..options.length {
        result.push(*combined.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
    }

    Ok(result.into_iter().collect())
}

fn selected_groups(
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    custom_symbols: Option<&str>,
    exclude_ambiguous: bool,
) -> Result<Vec<Vec<char>>, VaultError> {
    let mut groups = Vec::with_capacity(4);

    if lowercase {
        groups.push(charset(LOWER, exclude_ambiguous));
    }
    if uppercase {
        groups.push(charset(UPPER, exclude_ambiguous));
    }
    if digits {
        groups.push(charset(DIGITS, exclude_ambiguous));
    }
    if symbols {
        let source = custom_symbols
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_SYMBOLS);
        groups.push(charset(source, exclude_ambiguous));
    }

    if groups.is_empty() || groups.iter().any(|group| group.is_empty()) {
        return Err(VaultError::EmptyCharset);
    }

    Ok(groups)
}

fn charset(source: &str, exclude_ambiguous: bool) -> Vec<char> {
    let mut chars: Vec<char> = source
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS.contains(c))
        .collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_options() -> PasswordOptions {
        PasswordOptions::default()
    }

    #[test]
    fn password_length_is_respected() {
        for length in [8, 16, 128] {
            let options = PasswordOptions {
                length,
                ..password_options()
            };
            assert_eq!(generate_password(&options).unwrap().chars().count(), length);
        }
    }

    #[test]
    fn password_uses_only_selected_classes() {
        let options = PasswordOptions {
            length: 64,
            lowercase: false,
            uppercase: false,
            digits: true,
            symbols: false,
            ..password_options()
        };
        let password = generate_password(&options).unwrap();

        assert!(password.chars().all(|c| c.is_ascii_digit()));
        assert!(password.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn password_contains_each_selected_class() {
        let options = PasswordOptions {
            length: 8,
            ..password_options()
        };
        let password = generate_password(&options).unwrap();

        assert!(password.chars().any(|c| c.is_ascii_lowercase()));
        assert!(password.chars().any(|c| c.is_ascii_uppercase()));
        assert!(password.chars().any(|c| c.is_ascii_digit()));
        assert!(password.chars().any(|c| !c.is_ascii_alphanumeric()));
    }

    #[test]
    fn password_excludes_ambiguous_characters() {
        let options = PasswordOptions {
            length: 128,
            ..password_options()
        };
        let password = generate_password(&options).unwrap();

        assert!(!password.chars().any(|c| AMBIGUOUS.contains(&c)));
    }

    #[test]
    fn password_rejects_length_out_of_range() {
        for length in [7, 129] {
            let options = PasswordOptions {
                length,
                ..password_options()
            };
            assert_eq!(
                generate_password(&options),
                Err(VaultError::BadParams)
            );
        }
    }

    #[test]
    fn password_rejects_empty_charset() {
        let options = PasswordOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..password_options()
        };
        assert_eq!(
            generate_password(&options),
            Err(VaultError::EmptyCharset)
        );
    }

    #[test]
    fn login_default_is_twelve_lowercase_digits() {
        let login = generate_login(&LoginOptions::default()).unwrap();

        assert_eq!(login.chars().count(), 12);
        assert!(
            login
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn login_respects_custom_symbols() {
        let options = LoginOptions {
            length: 32,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: true,
            custom_symbols: Some("!@".to_string()),
            ..LoginOptions::default()
        };
        let login = generate_login(&options).unwrap();

        assert!(login.chars().all(|c| c == '!' || c == '@'));
    }

    #[test]
    fn login_length_limits() {
        for length in [7, 65] {
            let options = LoginOptions {
                length,
                ..LoginOptions::default()
            };
            assert_eq!(
                generate_login(&options),
                Err(VaultError::BadParams)
            );
        }
    }

    #[test]
    fn two_generations_differ() {
        let options = PasswordOptions {
            length: 32,
            ..password_options()
        };
        let first = generate_password(&options).unwrap();
        let second = generate_password(&options).unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn custom_symbols_are_used_and_filtered() {
        let options = PasswordOptions {
            length: 64,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: true,
            custom_symbols: Some("0O1lI|!@".to_string()),
            exclude_ambiguous: true,
        };
        let password = generate_password(&options).unwrap();

        assert!(password.chars().all(|c| c == '!' || c == '@'));
    }

    #[test]
    fn login_can_use_all_classes() {
        let options = LoginOptions {
            length: 64,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            ..LoginOptions::default()
        };
        let login = generate_login(&options).unwrap();

        assert!(login.chars().any(|c| c.is_ascii_lowercase()));
        assert!(login.chars().any(|c| c.is_ascii_uppercase()));
        assert!(login.chars().any(|c| c.is_ascii_digit()));
        assert!(login.chars().any(|c| !c.is_ascii_alphanumeric()));
    }
}
