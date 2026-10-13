use super::*;

#[test]
fn deal_returns_the_url_that_is_neither_pinned_nor_removed() {
    let candidates = vec![
        "wss://Pinned.Example/".to_string(),
        "wss://kept.example".to_string(),
        "wss://removed.example/".to_string(),
    ];
    let pinned = vec!["wss://pinned.example".to_string()];
    let removed = vec!["wss://Removed.Example".to_string()];

    assert_eq!(
        deal(&candidates, 3, &pinned, &removed),
        vec!["wss://kept.example".to_string()]
    );
}

#[test]
fn deal_returns_nothing_for_an_empty_list() {
    assert_eq!(deal(&[], 3, &[], &[]), Vec::<String>::new());
}

#[test]
fn deal_returns_nothing_when_no_seats_are_open() {
    let candidates = vec!["wss://kept.example".to_string()];

    assert_eq!(deal(&candidates, 0, &[], &[]), Vec::<String>::new());
}

fn five_candidates() -> Vec<String> {
    vec![
        "wss://a.example".to_string(),
        "wss://Pinned.Example/".to_string(),
        "wss://b.example".to_string(),
        "wss://c.example".to_string(),
        "wss://d.example".to_string(),
    ]
}

#[test]
fn deal_returns_the_first_two_that_pass() {
    let pinned = vec!["wss://pinned.example".to_string()];

    assert_eq!(
        deal(&five_candidates(), 2, &pinned, &[]),
        vec!["wss://a.example".to_string(), "wss://b.example".to_string()]
    );
}

#[test]
fn deal_returns_every_url_that_passes_when_more_seats_are_open() {
    let pinned = vec!["wss://pinned.example".to_string()];

    assert_eq!(
        deal(&five_candidates(), 10, &pinned, &[]),
        vec![
            "wss://a.example".to_string(),
            "wss://b.example".to_string(),
            "wss://c.example".to_string(),
            "wss://d.example".to_string(),
        ]
    );
}
