use pms_server::config::{Config, ConfigError};
use pms_server::tls::{self, TlsMode};

const MINIMAL: &str = r#"
[server]
data_dir = "/tmp/pms"
"#;

#[test]
fn missing_tls_values_are_refused_by_default() {
    let config = Config::parse(MINIMAL).unwrap();

    assert!(config.tls.required);
    assert_eq!(config.quota.default_bytes, 1_073_741_824);
    assert_eq!(config.server.listen, "127.0.0.1:8443");
    assert_eq!(
        tls::resolve(&config).err(),
        Some(ConfigError::TlsRequired),
        "без сертификата и с required по умолчанию сервер не должен стартовать"
    );
}

#[test]
fn tls_can_be_disabled_explicitly() {
    let config = Config::parse(
        r#"
        [server]
        data_dir = "/tmp/pms"

        [tls]
        required = false
        mode = "off"
        "#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(tls::resolve(&config).unwrap(), TlsMode::Off);
}

#[test]
fn files_mode_without_paths_is_refused() {
    let config = Config::parse(
        r#"
        [server]
        data_dir = "/tmp/pms"
        "#,
    )
    .unwrap();

    assert_eq!(tls::resolve(&config).err(), Some(ConfigError::TlsRequired));
}

#[test]
fn zero_quota_is_rejected() {
    let config = Config::parse(
        r#"
        [server]
        data_dir = "/tmp/pms"

        [quota]
        default_bytes = 0
        "#,
    )
    .unwrap();

    assert_eq!(config.validate().err(), Some(ConfigError::BadQuota));
}

#[test]
fn zero_limits_are_rejected() {
    let config = Config::parse(
        r#"
        [server]
        data_dir = "/tmp/pms"

        [limits]
        max_connections = 0
        "#,
    )
    .unwrap();

    assert_eq!(config.validate().err(), Some(ConfigError::BadLimit));
}

#[test]
fn unknown_field_is_an_error() {
    assert!(matches!(
        Config::parse(
            r#"
            [server]
            data_dir = "/tmp/pms"
            listenx = "0.0.0.0:1"
            "#
        ),
        Err(ConfigError::Parse(_))
    ));
}

#[test]
fn example_config_parses() {
    let config = Config::parse(include_str!("../pms-server.example.toml")).unwrap();
    assert_eq!(config.quota.default_bytes, 1_073_741_824);
    assert!(config.tls.required);
}
