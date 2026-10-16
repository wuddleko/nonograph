use super::*;

#[test]
fn test_default_config() {
    let mut config = Config::default();
    assert_eq!(config.limits.title_max_length, 128);
    assert_eq!(config.limits.alias_max_length, 32);
    assert_eq!(config.limits.content_max_length, 128000);
    assert_eq!(config.server.port, 8000);
    assert_eq!(config.nostr.relays.len(), 3);
    assert_eq!(config.nostr.timeout_secs, 3);
    assert!(config.nostr.socks.is_empty());
    assert_eq!(config.socks_addr(), Ok(None));
    config.nostr.socks = "127.0.0.1:9050".to_string();
    assert_eq!(
        config.socks_addr(),
        Ok(Some("127.0.0.1:9050".parse().unwrap()))
    );
    config.nostr.socks = "not-a-socket".to_string();
    assert_eq!(config.socks_addr(), Err(()));
    config.nostr.socks = "   ".to_string();
    assert_eq!(config.socks_addr(), Ok(None));
}

#[test]
fn a_missing_config_is_the_default_and_a_bad_file_is_fatal() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.toml");
    let config = Config::load_from(&missing).unwrap();
    assert_eq!(config.server.port, Config::default().server.port);
    assert_eq!(
        config.limits.content_max_length,
        Config::default().limits.content_max_length
    );
    assert!(config.nostr.socks.is_empty());

    let unread = temp.path().join("Config.toml");
    std::fs::create_dir(&unread).unwrap();
    let error = Config::load_from(&unread).unwrap_err();
    assert!(error.starts_with("Failed to read Config.toml:"), "{error}");

    let parsed = temp.path().join("broken.toml");
    std::fs::write(&parsed, "[[[not toml").unwrap();
    let error = Config::load_from(&parsed).unwrap_err();
    assert!(error.starts_with("Failed to parse Config.toml:"), "{error}");

    let valid = temp.path().join("valid.toml");
    std::fs::write(
        &valid,
        r#"
[limits]
title_max_length = 128
alias_max_length = 32
content_max_length = 10
form_data_limit_kb = 512

[server]
port = 9
address = "127.0.0.1"
onion_url = ""
onion_hostname_file = "/var/lib/tor/hidden_service/hostname"

[security]
max_url_length = 4096
external_link_security = true
csrf_protection_enabled = true

[theme]
syntax_highlighting = "base16-ocean.dark"
"#,
    )
    .unwrap();
    let config = Config::load_from(&valid).unwrap();
    assert_eq!(config.server.port, 9);
    assert_eq!(config.limits.content_max_length, 10);
}

#[test]
fn rocket_address_overrides_the_config_file() {
    let mut config = Config::default();
    config.server.address = "127.0.0.1".to_string();
    assert_eq!(
        config.listen_address(None).unwrap().to_string(),
        "127.0.0.1"
    );
    assert_eq!(
        config.listen_address(Some("")).unwrap().to_string(),
        "127.0.0.1"
    );
    assert_eq!(
        config.listen_address(Some("   ")).unwrap().to_string(),
        "127.0.0.1"
    );
    assert_eq!(
        config.listen_address(Some("0.0.0.0")).unwrap().to_string(),
        "0.0.0.0"
    );
    assert!(config.listen_address(Some("everywhere")).is_err());
    config.server.address = "not-an-ip".to_string();
    assert!(config.listen_address(None).is_err());
}

#[test]
fn test_form_data_limit_bytes() {
    let config = Config::default();
    assert_eq!(
        config.form_data_limit_bytes(),
        (128_000 + 128 + 32) * 4 + 4096
    );

    let mut wide = Config::default();
    wide.limits.content_max_length = usize::MAX;
    assert_eq!(wide.form_data_limit_bytes(), u32::MAX);
}

#[test]
fn test_post_validation() {
    let config = Config::default();

    // Valid post
    assert!(config
        .validate_post("Test Title", "Test content", Some("Author"))
        .is_ok());

    // Empty title
    assert_eq!(
        config.validate_post("", "Test content", None).unwrap_err(),
        "title_required"
    );

    // Empty content
    assert_eq!(
        config.validate_post("Title", "", None).unwrap_err(),
        "content_required"
    );

    // Title too long
    let long_title = "x".repeat(200);
    assert_eq!(
        config
            .validate_post(&long_title, "Content", None)
            .unwrap_err(),
        "title_too_long"
    );

    // Content too long
    let long_content = "x".repeat(130000);
    assert_eq!(
        config
            .validate_post("Title", &long_content, None)
            .unwrap_err(),
        "content_too_long"
    );

    // Alias too long
    let long_alias = "x".repeat(50);
    assert_eq!(
        config
            .validate_post("Title", "Content", Some(&long_alias))
            .unwrap_err(),
        "alias_too_long"
    );

    let characters = "é".repeat(128);
    assert!(config.validate_post(&characters, "Content", None).is_ok());
    let over = "é".repeat(129);
    assert_eq!(
        config.validate_post(&over, "Content", None).unwrap_err(),
        "title_too_long"
    );
}

#[test]
fn test_normalize_onion_url() {
    assert_eq!(
        normalize_onion_url("abcd1234.onion"),
        Some("http://abcd1234.onion".to_string())
    );
    assert_eq!(
        normalize_onion_url("http://abcd1234.onion/index.html"),
        Some("http://abcd1234.onion/index.html".to_string())
    );
    assert_eq!(
        normalize_onion_url("https://abcd1234.onion"),
        Some("https://abcd1234.onion".to_string())
    );
    assert_eq!(
        normalize_onion_url("  abcd1234.onion\n"),
        Some("http://abcd1234.onion".to_string())
    );
    assert_eq!(normalize_onion_url("example.com"), None);
    assert_eq!(normalize_onion_url("http://example.com"), None);
    assert_eq!(normalize_onion_url(""), None);
    assert_eq!(normalize_onion_url("   "), None);
    assert_eq!(normalize_onion_url("abcd.onion\r\nSet-Cookie: x=1"), None);
    assert_eq!(normalize_onion_url("ftp://abcd.onion"), None);
}

#[test]
fn test_normalize_https_url() {
    assert_eq!(
        normalize_https_url("https://nonogra.ph/"),
        Some("https://nonogra.ph/".to_string())
    );
    assert_eq!(normalize_https_url("http://nonogra.ph"), None);
    assert_eq!(normalize_https_url("https://abcd.onion"), None);
    assert_eq!(normalize_https_url("javascript:alert(1)"), None);
    assert_eq!(normalize_https_url(""), None);
}

#[test]
fn test_csrf_configuration() {
    let default_config = Config::default();

    // Default should have CSRF protection enabled
    assert!(default_config.security.csrf_protection_enabled);

    // Test that all security settings have expected defaults
    assert_eq!(default_config.security.max_url_length, 4096);
    assert!(default_config.security.external_link_security);
    assert!(default_config.security.csrf_protection_enabled);
}
