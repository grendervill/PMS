use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use crate::commands::{ApiError, ApiResult};

const FAVICON_ENDPOINT: &str = "https://www.google.com/s2/favicons";
const FAVICON_SIZE: u32 = 64;
const MAX_FAVICON_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

pub fn normalize_host(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = trimmed.strip_prefix("www.").unwrap_or(&trimmed);
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return None;
    }
    if !host.split('.').all(is_valid_label) {
        return None;
    }
    Some(host.to_string())
}

fn is_valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

pub fn is_image_content_type(value: &str) -> bool {
    let mime = value.split(';').next().unwrap_or("").trim();
    mime.starts_with("image/")
}

pub fn favicon_url(host: &str) -> ApiResult<String> {
    let mut url = reqwest::Url::parse(FAVICON_ENDPOINT).map_err(|_| {
        ApiError::new(
            "favicon_fetch_failed",
            "Не удалось подготовить запрос значка",
        )
    })?;
    url.query_pairs_mut()
        .append_pair("sz", &FAVICON_SIZE.to_string())
        .append_pair("domain", host);
    Ok(url.to_string())
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn fetch_error(error: reqwest::Error) -> ApiError {
    ApiError::new(
        "favicon_fetch_failed",
        format!("Не удалось загрузить значок: {error}"),
    )
}

#[tauri::command]
pub async fn favicon_fetch(host: String) -> ApiResult<String> {
    let host = normalize_host(&host)
        .ok_or_else(|| ApiError::new("favicon_invalid_host", "Некорректный домен значка"))?;
    let url = favicon_url(&host)?;

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(concat!("PMS/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(fetch_error)?;

    let mut response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "image/*")
        .send()
        .await
        .map_err(fetch_error)?
        .error_for_status()
        .map_err(fetch_error)?;

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !is_image_content_type(&content_type) {
        return Err(ApiError::new(
            "favicon_not_image",
            "Сервис вернул не изображение",
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_FAVICON_BYTES as u64)
    {
        return Err(ApiError::new("favicon_too_large", "Значок слишком большой"));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(fetch_error)? {
        if bytes.len() + chunk.len() > MAX_FAVICON_BYTES {
            return Err(ApiError::new("favicon_too_large", "Значок слишком большой"));
        }
        bytes.extend_from_slice(&chunk);
    }

    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("image/png")
        .trim()
        .to_ascii_lowercase();
    Ok(data_url(&mime, &bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hosts_for_the_favicon_service() {
        assert_eq!(normalize_host("GitHub.com"), Some("github.com".into()));
        assert_eq!(
            normalize_host(" www.GitHub.com. "),
            Some("github.com".into())
        );
        assert_eq!(
            normalize_host("sub.example.co.uk"),
            Some("sub.example.co.uk".into())
        );
        assert_eq!(
            normalize_host("xn--e1afmkfd.xn--p1ai"),
            Some("xn--e1afmkfd.xn--p1ai".into())
        );
    }

    #[test]
    fn rejects_values_that_are_not_hostnames() {
        for value in [
            "",
            "localhost",
            "https://github.com/path",
            "github.com/path",
            "-bad.example",
            "bad-.example",
            "bad..example",
            "bad_host.example",
        ] {
            assert_eq!(normalize_host(value), None, "{value}");
        }
        assert_eq!(normalize_host(&format!("{}.example", "a".repeat(64))), None);
    }

    #[test]
    fn recognizes_image_content_types() {
        assert!(is_image_content_type("image/png"));
        assert!(is_image_content_type("image/svg+xml"));
        assert!(is_image_content_type("image/x-icon; charset=binary"));
        assert!(!is_image_content_type("text/html"));
        assert!(!is_image_content_type(""));
    }

    #[test]
    fn builds_an_encoded_favicon_url() {
        let url = favicon_url("sub.example.com").expect("url");

        assert!(url.starts_with("https://www.google.com/s2/favicons?"));
        assert!(url.contains("sz=64"));
        assert!(url.contains("domain=sub.example.com"));
    }

    #[test]
    fn wraps_image_bytes_into_a_data_url() {
        let value = data_url("image/png", &[1, 2, 3]);

        assert_eq!(
            value,
            format!("data:image/png;base64,{}", STANDARD.encode([1, 2, 3]))
        );
    }
}
