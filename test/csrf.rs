use super::*;

#[test]
fn csrf_token_round_trip() {
    let token = generate_csrf_token_with_timestamp();
    assert!(is_valid_csrf_token(&token));
    assert!(!is_valid_csrf_token(""));
    assert!(!is_valid_csrf_token("not-a-token"));
    assert!(!is_valid_csrf_token("1.2"));
}
