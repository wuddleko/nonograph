use super::*;
use serde_json::json;

const NOW: i64 = 1_700_000_100;

fn answered() -> Visit {
    Visit {
        at: NOW - 5,
        socket_answered: true,
        nip11: Some(json!({ "name": "example" })),
        probes: vec![ProbeResult {
            name: "30323".to_owned(),
            accepted: true,
            detail: "OK".to_owned(),
        }],
    }
}

fn missed() -> Visit {
    Visit {
        at: NOW - 5,
        socket_answered: false,
        nip11: None,
        probes: Vec::new(),
    }
}

fn empty() -> Queue {
    Queue::new(Pool { rows: Vec::new() })
}

#[test]
fn push_canonicalizes_a_new_url_and_inserts_its_row() {
    let mut queue = empty();

    assert!(queue.push("wss://A.Example/", NOW));
    assert!(!queue.push("wss://127.0.0.1", NOW));

    assert_eq!(queue.pool().rows.len(), 1);
    assert_eq!(queue.pool().rows[0].url, "wss://a.example");
    assert_eq!(queue.pool().rows[0].first_seen, NOW);
    assert_eq!(queue.pool().rows[0].next_try, NOW);
    assert_eq!(queue.pool().rows[0].failures, 0);
    assert!(queue.pool().rows[0].visit.is_none());
    assert_eq!(queue.pop().as_deref(), Some("wss://a.example"));
}

#[test]
fn push_places_a_due_row_and_leaves_a_cooling_row() {
    let mut queue = Queue::new(Pool {
        rows: vec![
            Row {
                url: "wss://a.example".to_owned(),
                first_seen: 1_700_000_000,
                next_try: NOW + 24 * 60 * 60,
                failures: 0,
                visit: None,
            },
            Row {
                url: "wss://b.example".to_owned(),
                first_seen: 1_700_000_000,
                next_try: NOW,
                failures: 4,
                visit: Some(missed()),
            },
            Row {
                url: "wss://c.example".to_owned(),
                first_seen: 1_700_000_000,
                next_try: NOW + 15 * 60,
                failures: 1,
                visit: Some(missed()),
            },
        ],
    });

    assert!(queue.push("wss://A.Example/", NOW));
    assert!(queue.push("wss://b.example", NOW));
    assert!(!queue.push("wss://c.example", NOW));

    assert_eq!(queue.pool().rows.len(), 3);
    assert_eq!(queue.pool().rows[0].first_seen, 1_700_000_000);
    assert_eq!(queue.pop().as_deref(), Some("wss://a.example"));
    assert_eq!(queue.pop().as_deref(), Some("wss://b.example"));
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.pool().rows[2].next_try, NOW + 15 * 60);
    assert_eq!(queue.pool().rows[2].failures, 1);
}

#[test]
fn the_same_url_is_not_queued_twice() {
    let mut queue = empty();

    assert!(queue.push("wss://a.example", NOW));
    assert!(!queue.push("wss://A.Example/", NOW));

    assert_eq!(queue.pool().rows.len(), 1);
    assert_eq!(queue.pop().as_deref(), Some("wss://a.example"));
    assert_eq!(queue.pop(), None);
}

#[test]
fn a_url_in_flight_is_not_queued_again() {
    let mut queue = empty();
    queue.push("wss://a.example", NOW);
    queue.push("wss://b.example", NOW);

    assert_eq!(queue.pop().as_deref(), Some("wss://a.example"));
    assert_eq!(queue.pop().as_deref(), Some("wss://b.example"));
    assert!(!queue.push("wss://A.Example/", NOW));
    assert!(!queue.push("wss://b.example", NOW));
    assert_eq!(queue.pop(), None);
}

#[test]
fn finish_writes_the_visit_and_an_answer_waits_one_day() {
    let mut queue = Queue::new(Pool {
        rows: vec![Row {
            url: "wss://a.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: NOW,
            failures: 4,
            visit: Some(missed()),
        }],
    });
    queue.push("wss://a.example", NOW);
    let url = queue.pop().unwrap();

    queue.finish(&url, answered(), Vec::<&str>::new(), NOW);

    let row = &queue.pool().rows[0];
    let visit = row.visit.as_ref().unwrap();
    assert!(visit.socket_answered);
    assert_eq!(visit.at, NOW - 5);
    assert_eq!(visit.nip11.as_ref().unwrap()["name"], "example");
    assert_eq!(visit.probes[0].name, "30323");
    assert!(visit.probes[0].accepted);
    assert_eq!(row.failures, 0);
    assert_eq!(row.next_try, NOW + 24 * 60 * 60);
    assert_eq!(row.first_seen, 1_700_000_000);
    assert!(!queue.push("wss://a.example", NOW));
}

