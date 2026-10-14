use super::*;

#[test]
fn canonical_public_relay_accepts_one_public_spelling() {
    assert_eq!(
        canonical_public_relay("wss://Relay.Damus.io/"),
        Some("wss://relay.damus.io".to_string())
    );
    assert_eq!(
        canonical_public_relay("wss://nos.lol/path/"),
        Some("wss://nos.lol/path".to_string())
    );
    assert_eq!(
        canonical_public_relay("wss://Relay.Damus.io/"),
        canonical_public_relay("wss://relay.damus.io")
    );
    assert_eq!(
        canonical_public_relay("wss://nos.lol/?x=1"),
        Some("wss://nos.lol?x=1".to_string())
    );
    assert_eq!(
        canonical_public_relay("wss://Example.Onion/"),
        Some("wss://example.onion".to_string())
    );
    assert_eq!(canonical_public_relay("ws://relay.damus.io"), None);
    assert_eq!(
        canonical_public_relay("wss://user:pass@relay.damus.io"),
        None
    );
    assert_eq!(canonical_public_relay("wss://192.168.1.1"), None);
}
