use super::*;

#[test]
fn list_urls_reads_public_relays_and_keeps_a_path() {
    let event = r#"{
        "kind": 10002,
        "tags": [
            ["r", "wss://Relay.Example/"],
            ["r", "wss://other.example", "read"],
            ["r", "wss://third.example/room/?x=1", "write"],
            ["relay", "wss://skipped.example"],
            ["r", "wss://relay.example/", "write"],
            ["r", "ws://bad.example"]
        ]
    }"#;

    assert_eq!(
        list_urls(event),
        vec![
            "wss://relay.example".to_string(),
            "wss://other.example".to_string(),
            "wss://third.example/room?x=1".to_string(),
        ]
    );
}

#[test]
fn list_urls_returns_nothing_when_the_event_does_not_match() {
    assert!(list_urls("not json").is_empty());
    assert!(list_urls(r#"{"kind": 3, "tags": [["r", "wss://relay.example"]]}"#).is_empty());
    assert!(list_urls(r#"{"kind": 10002, "tags": [["relay", "wss://relay.example"]]}"#).is_empty());
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
}