#[test]
fn a_miss_keeps_the_row_and_waits_fifteen_minutes() {
    let mut queue = empty();
    queue.push("wss://a.example", NOW);
    queue.pop();

    queue.finish("wss://A.Example/", missed(), Vec::<&str>::new(), NOW);

    assert_eq!(queue.pool().rows.len(), 1);
    assert_eq!(queue.pool().rows[0].url, "wss://a.example");
    assert!(!queue.pool().rows[0].visit.as_ref().unwrap().socket_answered);
    assert_eq!(queue.pool().rows[0].failures, 1);
    assert_eq!(queue.pool().rows[0].next_try, NOW + 15 * 60);
    assert!(!queue.push("wss://a.example", NOW));
    assert_eq!(queue.pop(), None);
}

#[test]
fn finish_clears_in_flight() {
    let mut queue = empty();
    queue.push("wss://a.example", NOW);
    queue.pop();
    queue.finish("wss://a.example", missed(), Vec::<&str>::new(), NOW);

    queue.finish("wss://a.example", missed(), ["wss://b.example"], NOW);

    assert_eq!(queue.pool().rows.len(), 1);
    assert_eq!(queue.pool().rows[0].failures, 1);
    assert_eq!(queue.pool().rows[0].next_try, NOW + 15 * 60);
    assert_eq!(queue.pop(), None);
}

#[test]
fn a_url_found_mid_pass_is_the_next_pop() {
    let mut queue = empty();
    queue.push("wss://a.example", NOW);
    queue.push("wss://b.example", NOW);
    queue.pop();

    queue.finish(
        "wss://a.example",
        answered(),
        [
            "wss://127.0.0.1",
            "wss://c.example",
            "wss://C.Example/",
            "wss://d.example",
        ],
        NOW,
    );

    assert_eq!(queue.pop().as_deref(), Some("wss://c.example"));
    assert_eq!(queue.pop().as_deref(), Some("wss://d.example"));
    assert_eq!(queue.pop().as_deref(), Some("wss://b.example"));
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.pool().rows[0].url, "wss://a.example");
    assert_eq!(queue.pool().rows[0].next_try, NOW + 24 * 60 * 60);
}

#[test]
fn a_discovered_url_already_waiting_or_in_flight_is_left_alone() {
    let mut queue = Queue::new(Pool {
        rows: vec![Row {
            url: "wss://c.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: NOW + 60 * 60,
            failures: 2,
            visit: Some(missed()),
        }],
    });
    queue.push("wss://a.example", NOW);
    queue.push("wss://b.example", NOW);
    queue.push("wss://d.example", NOW);
    queue.pop();
    let held = queue.pop().unwrap();

    queue.finish(
        "wss://a.example",
        answered(),
        [
            "wss://a.example",
            "wss://b.example",
            "wss://c.example",
            "wss://d.example",
        ],
        NOW,
    );

    assert_eq!(held, "wss://b.example");
    assert_eq!(queue.pool().rows.len(), 4);
    assert_eq!(queue.pool().rows[0].first_seen, 1_700_000_000);
    assert_eq!(queue.pool().rows[0].failures, 2);
    assert_eq!(queue.pool().rows[0].next_try, NOW + 60 * 60);
    assert_eq!(queue.pop().as_deref(), Some("wss://d.example"));
    assert_eq!(queue.pop(), None);
    assert!(!queue.push("wss://b.example", NOW));
}

#[test]
fn idle_is_an_empty_ready_list_with_nothing_in_flight() {
    let mut queue = empty();
    assert!(queue.idle());

    queue.push("wss://a.example", NOW);
    queue.push("wss://b.example", NOW);
    assert!(!queue.idle());

    queue.pop();
    queue.pop();
    assert!(!queue.idle());

    queue.finish("wss://a.example", answered(), ["wss://c.example"], NOW);
    assert!(!queue.idle());

    queue.finish("wss://b.example", missed(), Vec::<&str>::new(), NOW);
    assert!(!queue.idle());

    queue.pop();
    assert!(!queue.idle());
    queue.finish("wss://c.example", answered(), Vec::<&str>::new(), NOW);
    assert!(queue.idle());
}

#[test]
fn a_cooling_row_left_unpushed_leaves_the_queue_idle() {
    let queue = Queue::new(Pool {
        rows: vec![Row {
            url: "wss://a.example".to_owned(),
            first_seen: 1_700_000_000,
            next_try: NOW + 15 * 60,
            failures: 1,
            visit: Some(missed()),
        }],
    });

    assert!(queue.idle());
}
