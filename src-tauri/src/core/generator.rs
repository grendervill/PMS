use std::sync::OnceLock;

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::error::VaultError;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const DEFAULT_SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?/";
const AMBIGUOUS: &[char] = &['0', 'O', '1', 'l', 'I', '|'];

/// Word count of the bundled EFF large diceware list (6^5).
const DICEWARE_WORDS: usize = 7776;
const PASSPHRASE_MIN_WORDS: u8 = 3;
const PASSPHRASE_MAX_WORDS: u8 = 12;
/// Only these separators are accepted; arbitrary strings would silently change
/// the entropy story and make word boundaries ambiguous.
const PASSPHRASE_SEPARATORS: &[&str] = &["", "-", ".", "_", " ", "+", "=", ":"];
const PASSPHRASE_DEFAULT_SEPARATOR: &str = "-";
/// Sites reject whole classes of characters; a longer exclusion list than this
/// is almost certainly a payload mistake, not a real rule.
const MAX_EXCLUDE_CHARS: usize = 64;

static WORDLIST: OnceLock<Vec<&'static str>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Other,
    Digits,
    Symbols,
}

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
    /// Characters the site refuses; filtered out of every enabled set.
    pub exclude_chars: Option<String>,
    /// Minimum number of digits in the result (0 = no requirement).
    pub min_digits: u8,
    /// Minimum number of characters taken from the symbols set (0 = no requirement).
    pub min_symbols: u8,
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
            exclude_chars: None,
            min_digits: 0,
            min_symbols: 0,
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
    /// Characters the site refuses; filtered out of every enabled set.
    pub exclude_chars: Option<String>,
    /// Minimum number of digits in the result (0 = no requirement).
    pub min_digits: u8,
    /// Minimum number of characters taken from the symbols set (0 = no requirement).
    pub min_symbols: u8,
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
            exclude_chars: None,
            min_digits: 0,
            min_symbols: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PassphraseOptions {
    pub words: u8,
    pub separator: String,
    pub capitalize: bool,
    pub include_number: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        Self {
            words: 5,
            separator: PASSPHRASE_DEFAULT_SEPARATOR.to_string(),
            capitalize: false,
            include_number: false,
        }
    }
}

/// Entropy request from the UI; every variant is validated exactly like the
/// corresponding generator so the shown number cannot describe a rejected setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum EntropyInput {
    Password { options: PasswordOptions },
    Login { options: LoginOptions },
    Passphrase { options: PassphraseOptions },
}

