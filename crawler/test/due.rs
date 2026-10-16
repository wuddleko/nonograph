use super::*;
use serde_json::json;

fn visited(url: &str, next_try: i64) -> Row {
    Row {
        url: url.to_owned(),
        first_seen: 1_700_000_000,
        next_try,
        failures: 0,
        publish_refused: false,
        seen_from: Vec::new(),
        visit: Some(Visit {
            at: 1_700_000_000,
            socket_answered: true,
            nip11: Some(json!({})),
            probes: Vec::new(),
        }),
    }
}

fn unvisited(url: &str, next_try: i64) -> Row {
    Row {
        url: url.to_owned(),
        first_seen: 1_700_000_000,
        next_try,
        failures: 0,
        publish_refused: false,
        seen_from: Vec::new(),
        visit: None,
    }
}

#[test]
fn a_row_with_no_visit_is_due() {
    let now = 1_700_000_100;
    let rows = vec![unvisited("wss://relay.example", now + 24 * 60 * 60)];

    assert_eq!(due(&rows, now), vec!["wss://relay.example".to_owned()]);
}

#[test]
fn a_row_still_cooling_off_is_not_due() {
    let now = 1_700_000_100;
    let rows = vec![visited("wss://relay.example", now + 15 * 60)];

    assert!(due(&rows, now).is_empty());
}

#[test]
fn a_next_try_at_or_before_the_timestamp_is_due() {
    let now = 1_700_000_100;
    let rows = vec![
        visited("wss://relay.example", now),
        visited("wss://relay.earlier", now - 1),
    ];

    assert_eq!(
        due(&rows, now),
        vec![
            "wss://relay.example".to_owned(),
            "wss://relay.earlier".to_owned()
        ]
    );
}

#[test]
fn due_returns_ready_urls_in_pool_order() {
    let now = 1_700_000_100;
    let rows = vec![
        visited("wss://relay.cooling", now + 1),
        unvisited("wss://relay.new", now + 24 * 60 * 60),
        visited("wss://relay.ready", now - 60),
    ];

    assert_eq!(
        due(&rows, now),
        vec!["wss://relay.new".to_owned(), "wss://relay.ready".to_owned()]
    );
}
