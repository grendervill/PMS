//! Breach check against Have I Been Pwned via k-anonymity.
//!
//! Only the first five hex characters of the SHA-1 digest ever leave the
//! machine; the service answers with every suffix in that bucket and the match
//! happens locally. There is no cache and nothing is logged, so a checked
//! password leaves no trace beyond the in-flight request.

use std::time::Duration;

use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

use crate::commands::{ApiError, ApiResult};

const RANGE_ENDPOINT: &str = "https://api.pwnedpasswords.com/range/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// A range response is ~35 KB; 1 MiB leaves room for padding and still bounds
/// what a hostile or broken proxy can push into memory.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
/// HIBP pads every range response with fake entries so the response size does
/// not reveal the popularity of the prefix.
const PADDING_HEADER: &str = "Add-Padding";

fn unavailable() -> ApiError {
    ApiError::new("breach_unavailable", "Не удалось проверить утечки")
}

/// Splits SHA-1 into the public range prefix and the private suffix.
pub fn hash_prefix_and_suffix(password: &str) -> (String, String) {
    let digest = Sha1::digest(password.as_bytes());
    let hex = hex::encode_upper(digest);
    let (prefix, suffix) = hex.split_at(5);
    (prefix.to_string(), suffix.to_string())
}

/// Finds `suffix` in a `SUFFIX:COUNT` body and returns the count, or 0.
///
/// Malformed lines are skipped rather than failing the whole check: HIBP adds
/// padding entries and proxies occasionally inject banners.
pub fn parse_range(body: &str, suffix: &str) -> u32 {
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((candidate, count)) = line.split_once(':') else {
            continue;
        };
        if !candidate.trim().eq_ignore_ascii_case(suffix) {
            continue;
        }
        return count.trim().parse::<u32>().unwrap_or(0);
    }

    0
}

async fn fetch_range(prefix: &str) -> ApiResult<String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("PMS/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| unavailable())?;

    let mut response = client
        .get(format!("{RANGE_ENDPOINT}{prefix}"))
        .header(PADDING_HEADER, "true")
        .send()
        .await
        .map_err(|_| unavailable())?
        .error_for_status()
        .map_err(|_| unavailable())?;

    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(unavailable());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(unavailable());
        }
        body.extend_from_slice(&chunk);
    }

    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Number of known breaches containing `password` (0 = not found).
///
/// Network failures surface as `breach_unavailable` and never include the
/// password or the hash in the message.
#[tauri::command]
pub async fn password_breach_count(password: String) -> ApiResult<u32> {
    if password.is_empty() {
        return Ok(0);
    }

    // Zeroized on drop so the plaintext does not linger on the heap.
    let password = Zeroizing::new(password);
    let (prefix, suffix) = hash_prefix_and_suffix(&password);
    let body = fetch_range(&prefix).await?;

    Ok(parse_range(&body, &suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_the_known_sha1_vector() {
        // SHA-1("password") = 5baa61e4c9b93f3f0682250b6cf8331b7ee68fd8
        let (prefix, suffix) = hash_prefix_and_suffix("password");
        assert_eq!(prefix, "5BAA6");
        assert_eq!(suffix, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
        assert_eq!(format!("{prefix}{suffix}").len(), 40);
    }

    #[test]
    fn hashes_are_uppercase_for_the_service() {
        let (prefix, suffix) = hash_prefix_and_suffix("correct horse battery staple");
        assert!(prefix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        assert!(suffix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn parses_a_matching_suffix() {
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n\
                    0E3D9F1A0BC1D7C1D8A0D3B2F37E2E3D7A1B1C2D3:4\r\n";
        assert_eq!(
            parse_range(body, "1E4C9B93F3F0682250B6CF8331B7EE68FD8"),
            9659365
        );
        assert_eq!(
            parse_range(body, "1e4c9b93f3f0682250b6cf8331b7ee68fd8"),
            9659365
        );
        assert_eq!(parse_range(body, "0000000000000000000000000000000000A"), 0);
    }

    #[test]
    fn ignores_empty_bodies_and_junk() {
        assert_eq!(parse_range("", "ABCDE"), 0);
        assert_eq!(parse_range("\r\n\r\n", "ABCDE"), 0);
        assert_eq!(parse_range("not a range response", "ABCDE"), 0);
        assert_eq!(parse_range("ABCDE:notanumber", "ABCDE"), 0);
        // Padding rows without a count are skipped, a real row still matches.
        assert_eq!(parse_range("FFFFFF:0\n\nABCDE:7", "ABCDE"), 7);
    }

    #[test]
    fn does_not_fall_for_a_prefix_only_match() {
        let body = "ABCDEF:123\r\nABCDE1:456\r\n";
        assert_eq!(parse_range(body, "ABCDE"), 0);
        assert_eq!(parse_range(body, "ABCDE1"), 456);
    }
}
