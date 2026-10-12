use super::*;

#[test]
fn list_urls_reads_a_custom_kind_and_tag() {
    let event = r#"{
        "kind": 7,
        "tags": [
            ["relay", "wss://Relay.Example/"],
            ["relay", "wss://other.example", "read"],
            ["relay", "wss://third.example", "write"],
            ["r", "wss://skipped.example"],
            ["relay", "wss://relay.example/", "write"],
            ["relay", "ws://bad.example"]
        ]
    }"#;

    assert_eq!(
        list_urls_with(event, 7, "relay"),
        vec![
            "wss://relay.example".to_string(),
            "wss://other.example".to_string(),
            "wss://third.example".to_string(),
        ]
    );
}

#[test]
fn list_urls_returns_nothing_when_the_event_does_not_match() {
    assert!(list_urls_with("not json", 7, "relay").is_empty());
    assert!(list_urls_with(
        r#"{"kind": 8, "tags": [["relay", "wss://relay.example"]]}"#,
        7,
        "relay"
    )
    .is_empty());
    assert!(list_urls_with(
        r#"{"kind": 7, "tags": [["r", "wss://relay.example"]]}"#,
        7,
        "relay"
    )
    .is_empty());
}

#[test]
fn list_urls_reads_a_saved_10002_event() {
    let event = include_str!("fixtures/10002.json");
    assert_eq!(
        list_urls(event),
        vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
            "wss://relay.primal.net".to_string(),
        ]
    );
    assert!(list_urls_with(event, 3, "r").is_empty());
}
