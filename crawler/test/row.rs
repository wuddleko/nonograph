use super::*;
use serde_json::json;

#[test]
fn row_holds_a_visit() {
    let row = Row {
        url: "wss://relay.example".to_owned(),
        first_seen: 1_700_000_000,
        next_try: 1_700_086_400,
        failures: 0,
        publish_refused: false,
        seen_from: Vec::new(),
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
            publish_refused: false,
            seen_from: Vec::new(),
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
fn a_bad_pool_file_is_left_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(&path, "not a pool").unwrap();
    let mut crawl = Pool {
        rows: vec![Row {
            url: "wss://relay.example".to_owned(),
            first_seen: 1,
            next_try: 2,
            failures: 0,
            publish_refused: false,
            seen_from: Vec::new(),
            visit: None,
        }],
    };

    let err = save_crawl(&path, &mut crawl).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read(&path).unwrap(), b"not a pool");
    assert_eq!(crawl.rows.len(), 1);

    let err = edit_pool(&path, |pool| {
        pool.rows.clear();
        true
    })
    .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read(&path).unwrap(), b"not a pool");
}

#[test]
fn save_crawl_keeps_a_path_flag_off_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let old = Visit {
        at: 1,
        socket_answered: false,
        nip11: None,
        probes: Vec::new(),
    };
    let fresh = Visit {
        at: 2,
        socket_answered: true,
        nip11: None,
        probes: Vec::new(),
    };
    let disk = |url: &str, refused: bool| Row {
        url: url.to_owned(),
        first_seen: 1,
        next_try: 1,
        failures: 4,
        publish_refused: refused,
        seen_from: Vec::new(),
        visit: Some(old.clone()),
    };
    let crawl_row = |url: &str| Row {
        url: url.to_owned(),
        first_seen: 1,
        next_try: 9,
        failures: 0,
        publish_refused: false,
        seen_from: Vec::new(),
        visit: Some(fresh.clone()),
    };
    save(
        &path,
        &Pool {
            rows: vec![
                disk("wss://nos.lol/path", true),
                disk("wss://Open.Example/", true),
                disk("wss://nos.lol", false),
            ],
        },
    )
    .unwrap();
    let mut crawl = Pool {
        rows: vec![
            crawl_row("wss://open.example"),
            crawl_row("wss://nos.lol"),
            crawl_row("wss://nos.lol/path"),
        ],
    };

    save_crawl(&path, &mut crawl).unwrap();

    assert_eq!(load(&path), crawl);
    let pool = load(&path);
    let open = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://open.example")
        .unwrap();
    assert!(open.publish_refused);
    assert_eq!(open.next_try, 9);
    assert_eq!(open.visit.as_ref().unwrap().at, 2);
    let host = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://nos.lol")
        .unwrap();
    assert!(!host.publish_refused);
    assert_eq!(host.visit.as_ref().unwrap().at, 2);
    let listed = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://nos.lol/path")
        .unwrap();
    assert!(listed.publish_refused);
    assert_eq!(listed.visit.as_ref().unwrap().at, 2);
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
    assert!(!pool.rows[0].publish_refused);
    assert!(pool.rows[0].visit.is_none());
}

#[test]
fn load_without_publish_refused_is_false() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(
        &path,
        r#"{"rows":[{"url":"wss://relay.example","first_seen":1,"next_try":2,"failures":0,"visit":null}]}"#,
    )
    .unwrap();

    let pool = load(&path);
    assert_eq!(pool.rows.len(), 1);
    assert!(!pool.rows[0].publish_refused);
}

#[test]
fn publish_refused_survives_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let pool = Pool {
        rows: vec![Row {
            url: "wss://relay.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: 1_700_000_900,
            failures: 0,
            publish_refused: true,
            seen_from: Vec::new(),
            visit: None,
        }],
    };

    save(&path, &pool).unwrap();
    assert_eq!(load(&path), pool);
    assert!(load(&path).rows[0].publish_refused);
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
            publish_refused: false,
            seen_from: Vec::new(),
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
