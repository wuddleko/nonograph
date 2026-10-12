use super::*;
use serde_json::json;

#[test]
fn row_holds_a_visit() {
    let row = Row {
        url: "wss://relay.example".to_owned(),
        first_seen: 1_700_000_000,
        next_try: 1_700_086_400,
        failures: 0,
        visit: Some(Visit {
            at: 1_700_000_100,
            socket_answered: true,
            nip11: Some(json!({ "name": "example" })),
            probes: vec![ProbeResult {
                name: "30323".to_owned(),
                accepted: true,
                detail: "OK".to_owned(),
            }],
        }),
    };

    let pool = Pool { rows: vec![row] };
    let visit = pool.rows[0].visit.as_ref().unwrap();

    assert_eq!(pool.rows[0].url, "wss://relay.example");
    assert_eq!(pool.rows[0].first_seen, 1_700_000_000);
    assert_eq!(pool.rows[0].next_try, 1_700_086_400);
    assert!(visit.socket_answered);
    assert_eq!(visit.nip11.as_ref().unwrap()["name"], "example");
    assert_eq!(visit.probes[0].name, "30323");
    assert!(visit.probes[0].accepted);
    assert_eq!(visit.probes[0].detail, "OK");
}

#[test]
fn save_and_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let pool = Pool {
        rows: vec![Row {
            url: "wss://relay.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: 1_700_086_400,
            failures: 0,
            visit: Some(Visit {
                at: 1_700_000_100,
                socket_answered: true,
                nip11: Some(json!({
                    "limitation": {
                        "auth_required": false,
                        "payment_required": false
                    }
                })),
                probes: vec![
                    ProbeResult {
                        name: "30323".to_owned(),
                        accepted: true,
                        detail: "OK".to_owned(),
                    },
                    ProbeResult {
                        name: "open".to_owned(),
                        accepted: false,
                        detail: "closed".to_owned(),
                    },
                ],
            }),
        }],
    };

    save(&path, &pool).unwrap();
    assert_eq!(load(&path), pool);
}

#[test]
fn load_missing_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    assert_eq!(load(&path), Pool { rows: Vec::new() });
}

#[test]
fn load_bad_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(&path, "not a pool").unwrap();
    assert_eq!(load(&path), Pool { rows: Vec::new() });
}

#[test]
fn load_without_failures_is_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(
        &path,
        r#"{"rows":[{"url":"wss://relay.example","first_seen":1,"next_try":2,"visit":null}]}"#,
    )
    .unwrap();

    let pool = load(&path);
    assert_eq!(pool.rows.len(), 1);
    assert_eq!(pool.rows[0].url, "wss://relay.example");
    assert_eq!(pool.rows[0].first_seen, 1);
    assert_eq!(pool.rows[0].next_try, 2);
    assert_eq!(pool.rows[0].failures, 0);
    assert!(pool.rows[0].visit.is_none());
}

#[test]
fn failures_survive_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let pool = Pool {
        rows: vec![Row {
            url: "wss://relay.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: 1_700_000_900,
            failures: 3,
            visit: None,
        }],
    };

    save(&path, &pool).unwrap();
    assert_eq!(load(&path), pool);
}

#[test]
fn load_null_failures_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(
        &path,
        r#"{"rows":[{"url":"wss://relay.example","first_seen":1,"next_try":2,"failures":null,"visit":null}]}"#,
    )
    .unwrap();
    assert_eq!(load(&path), Pool { rows: Vec::new() });
}
