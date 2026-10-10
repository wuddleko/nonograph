use super::*;

#[test]
fn csrf_token_round_trip() {
    let token = generate_csrf_token_with_timestamp();
    assert!(is_valid_csrf_token(&token));
    assert!(!is_valid_csrf_token(""));
    assert!(!is_valid_csrf_token("not-a-token"));
    assert!(!is_valid_csrf_token("1.2"));
    let (data, mac) = token.split_once('.').unwrap();
    let mut forged = String::from(data);
    forged.push('.');
    forged.push(if mac.starts_with('a') { 'b' } else { 'a' });
    forged.push_str(&mac[1..]);
    assert!(!is_valid_csrf_token(&forged));
}

#[test]
fn a_public_checksum_is_not_a_csrf_token() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let data = format!("{now}:{}", "ab".repeat(32));
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let forged = format!("{data}.{:x}", hasher.finish());
    assert!(!is_valid_csrf_token(&forged));
}
