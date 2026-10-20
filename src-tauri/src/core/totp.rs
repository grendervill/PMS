use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use url::Url;

use super::error::VaultError;
use super::models::{TotpAlgorithm, TotpConfig};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TotpCode {
    pub code: String,
    pub remaining: u32,
}

pub fn inspect(input: &str) -> Result<TotpConfig, VaultError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(VaultError::InvalidTotp);
    }

    let config = if trimmed.to_ascii_lowercase().starts_with("otpauth://") {
        parse_uri(trimmed)?
    } else {
        TotpConfig {
            secret: normalize_secret(trimmed),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: None,
            account: None,
        }
    };

    validate_config(&config)?;
    Ok(config)
}

pub fn code(config: &TotpConfig, at: Option<u64>) -> Result<TotpCode, VaultError> {
    validate_config(config)?;
    let secret = decode_secret(&config.secret)?;
    let time = at.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0)
    });
    let counter = time / config.period as u64;
    let message = counter.to_be_bytes();

    let digest = match config.algorithm {
        TotpAlgorithm::Sha1 => {
            let mut mac =
                Hmac::<Sha1>::new_from_slice(&secret).map_err(|_| VaultError::InvalidTotp)?;
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha256 => {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(&secret).map_err(|_| VaultError::InvalidTotp)?;
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha512 => {
            let mut mac =
                Hmac::<Sha512>::new_from_slice(&secret).map_err(|_| VaultError::InvalidTotp)?;
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
    };

    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    if offset + 4 > digest.len() {
        return Err(VaultError::InvalidTotp);
    }
    let binary = ((digest[offset] & 0x7f) as u32) << 24
        | (digest[offset + 1] as u32) << 16
        | (digest[offset + 2] as u32) << 8
        | digest[offset + 3] as u32;
    let modulo = 10u32.pow(config.digits as u32);
    let value = binary % modulo;
    let code = format!("{value:0width$}", width = config.digits as usize);
    let remaining = config.period - (time % config.period as u64) as u32;

    Ok(TotpCode { code, remaining })
}

fn parse_uri(input: &str) -> Result<TotpConfig, VaultError> {
    let url = Url::parse(input).map_err(|_| VaultError::InvalidTotp)?;
    if url.scheme() != "otpauth" || url.host_str() != Some("totp") {
        return Err(VaultError::InvalidTotp);
    }

    let mut secret = None;
    let mut issuer = None;
    let mut algorithm = TotpAlgorithm::Sha1;
    let mut digits = 6;
    let mut period = 30;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "secret" => secret = Some(normalize_secret(&value)),
            "issuer" => issuer = Some(value.into_owned()),
            "algorithm" => {
                algorithm = match value.to_ascii_uppercase().as_str() {
                    "SHA1" => TotpAlgorithm::Sha1,
                    "SHA256" => TotpAlgorithm::Sha256,
                    "SHA512" => TotpAlgorithm::Sha512,
                    _ => return Err(VaultError::InvalidTotp),
                }
            }
            "digits" => {
                digits = value.parse::<u8>().map_err(|_| VaultError::InvalidTotp)?;
            }
            "period" => {
                period = value.parse::<u32>().map_err(|_| VaultError::InvalidTotp)?;
            }
            _ => {}
        }
    }

    let label = url.path().trim_start_matches('/');
    let (label_issuer, account) = match label.split_once(':') {
        Some((issuer, account)) => (Some(issuer.replace("%20", " ")), Some(account.replace("%20", " "))),
        None => (None, Some(label.replace("%20", " "))),
    };

    let config = TotpConfig {
        secret: secret.ok_or(VaultError::InvalidTotp)?,
        algorithm,
        digits,
        period,
        issuer: issuer.or(label_issuer),
        account: account.filter(|value| !value.is_empty()),
    };
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &TotpConfig) -> Result<(), VaultError> {
    if config.digits != 6 && config.digits != 8 {
        return Err(VaultError::InvalidTotp);
    }
    if !(1..=300).contains(&config.period) {
        return Err(VaultError::InvalidTotp);
    }
    decode_secret(&config.secret)?;
    Ok(())
}

