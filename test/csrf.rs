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

#[test]
fn a_valid_cookie_is_reused_and_a_missing_one_is_minted() {
    let minted = generate_csrf_token_with_timestamp();
    let (reused, set_cookie) = token_for_form(Some(&minted));
    assert!(!set_cookie);
    assert_eq!(reused, minted);

    let (fresh, set_cookie) = token_for_form(None);
    assert!(set_cookie);
    assert!(is_valid_csrf_token(&fresh));

    let (replaced, set_cookie) = token_for_form(Some("not-a-token"));
    assert!(set_cookie);
    assert!(is_valid_csrf_token(&replaced));
    assert_ne!(replaced, minted);
}

#[test]
fn a_matching_cookie_and_field_is_accepted() {
    let token = generate_csrf_token_with_timestamp();
    assert!(accepts_submission(&token, &token));
}

#[test]
fn two_independently_valid_tokens_are_rejected() {
    let field = generate_csrf_token_with_timestamp();
    let cookie = generate_csrf_token_with_timestamp();
    assert!(is_valid_csrf_token(&field));
    assert!(is_valid_csrf_token(&cookie));
    assert!(!accepts_submission(&field, &cookie));
}

#[test]
fn a_valid_field_with_no_cookie_is_rejected() {
    let field = generate_csrf_token_with_timestamp();
    assert!(is_valid_csrf_token(&field));
    assert!(!accepts_submission(&field, ""));
}