pub fn generate_password(options: &PasswordOptions) -> Result<String, VaultError> {
    let groups = validate_password(options)?;

    let mut rng = rand::rngs::OsRng;
    let mut result: Vec<char> = Vec::with_capacity(options.length);
    let mut kinds: Vec<GroupKind> = Vec::with_capacity(options.length);

    // One character out of every enabled group, as before.
    for (kind, group) in &groups {
        let picked = *group.choose(&mut rng).ok_or(VaultError::EmptyCharset)?;
        result.push(picked);
        kinds.push(*kind);
    }

    let mut digit_count = count_kind(&kinds, GroupKind::Digits);
    let mut symbol_count = count_kind(&kinds, GroupKind::Symbols);
    let min_digits = usize::from(options.min_digits);
    let min_symbols = usize::from(options.min_symbols);

    // Explicit minimums win over the per-group guarantee: if both cannot fit in
    // `length`, drop group representatives instead of silently missing a minimum.
    loop {
        let extra =
            min_digits.saturating_sub(digit_count) + min_symbols.saturating_sub(symbol_count);
        if result.len() + extra <= options.length {
            break;
        }
        let droppable = (0..result.len()).rev().find(|index| match kinds[*index] {
            GroupKind::Other => true,
            GroupKind::Digits => digit_count > min_digits,
            GroupKind::Symbols => symbol_count > min_symbols,
        });
        let index = match droppable {
            Some(index) => index,
            None => break,
        };
        kinds.remove(index);
        result.remove(index);
        digit_count = count_kind(&kinds, GroupKind::Digits);
        symbol_count = count_kind(&kinds, GroupKind::Symbols);
    }

    if min_digits > digit_count {
        let group = group_of_kind(&groups, GroupKind::Digits).ok_or(VaultError::BadParams)?;
        for _ in 0..(min_digits - digit_count) {
            result.push(*group.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
        }
    }
    if min_symbols > symbol_count {
        let group = group_of_kind(&groups, GroupKind::Symbols).ok_or(VaultError::BadParams)?;
        for _ in 0..(min_symbols - symbol_count) {
            result.push(*group.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
        }
    }

    let combined = combined_charset(&groups);
    while result.len() < options.length {
        result.push(*combined.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
    }

    result.shuffle(&mut rng);
    Ok(result.into_iter().collect())
}

pub fn generate_login(options: &LoginOptions) -> Result<String, VaultError> {
    let groups = validate_login(options)?;
    let combined = combined_charset(&groups);

    let mut rng = rand::rngs::OsRng;
    let mut result: Vec<char> = Vec::with_capacity(options.length);

    // Minimums first, then free fill; no per-group guarantee for logins.
    for kind in [GroupKind::Digits, GroupKind::Symbols] {
        let wanted = match kind {
            GroupKind::Digits => usize::from(options.min_digits),
            GroupKind::Symbols => usize::from(options.min_symbols),
            GroupKind::Other => 0,
        };
        if wanted == 0 {
            continue;
        }
        let group = group_of_kind(&groups, kind).ok_or(VaultError::BadParams)?;
        for _ in 0..wanted {
            result.push(*group.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
        }
    }

    while result.len() < options.length {
        result.push(*combined.choose(&mut rng).ok_or(VaultError::EmptyCharset)?);
    }

    result.shuffle(&mut rng);
    Ok(result.into_iter().collect())
}

pub fn generate_passphrase(options: &PassphraseOptions) -> Result<String, VaultError> {
    validate_passphrase(options)?;

    let words = wordlist();
    if words.is_empty() {
        return Err(VaultError::EmptyCharset);
    }

    let mut rng = rand::rngs::OsRng;
    let mut parts: Vec<String> = Vec::with_capacity(usize::from(options.words));
    for _ in 0..options.words {
        let index = rng.gen_range(0..words.len());
        let word = *words.get(index).ok_or(VaultError::BadParams)?;
        parts.push(if options.capitalize {
            capitalize_word(word)
        } else {
            word.to_string()
        });
    }

    let mut phrase = parts.join(&options.separator);
    if options.include_number {
        let digit = b'0' + rng.gen_range(0u8..10);
        phrase.push(char::from(digit));
    }

    Ok(phrase)
}

pub fn entropy_bits(input: &EntropyInput) -> Result<f64, VaultError> {
    match input {
        EntropyInput::Password { options } => {
            validate_password(options)?;
            Ok(password_entropy_bits(options))
        }
        EntropyInput::Login { options } => {
            validate_login(options)?;
            Ok(login_entropy_bits(options))
        }
        EntropyInput::Passphrase { options } => {
            validate_passphrase(options)?;
            Ok(passphrase_entropy_bits(options))
        }
    }
}

fn password_entropy_bits(options: &PasswordOptions) -> f64 {
    let pool = session_pool(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
        options.exclude_chars.as_deref(),
    );
    entropy_from_pool(options.length, pool)
}

fn login_entropy_bits(options: &LoginOptions) -> f64 {
    let pool = session_pool(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
        options.exclude_chars.as_deref(),
    );
    entropy_from_pool(options.length, pool)
}

fn passphrase_entropy_bits(options: &PassphraseOptions) -> f64 {
    let per_word = (DICEWARE_WORDS as f64).log2();
    let mut bits = f64::from(options.words) * per_word;
    if options.include_number {
        bits += 10f64.log2();
    }
    bits
}

fn entropy_from_pool(length: usize, pool: usize) -> f64 {
    if pool < 2 {
        return 0.0;
    }
    length as f64 * (pool as f64).log2()
}

fn session_pool(
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    custom_symbols: Option<&str>,
    exclude_ambiguous: bool,
    exclude_chars: Option<&str>,
) -> usize {
    let groups = raw_groups(
        lowercase,
        uppercase,
        digits,
        symbols,
        custom_symbols,
        exclude_ambiguous,
        exclude_chars,
    );
    combined_charset(&groups).len()
}

/// An empty string means "nothing to exclude" so the UI can send it as-is;
/// overlong lists are rejected instead of silently truncated.
fn validate_exclude_chars(exclude_chars: Option<&str>) -> Result<(), VaultError> {
    if let Some(value) = exclude_chars {
        if value.chars().count() > MAX_EXCLUDE_CHARS {
            return Err(VaultError::BadParams);
        }
    }
    Ok(())
}

fn validate_password(options: &PasswordOptions) -> Result<Vec<(GroupKind, Vec<char>)>, VaultError> {
    validate_exclude_chars(options.exclude_chars.as_deref())?;
    if !(8..=128).contains(&options.length) {
        return Err(VaultError::BadParams);
    }
    validate_minimums(
        options.length,
        options.digits,
        options.symbols,
        options.min_digits,
        options.min_symbols,
    )?;

    let groups = selected_groups(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
        options.exclude_chars.as_deref(),
    )?;
    if groups.len() > options.length {
        return Err(VaultError::BadParams);
    }

    Ok(groups)
}

fn validate_login(options: &LoginOptions) -> Result<Vec<(GroupKind, Vec<char>)>, VaultError> {
    validate_exclude_chars(options.exclude_chars.as_deref())?;
    if !(8..=64).contains(&options.length) {
        return Err(VaultError::BadParams);
    }
    validate_minimums(
        options.length,
        options.digits,
        options.symbols,
        options.min_digits,
        options.min_symbols,
    )?;

    selected_groups(
        options.lowercase,
        options.uppercase,
        options.digits,
        options.symbols,
        options.custom_symbols.as_deref(),
        options.exclude_ambiguous,
        options.exclude_chars.as_deref(),
    )
}

fn validate_minimums(
    length: usize,
    digits: bool,
    symbols: bool,
    min_digits: u8,
    min_symbols: u8,
) -> Result<(), VaultError> {
    let digits_min = usize::from(min_digits);
    let symbols_min = usize::from(min_symbols);

    if digits_min + symbols_min > length {
        return Err(VaultError::BadParams);
    }
    if digits_min > 0 && !digits {
        return Err(VaultError::BadParams);
    }
    if symbols_min > 0 && !symbols {
        return Err(VaultError::BadParams);
    }

    Ok(())
}

fn validate_passphrase(options: &PassphraseOptions) -> Result<(), VaultError> {
    if !(PASSPHRASE_MIN_WORDS..=PASSPHRASE_MAX_WORDS).contains(&options.words) {
        return Err(VaultError::BadParams);
    }
    if !PASSPHRASE_SEPARATORS.contains(&options.separator.as_str()) {
        return Err(VaultError::BadParams);
    }

    Ok(())
}

fn selected_groups(
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    custom_symbols: Option<&str>,
    exclude_ambiguous: bool,
    exclude_chars: Option<&str>,
) -> Result<Vec<(GroupKind, Vec<char>)>, VaultError> {
    let groups = raw_groups(
        lowercase,
        uppercase,
        digits,
        symbols,
        custom_symbols,
        exclude_ambiguous,
        exclude_chars,
    );

    // A set emptied by the exclusion list is an error, not something to drop:
    // silently losing a class would break the promised charset.
    if groups.is_empty() || groups.iter().any(|(_, group)| group.is_empty()) {
        return Err(VaultError::EmptyCharset);
    }

    Ok(groups)
}

fn raw_groups(
    lowercase: bool,
    uppercase: bool,
    digits: bool,
    symbols: bool,
    custom_symbols: Option<&str>,
    exclude_ambiguous: bool,
    exclude_chars: Option<&str>,
) -> Vec<(GroupKind, Vec<char>)> {
    let mut groups = Vec::with_capacity(4);

    if lowercase {
        groups.push((
            GroupKind::Other,
            charset(LOWER, exclude_ambiguous, exclude_chars),
        ));
    }
    if uppercase {
        groups.push((
            GroupKind::Other,
            charset(UPPER, exclude_ambiguous, exclude_chars),
        ));
    }
    if digits {
        groups.push((
            GroupKind::Digits,
            charset(DIGITS, exclude_ambiguous, exclude_chars),
        ));
    }
    if symbols {
        let source = custom_symbols
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_SYMBOLS);
        groups.push((
            GroupKind::Symbols,
            charset(source, exclude_ambiguous, exclude_chars),
        ));
    }

    groups
}

fn group_of_kind(groups: &[(GroupKind, Vec<char>)], kind: GroupKind) -> Option<&[char]> {
    groups
        .iter()
        .find(|(group_kind, _)| *group_kind == kind)
        .map(|(_, chars)| chars.as_slice())
}

fn combined_charset(groups: &[(GroupKind, Vec<char>)]) -> Vec<char> {
    let mut chars: Vec<char> = groups
        .iter()
        .flat_map(|(_, group)| group.iter().copied())
        .collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

fn count_kind(kinds: &[GroupKind], kind: GroupKind) -> usize {
    kinds.iter().filter(|value| **value == kind).count()
}

fn charset(source: &str, exclude_ambiguous: bool, exclude_chars: Option<&str>) -> Vec<char> {
    let excluded: Vec<char> = exclude_chars
        .map(|value| value.chars().collect())
        .unwrap_or_default();
    let mut chars: Vec<char> = source
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS.contains(c))
        .filter(|c| !excluded.contains(c))
        .collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

fn wordlist() -> &'static [&'static str] {
    WORDLIST.get_or_init(|| {
        include_str!("../../assets/wordlists/eff_large.txt")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect()
    })
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(word.len());
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_options() -> PasswordOptions {
        PasswordOptions::default()
    }

    fn count_chars(password: &str, source: &str, exclude_ambiguous: bool) -> usize {
        let set = charset(source, exclude_ambiguous, None);
        password.chars().filter(|c| set.contains(c)).count()
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
            assert_eq!(generate_password(&options), Err(VaultError::BadParams));
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
        assert_eq!(generate_password(&options), Err(VaultError::EmptyCharset));
    }

    #[test]
    fn login_default_is_twelve_lowercase_digits() {
        let login = generate_login(&LoginOptions::default()).unwrap();

        assert_eq!(login.chars().count(), 12);
        assert!(login
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
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
            assert_eq!(generate_login(&options), Err(VaultError::BadParams));
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
            exclude_chars: None,
            min_digits: 0,
            min_symbols: 0,
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

    #[test]
    fn password_minimums_are_respected() {
        let cases = [
            // (length, digits, symbols, min_digits, min_symbols, custom_symbols)
            (16, true, true, 5, 3, None),
            (8, true, true, 8, 0, None),
            (20, true, true, 0, 6, None),
            (12, true, true, 2, 2, Some("!@#$")),
            (16, true, true, 16, 0, None),
            (16, true, true, 0, 16, None),
            (10, true, true, 7, 3, None),
        ];

        for (length, digits, symbols, min_digits, min_symbols, custom_symbols) in cases {
            let options = PasswordOptions {
                length,
                lowercase: true,
                uppercase: true,
                digits,
                symbols,
                custom_symbols: custom_symbols.map(str::to_string),
                exclude_ambiguous: true,
                exclude_chars: None,
                min_digits,
                min_symbols,
            };
            let symbol_source = custom_symbols.unwrap_or(DEFAULT_SYMBOLS);

            for _ in 0..300 {
                let password = generate_password(&options).unwrap();
                assert_eq!(password.chars().count(), length);
                assert!(
                    count_chars(&password, DIGITS, true) >= usize::from(min_digits),
                    "not enough digits in {password}"
                );
                assert!(
                    count_chars(&password, symbol_source, true) >= usize::from(min_symbols),
                    "not enough symbols in {password}"
                );
            }
        }
    }

    #[test]
    fn login_minimums_are_respected() {
        let cases = [(12, 4, 2), (8, 8, 0), (16, 0, 5), (9, 5, 4)];

        for (length, min_digits, min_symbols) in cases {
            let options = LoginOptions {
                length,
                lowercase: true,
                uppercase: false,
                digits: true,
                symbols: true,
                custom_symbols: None,
                exclude_ambiguous: true,
                exclude_chars: None,
                min_digits,
                min_symbols,
            };

            for _ in 0..300 {
                let login = generate_login(&options).unwrap();
                assert_eq!(login.chars().count(), length);
                assert!(count_chars(&login, DIGITS, true) >= usize::from(min_digits));
                assert!(count_chars(&login, DEFAULT_SYMBOLS, true) >= usize::from(min_symbols));
            }
        }
    }

    #[test]
    fn password_rejects_bad_minimum_combinations() {
        let cases = [
            // min_digits + min_symbols > length
            PasswordOptions {
                length: 8,
                digits: true,
                symbols: true,
                min_digits: 5,
                min_symbols: 4,
                ..password_options()
            },
            // min_digits without digits
            PasswordOptions {
                length: 16,
                digits: false,
                symbols: true,
                min_digits: 1,
                min_symbols: 0,
                ..password_options()
            },
            // min_symbols without symbols
            PasswordOptions {
                length: 16,
                digits: true,
                symbols: false,
                min_digits: 0,
                min_symbols: 1,
                ..password_options()
            },
            // both requirements above the length
            PasswordOptions {
                length: 8,
                digits: true,
                symbols: true,
                min_digits: 9,
                min_symbols: 0,
                ..password_options()
            },
        ];

        for options in cases {
            assert_eq!(generate_password(&options), Err(VaultError::BadParams));
            assert_eq!(
                entropy_bits(&EntropyInput::Password {
                    options: options.clone()
                }),
                Err(VaultError::BadParams)
            );
        }
    }

    #[test]
    fn login_rejects_bad_minimum_combinations() {
        let cases = [
            LoginOptions {
                length: 12,
                digits: true,
                symbols: true,
                min_digits: 7,
                min_symbols: 6,
                ..LoginOptions::default()
            },
            LoginOptions {
                length: 12,
                digits: false,
                symbols: true,
                min_digits: 2,
                min_symbols: 0,
                ..LoginOptions::default()
            },
            LoginOptions {
                length: 12,
                digits: true,
                symbols: false,
                min_digits: 0,
                min_symbols: 2,
                ..LoginOptions::default()
            },
        ];

        for options in cases {
            assert_eq!(generate_login(&options), Err(VaultError::BadParams));
            assert_eq!(
                entropy_bits(&EntropyInput::Login {
                    options: options.clone()
                }),
                Err(VaultError::BadParams)
            );
        }
    }

    #[test]
    fn passphrase_defaults_are_five_dash_separated_lowercase_words() {
        let phrase = generate_passphrase(&PassphraseOptions::default()).unwrap();
        assert!(phrase.chars().all(|c| c.is_ascii_lowercase() || c == '-'));

        // Four diceware words contain hyphens (drop-down, felt-tip, t-shirt,
        // yo-yo), so the word count is checked with an unambiguous separator.
        let options = PassphraseOptions {
            words: 5,
            separator: ".".to_string(),
            ..PassphraseOptions::default()
        };
        let dotted = generate_passphrase(&options).unwrap();
        let parts: Vec<&str> = dotted.split('.').collect();
        assert_eq!(parts.len(), 5);
        assert!(parts.iter().all(|part| wordlist().contains(part)));
    }

    #[test]
    fn passphrase_word_boundaries() {
        let cases = [
            (3u8, "."),
            (5, " "),
            (12, "."),
            (7, "_"),
            (4, "+"),
            (6, "="),
            (8, ":"),
        ];

        for (words, separator) in cases {
            let options = PassphraseOptions {
                words,
                separator: separator.to_string(),
                ..PassphraseOptions::default()
            };
            let phrase = generate_passphrase(&options).unwrap();
            let parts: Vec<&str> = phrase.split(separator).collect();

            assert_eq!(parts.len(), usize::from(words), "phrase {phrase}");
            assert!(
                parts.iter().all(|part| wordlist().contains(part)),
                "unknown word in {phrase}"
            );
        }

        // Empty separator: only wordlist characters survive, boundaries are gone.
        let options = PassphraseOptions {
            words: 4,
            separator: String::new(),
            ..PassphraseOptions::default()
        };
        let phrase = generate_passphrase(&options).unwrap();
        assert!(phrase.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        assert!(phrase.chars().count() >= 4 * 3);
    }

    #[test]
    fn passphrase_capitalizes_each_word() {
        let options = PassphraseOptions {
            words: 6,
            separator: "_".to_string(),
            capitalize: true,
            include_number: false,
        };
        let phrase = generate_passphrase(&options).unwrap();

        for part in phrase.split('_') {
            assert!(!part.is_empty(), "empty word in {phrase}");
            assert!(part.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
            assert!(part
                .chars()
                .skip(1)
                .all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }

    #[test]
    fn passphrase_can_append_a_number() {
        let options = PassphraseOptions {
            words: 4,
            separator: ".".to_string(),
            capitalize: false,
            include_number: true,
        };

        for _ in 0..50 {
            let phrase = generate_passphrase(&options).unwrap();
            assert!(phrase.chars().last().is_some_and(|c| c.is_ascii_digit()));
            let body = &phrase[..phrase.len() - 1];
            let parts: Vec<&str> = body.split('.').collect();
            assert_eq!(parts.len(), 4);
            assert!(parts.iter().all(|part| wordlist().contains(part)));
        }
    }

    #[test]
    fn passphrase_rejects_bad_parameters() {
        for words in [0u8, 1, 2, 13, 200] {
            let options = PassphraseOptions {
                words,
                ..PassphraseOptions::default()
            };
            assert_eq!(
                generate_passphrase(&options),
                Err(VaultError::BadParams),
                "words={words}"
            );
            assert_eq!(
                entropy_bits(&EntropyInput::Passphrase {
                    options: options.clone()
                }),
                Err(VaultError::BadParams)
            );
        }

        for separator in ["*", "--", "|", "\n", " - "] {
            let options = PassphraseOptions {
                words: 5,
                separator: separator.to_string(),
                ..PassphraseOptions::default()
            };
            assert_eq!(generate_passphrase(&options), Err(VaultError::BadParams));
        }
    }

    #[test]
    fn wordlist_is_complete_and_clean() {
        let words = wordlist();

        assert_eq!(words.len(), DICEWARE_WORDS);

        let mut sorted: Vec<&str> = words.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), DICEWARE_WORDS, "wordlist has duplicates");

        for word in words {
            assert!(!word.is_empty());
            assert!(
                word.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "unexpected word {word}"
            );
        }
    }

    #[test]
    fn passphrase_entropy_matches_word_count() {
        let base = PassphraseOptions {
            words: 6,
            ..PassphraseOptions::default()
        };
        let bits = entropy_bits(&EntropyInput::Passphrase {
            options: base.clone(),
        })
        .unwrap();
        assert!((bits - 77.55).abs() < 0.01, "bits={bits}");

        let with_number = PassphraseOptions {
            include_number: true,
            ..base
        };
        let bits_with_number = entropy_bits(&EntropyInput::Passphrase {
            options: with_number,
        })
        .unwrap();
        assert!(
            ((bits_with_number - bits) - 3.32).abs() < 0.01,
            "delta={}",
            bits_with_number - bits
        );
    }

    #[test]
    fn password_entropy_matches_pool_size() {
        let options = PasswordOptions {
            length: 16,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            custom_symbols: None,
            exclude_ambiguous: true,
            exclude_chars: None,
            min_digits: 0,
            min_symbols: 0,
        };

        let mut pool: Vec<char> = Vec::new();
        for source in [LOWER, UPPER, DIGITS, DEFAULT_SYMBOLS] {
            pool.extend(
                source
                    .chars()
                    .filter(|c| c.is_ascii_graphic())
                    .filter(|c| !AMBIGUOUS.contains(c)),
            );
        }
        pool.sort_unstable();
        pool.dedup();
        let expected = 16.0 * (pool.len() as f64).log2();

        let bits = entropy_bits(&EntropyInput::Password {
            options: options.clone(),
        })
        .unwrap();
        assert!(
            (bits - expected).abs() < 1e-9,
            "bits={bits} expected={expected}"
        );

        // Excluding ambiguous characters must lower the reported entropy.
        let without_filter = PasswordOptions {
            exclude_ambiguous: false,
            ..options.clone()
        };
        let bits_without_filter = entropy_bits(&EntropyInput::Password {
            options: without_filter,
        })
        .unwrap();
        assert!(bits < bits_without_filter);
    }

    #[test]
    fn legacy_options_payloads_deserialize_with_zero_minimums() {
        let old = r#"{
            "length": 20,
            "lowercase": true,
            "uppercase": false,
            "digits": true,
            "symbols": false,
            "customSymbols": null,
            "excludeAmbiguous": true
        }"#;
        let options: PasswordOptions = serde_json::from_str(old).unwrap();
        assert_eq!(options.length, 20);
        assert_eq!(options.min_digits, 0);
        assert_eq!(options.min_symbols, 0);

        let login: LoginOptions = serde_json::from_str(old).unwrap();
        assert_eq!(login.min_digits, 0);
        assert_eq!(login.min_symbols, 0);
    }

    #[test]
    fn entropy_input_matches_frontend_payload() {
        let password = r#"{"mode":"password","options":{"length":24,"minDigits":4}}"#;
        let parsed: EntropyInput = serde_json::from_str(password).unwrap();
        match parsed {
            EntropyInput::Password { options } => {
                assert_eq!(options.length, 24);
                assert_eq!(options.min_digits, 4);
            }
            _ => panic!("expected a password variant"),
        }

        let passphrase = r#"{"mode":"passphrase","options":{"words":6}}"#;
        let parsed: EntropyInput = serde_json::from_str(passphrase).unwrap();
        match parsed {
            EntropyInput::Passphrase { options } => {
                assert_eq!(options.words, 6);
                assert_eq!(options.separator, "-");
            }
            _ => panic!("expected a passphrase variant"),
        }
    }

    #[test]
    fn excluded_chars_never_appear_in_passwords() {
        let exclude = "<>\"'`;/\\";
        let options = PasswordOptions {
            length: 64,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            custom_symbols: Some([DEFAULT_SYMBOLS, "<>\"'`;/\\"].concat()),
            exclude_ambiguous: true,
            exclude_chars: Some(exclude.to_string()),
            min_digits: 2,
            min_symbols: 2,
        };

        for _ in 0..300 {
            let password = generate_password(&options).unwrap();
            assert_eq!(password.chars().count(), 64);
            for bad in exclude.chars() {
                assert!(
                    !password.contains(bad),
                    "found excluded {bad:?} in {password}"
                );
            }
        }
    }

    #[test]
    fn excluded_chars_never_appear_in_logins() {
        let options = LoginOptions {
            length: 32,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            custom_symbols: None,
            exclude_ambiguous: false,
            exclude_chars: Some("aeiou0123!@".to_string()),
            min_digits: 0,
            min_symbols: 0,
        };

        for _ in 0..300 {
            let login = generate_login(&options).unwrap();
            assert_eq!(login.chars().count(), 32);
            for bad in "aeiou0123!@".chars() {
                assert!(!login.contains(bad), "found excluded {bad:?} in {login}");
            }
        }
    }

    #[test]
    fn excluded_chars_lower_the_reported_entropy() {
        let base = PasswordOptions {
            length: 20,
            ..PasswordOptions::default()
        };
        let with_exclusion = PasswordOptions {
            exclude_chars: Some("abcdefghij".to_string()),
            ..base.clone()
        };

        let bits = entropy_bits(&EntropyInput::Password {
            options: base.clone(),
        })
        .unwrap();
        let reduced = entropy_bits(&EntropyInput::Password {
            options: with_exclusion.clone(),
        })
        .unwrap();
        assert!(reduced < bits, "reduced={reduced} bits={bits}");

        // The reported pool must be the real one: 26-10 lowercase, 26 upper,
        // 10 digits, 24 symbols minus the ambiguous ones.
        let pool = combined_charset(&raw_groups(
            true,
            true,
            true,
            true,
            None,
            true,
            Some("abcdefghij"),
        ))
        .len();
        let expected = 20.0 * (pool as f64).log2();
        assert!((reduced - expected).abs() < 1e-9);
    }

    #[test]
    fn fully_excluded_group_is_an_error() {
        let cases = [
            PasswordOptions {
                length: 16,
                lowercase: false,
                uppercase: false,
                digits: true,
                symbols: false,
                exclude_chars: Some(DIGITS.to_string()),
                ..PasswordOptions::default()
            },
            PasswordOptions {
                length: 16,
                lowercase: false,
                uppercase: false,
                digits: false,
                symbols: true,
                custom_symbols: Some("!@".to_string()),
                exclude_chars: Some("!@".to_string()),
                ..PasswordOptions::default()
            },
            PasswordOptions {
                length: 16,
                lowercase: true,
                uppercase: false,
                digits: false,
                symbols: false,
                exclude_chars: Some(LOWER.to_string()),
                ..PasswordOptions::default()
            },
        ];

        for options in cases {
            assert_eq!(generate_password(&options), Err(VaultError::EmptyCharset));
            assert_eq!(
                entropy_bits(&EntropyInput::Password {
                    options: options.clone()
                }),
                Err(VaultError::EmptyCharset)
            );
        }

        let login = LoginOptions {
            length: 12,
            lowercase: true,
            uppercase: false,
            digits: false,
            symbols: false,
            exclude_chars: Some(LOWER.to_string()),
            ..LoginOptions::default()
        };
        assert_eq!(generate_login(&login), Err(VaultError::EmptyCharset));
    }

    #[test]
    fn exclude_chars_combines_with_exclude_ambiguous() {
        let options = PasswordOptions {
            length: 64,
            lowercase: false,
            uppercase: false,
            digits: true,
            symbols: false,
            exclude_ambiguous: true,
            exclude_chars: Some("2345".to_string()),
            ..PasswordOptions::default()
        };

        for _ in 0..300 {
            let password = generate_password(&options).unwrap();
            // 0 and 1 are ambiguous, 2..=5 explicitly excluded: only 6-9 remain.
            assert!(
                password.chars().all(|c| ('6'..='9').contains(&c)),
                "unexpected character in {password}"
            );
        }

        // Both filters narrow the same pool, so entropy drops twice.
        let neither = PasswordOptions {
            length: 16,
            lowercase: false,
            uppercase: false,
            digits: true,
            symbols: false,
            exclude_ambiguous: false,
            exclude_chars: None,
            ..PasswordOptions::default()
        };
        let ambiguous_only = PasswordOptions {
            exclude_ambiguous: true,
            ..neither.clone()
        };
        let both = PasswordOptions {
            exclude_ambiguous: true,
            exclude_chars: Some("2345".to_string()),
            ..neither.clone()
        };
        let bits_none = entropy_bits(&EntropyInput::Password { options: neither }).unwrap();
        let bits_ambiguous = entropy_bits(&EntropyInput::Password {
            options: ambiguous_only,
        })
        .unwrap();
        let bits_both = entropy_bits(&EntropyInput::Password { options: both }).unwrap();
        assert!(bits_both < bits_ambiguous && bits_ambiguous < bits_none);
    }

    #[test]
    fn exclude_chars_limits_and_edge_cases() {
        // 64 characters are fine, 65 are not.
        let allowed = PasswordOptions {
            exclude_chars: Some("a".repeat(MAX_EXCLUDE_CHARS)),
            ..PasswordOptions::default()
        };
        assert!(generate_password(&allowed).is_ok());

        let too_long = PasswordOptions {
            exclude_chars: Some("a".repeat(MAX_EXCLUDE_CHARS + 1)),
            ..PasswordOptions::default()
        };
        assert_eq!(generate_password(&too_long), Err(VaultError::BadParams));
        assert_eq!(
            entropy_bits(&EntropyInput::Password {
                options: too_long.clone()
            }),
            Err(VaultError::BadParams)
        );

        let login_too_long = LoginOptions {
            exclude_chars: Some("a".repeat(MAX_EXCLUDE_CHARS + 1)),
            ..LoginOptions::default()
        };
        assert_eq!(generate_login(&login_too_long), Err(VaultError::BadParams));

        // An empty string means "nothing to exclude".
        let empty = PasswordOptions {
            exclude_chars: Some(String::new()),
            ..PasswordOptions::default()
        };
        let plain = PasswordOptions::default();
        assert_eq!(generate_password(&empty).unwrap().chars().count(), 16);
        assert!(
            (entropy_bits(&EntropyInput::Password { options: empty }).unwrap()
                - entropy_bits(&EntropyInput::Password { options: plain }).unwrap())
            .abs()
                < 1e-9
        );

        // Unicode exclusions count as characters, not bytes.
        let unicode = PasswordOptions {
            exclude_chars: Some("ж".repeat(MAX_EXCLUDE_CHARS)),
            ..PasswordOptions::default()
        };
        assert!(generate_password(&unicode).is_ok());
    }

    #[test]
    fn exclude_chars_payload_is_optional() {
        let with_field = r#"{"length":16,"excludeChars":"<>\""}"#;
        let parsed: PasswordOptions = serde_json::from_str(with_field).unwrap();
        assert_eq!(parsed.exclude_chars.as_deref(), Some("<>\""));

        let without_field = r#"{"length":16}"#;
        let parsed: PasswordOptions = serde_json::from_str(without_field).unwrap();
        assert_eq!(parsed.exclude_chars, None);
        let login: LoginOptions = serde_json::from_str(without_field).unwrap();
        assert_eq!(login.exclude_chars, None);
    }

    #[test]
    fn entropy_rejects_invalid_setups() {
        let password = PasswordOptions {
            length: 7,
            ..PasswordOptions::default()
        };
        assert_eq!(
            entropy_bits(&EntropyInput::Password { options: password }),
            Err(VaultError::BadParams)
        );

        let login = LoginOptions {
            length: 65,
            ..LoginOptions::default()
        };
        assert_eq!(
            entropy_bits(&EntropyInput::Login { options: login }),
            Err(VaultError::BadParams)
        );
    }
}