fn decode_secret(secret: &str) -> Result<Vec<u8>, VaultError> {
    let normalized = normalize_secret(secret);
    if normalized.is_empty() {
        return Err(VaultError::InvalidTotp);
    }
    BASE32_NOPAD
        .decode(normalized.as_bytes())
        .map_err(|_| VaultError::InvalidTotp)
}

fn normalize_secret(secret: &str) -> String {
    secret
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '=')
        .collect::<String>()
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret_for(value: &[u8]) -> String {
        BASE32_NOPAD.encode(value)
    }

    fn config(secret: &str, algorithm: TotpAlgorithm, digits: u8) -> TotpConfig {
        TotpConfig {
            secret: secret.to_string(),
            algorithm,
            digits,
            period: 30,
            issuer: None,
            account: None,
        }
    }

    #[test]
    fn matches_rfc_6238_sha1_vectors() {
        let secret = secret_for(b"12345678901234567890");
        let values = [
            (59, "94287082"),
            (1_111_111_109, "07081804"),
            (1_111_111_111, "14050471"),
            (1_234_567_890, "89005924"),
            (2_000_000_000, "69279037"),
            (20_000_000_000, "65353130"),
        ];

        for (time, expected) in values {
            let result = code(&config(&secret, TotpAlgorithm::Sha1, 8), Some(time)).unwrap();
            assert_eq!(result.code, expected, "time {time}");
        }
    }

    #[test]
    fn matches_rfc_6238_sha256_and_sha512_vectors() {
        let sha256 = secret_for(b"12345678901234567890123456789012");
        let sha512 = secret_for(b"1234567890123456789012345678901234567890123456789012345678901234");

        assert_eq!(
            code(&config(&sha256, TotpAlgorithm::Sha256, 8), Some(59))
                .unwrap()
                .code,
            "46119246"
        );
        assert_eq!(
            code(&config(&sha512, TotpAlgorithm::Sha512, 8), Some(59))
                .unwrap()
                .code,
            "90693936"
        );
    }

    #[test]
    fn reports_remaining_time() {
        let secret = secret_for(b"12345678901234567890");
        assert_eq!(
            code(&config(&secret, TotpAlgorithm::Sha1, 6), Some(30))
                .unwrap()
                .remaining,
            30
        );
        assert_eq!(
            code(&config(&secret, TotpAlgorithm::Sha1, 6), Some(31))
                .unwrap()
                .remaining,
            29
        );
    }

    #[test]
    fn parses_otpauth_uri() {
        let secret = secret_for(b"12345678901234567890");
        let input = format!(
            "otpauth://totp/GitHub:alex?secret={secret}&issuer=GitHub&algorithm=SHA256&digits=8&period=60"
        );
        let parsed = inspect(&input).unwrap();

        assert_eq!(parsed.algorithm, TotpAlgorithm::Sha256);
        assert_eq!(parsed.digits, 8);
        assert_eq!(parsed.period, 60);
        assert_eq!(parsed.issuer.as_deref(), Some("GitHub"));
        assert_eq!(parsed.account.as_deref(), Some("alex"));
    }

    #[test]
    fn accepts_plain_base32() {
        let secret = secret_for(b"12345678901234567890");
        let parsed = inspect(&format!(" {} ", secret.to_lowercase())).unwrap();
        assert_eq!(parsed.algorithm, TotpAlgorithm::Sha1);
        assert_eq!(parsed.digits, 6);
    }

    #[test]
    fn rejects_invalid_secrets_and_options() {
        assert_eq!(inspect("not base32!"), Err(VaultError::InvalidTotp));
        assert_eq!(inspect(""), Err(VaultError::InvalidTotp));
        assert_eq!(
            inspect("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=7"),
            Err(VaultError::InvalidTotp)
        );
        assert_eq!(
            inspect("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=0"),
            Err(VaultError::InvalidTotp)
        );
    }
}
