use std::sync::{Arc, Mutex};

use crawler::{load, save, schedule, Pool, ProbeResult, Row, Visit, VisitHook};
use serde_json::json;

use super::*;

const NOW: i64 = 1_700_000_100;

fn answered() -> Visit {
    Visit {
        at: NOW - 5,
        socket_answered: true,
        nip11: Some(json!({ "name": "kept" })),
        probes: vec![ProbeResult {
            name: "marker".to_owned(),
            accepted: true,
            detail: "ok".to_owned(),
        }],
    }
}

fn silent() -> Visit {
    Visit {
        at: NOW - 5,
        socket_answered: false,
        nip11: None,
        probes: Vec::new(),
    }
}

fn row(url: &str, next_try: i64, visit: Option<Visit>) -> Row {
    Row {
        url: url.to_owned(),
        first_seen: 1_700_000_000,
        next_try,
        failures: 0,
        publish_refused: false,
        visit,
    }
}

struct Script {
    seen: Arc<Mutex<Vec<String>>>,
}

struct Keep {
    seen: Arc<Mutex<Vec<String>>>,
    nip11: Value,
}

impl VisitHook for Keep {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        self.seen.lock().expect("seen").push(url.to_owned());
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: Some(self.nip11.clone()),
                probes: vec![ProbeResult {
                    name: "30323".to_owned(),
                    accepted: true,
                    detail: "OK".to_owned(),
                }],
            },
            vec!["wss://found.example".to_owned()],
        )
    }
}

impl VisitHook for Script {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        self.seen.lock().expect("seen").push(url.to_owned());
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: None,
                probes: Vec::new(),
            },
            Vec::new(),
        )
    }
}

#[test]
fn the_pool_starts_only_behind_a_proxy() {
    use std::sync::mpsc;

    assert!(!start_pool(RelaySocks::Direct, || panic!(
        "started without a proxy"
    )));
    assert!(!start_pool(RelaySocks::Disabled, || panic!(
        "started without a proxy"
    )));

    let proxy = RelaySocks::Proxy("127.0.0.1:9050".parse().unwrap());
    let (started, done) = mpsc::channel();
    assert!(start_pool(proxy, move || {
        let _ = started.send(());
    }));
    done.recv_timeout(Duration::from_secs(2))
        .expect("pool thread");
}

#[test]
fn the_pool_sleeps_until_the_earliest_next_try() {
    let mut answered = row("wss://open.example", NOW, Some(answered()));
    assert_eq!(schedule(&mut answered, NOW), NOW + 24 * 60 * 60);

    let mut missed = row("wss://miss.example", NOW, Some(silent()));
    assert_eq!(schedule(&mut missed, NOW), NOW + 15 * 60);
    assert_eq!(schedule(&mut missed, NOW), NOW + 60 * 60);
    assert_eq!(schedule(&mut missed, NOW), NOW + 6 * 60 * 60);

    let mut soon = row("wss://soon.example", NOW, Some(silent()));
    schedule(&mut soon, NOW);
    assert_eq!(
        super::pool_wait(&[answered, missed, soon], NOW),
        Duration::from_secs(15 * 60)
    );
    assert_eq!(super::pool_wait(&[], NOW), Duration::ZERO);
}

#[test]
fn a_save_error_lets_the_pool_continue() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let later = crate::nostr::now_secs() + 15 * 60;
    save(
        &path,
        &Pool {
            rows: vec![row("wss://relay.example", later, None)],
        },
    )
    .unwrap();
    let crawls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&crawls);
    let finished = Arc::clone(&crawls);
    let waits = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&waits);

    super::serve_pool(
        &path,
        move |_| {
            let attempt = count.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                Err(std::io::Error::new(std::io::ErrorKind::Other, "disk"))
            } else {
                Ok(())
            }
        },
        move |wait| {
            seen.lock().expect("waits").push(wait);
            crawls.load(Ordering::SeqCst) < 2
        },
    );

    assert_eq!(finished.load(Ordering::SeqCst), 2);
    let waits = waits.lock().expect("waits");
    assert_eq!(waits.len(), 2);
    assert!(waits[0].as_secs() <= 15 * 60);
    assert!(waits[0].as_secs() >= 15 * 60 - 5);
}

#[test]
fn a_due_pool_waits_a_minute_when_the_save_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    save(
        &path,
        &Pool {
            rows: vec![row("wss://relay.example", 0, None)],
        },
    )
    .unwrap();
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        |_| Err(std::io::Error::new(std::io::ErrorKind::Other, "disk")),
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    let waits = waits.lock().expect("waits");
    assert_eq!(waits.len(), 1);
    assert_eq!(waits[0], Duration::from_secs(60));
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("wss://relay.example"));
}

#[test]
fn a_bad_pool_file_waits_fifteen_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(&path, "not a pool").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        {
            let path = path.clone();
            let seen = Arc::clone(&seen);
            move |now| {
                run_crawl(&path, now, {
                    let seen = Arc::clone(&seen);
                    move || Script {
                        seen: Arc::clone(&seen),
                    }
                })
            }
        },
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    assert!(seen.lock().expect("seen").is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), b"not a pool");
    let waits = waits.lock().expect("waits");
    assert_eq!(waits.as_slice(), &[Duration::from_secs(15 * 60)]);
}

#[test]
fn a_due_pool_waits_fifteen_minutes_after_a_pass() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    save(
        &path,
        &Pool {
            rows: vec![row("wss://relay.example", 0, None)],
        },
    )
    .unwrap();
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        |_| Ok(()),
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    let waits = waits.lock().expect("waits");
    assert_eq!(waits.as_slice(), &[Duration::from_secs(15 * 60)]);
}

#[test]
fn a_resting_pool_sleeps_until_the_next_try() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let later = crate::nostr::now_secs() + 5 * 60;
    save(
        &path,
        &Pool {
            rows: vec![row("wss://relay.example", later, Some(answered()))],
        },
    )
    .unwrap();
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        |_| Ok(()),
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    let waits = waits.lock().expect("waits");
    assert_eq!(waits.len(), 1);
    assert!(waits[0].as_secs() <= 5 * 60);
    assert!(waits[0].as_secs() >= 5 * 60 - 5);
}

#[test]
fn a_capped_pass_leaves_due_rows_for_fifteen_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let rows = (0..49)
        .map(|index| row(&format!("wss://r{index}.example"), NOW, None))
        .collect();
    save(&path, &Pool { rows }).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        {
            let path = path.clone();
            let seen = Arc::clone(&seen);
            move |now| {
                run_crawl(&path, now, {
                    let seen = Arc::clone(&seen);
                    move || Script {
                        seen: Arc::clone(&seen),
                    }
                })
            }
        },
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    assert_eq!(seen.lock().expect("seen").len(), crawler::VISIT_CAP);
    let pool = load(&path);
    assert_eq!(
        pool.rows.iter().filter(|row| row.visit.is_some()).count(),
        crawler::VISIT_CAP
    );
    let waiting: Vec<Row> = pool
        .rows
        .iter()
        .filter(|row| row.visit.is_none())
        .cloned()
        .collect();
    assert!(!waiting.is_empty());
    assert_eq!(due(&waiting, crate::nostr::now_secs()).len(), waiting.len());
    let waits = waits.lock().expect("waits");
    assert_eq!(waits.as_slice(), &[Duration::from_secs(15 * 60)]);
}

#[test]
fn a_full_pool_checks_the_due_relays_then_sleeps_until_the_next_try() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut rows: Vec<Row> = (0..CANDIDATE_CAP)
        .map(|index| {
            visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            )
        })
        .collect();
    rows.push(row("wss://new.example", NOW, None));
    save(&path, &Pool { rows }).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&waits);

    super::serve_pool(
        &path,
        {
            let path = path.clone();
            let seen = Arc::clone(&seen);
            let open = open.clone();
            move |now| {
                run_crawl(&path, now, {
                    let seen = Arc::clone(&seen);
                    let open = open.clone();
                    move || Keep {
                        seen: Arc::clone(&seen),
                        nip11: open.clone(),
                    }
                })
            }
        },
        move |wait| {
            recorded.lock().expect("waits").push(wait);
            false
        },
    );

    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    let mut expected: Vec<String> = (0..CANDIDATE_CAP)
        .map(|index| format!("wss://relay{index}.example"))
        .collect();
    expected.sort();
    assert_eq!(seen, expected);
    let pool = load(&path);
    assert!(pool
        .rows
        .iter()
        .any(|row| row.url == "wss://found.example" && row.visit.is_none()));
    assert!(pool
        .rows
        .iter()
        .any(|row| row.url == "wss://new.example" && row.visit.is_none()));
    let day = 24 * 60 * 60;
    let waits = waits.lock().expect("waits");
    assert_eq!(waits.len(), 1);
    assert!(waits[0].as_secs() <= day);
    assert!(waits[0].as_secs() >= day - 5);
}

#[test]
fn the_pool_file_lives_beside_the_pages() {
    assert_eq!(
        pool_path(Path::new("content")),
        PathBuf::from("content/relay-pool.json")
    );
    assert!((4..=8).contains(&WORKERS));
}

#[test]
fn doors_are_pushed_before_rows_that_are_already_due() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let cooling = row("wss://later.example", NOW + 15 * 60, Some(answered()));
    save(
        &path,
        &Pool {
            rows: vec![
                row("wss://nos.lol", NOW + 60, None),
                row("wss://relay.example", NOW + 60, None),
                cooling.clone(),
            ],
        },
    )
    .unwrap();

    let mut queue = batch_queue(&path, NOW).unwrap();

    assert_eq!(queue.pop().as_deref(), Some("wss://relay.primal.net"));
    assert_eq!(queue.pop().as_deref(), Some("wss://nos.lol"));
    assert_eq!(queue.pop().as_deref(), Some("wss://relay.damus.io"));
    assert_eq!(queue.pop().as_deref(), Some("wss://relay.example"));
    assert_eq!(queue.pop(), None);
    let kept = queue
        .pool()
        .rows
        .iter()
        .find(|row| row.url == "wss://later.example")
        .unwrap();
    assert_eq!(kept, &cooling);
}

#[test]
fn a_full_pool_visits_the_due_relay_and_stores_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut rows: Vec<Row> = (0..CANDIDATE_CAP)
        .map(|index| {
            let mut row = visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            );
            row.next_try = if index == 0 { NOW } else { NOW + 24 * 60 * 60 };
            row
        })
        .collect();
    let mut outside = visited("wss://relay48.example", true, open, true);
    outside.next_try = NOW;
    rows.push(outside);
    rows.push(row("wss://new.example", NOW, None));
    save(&path, &Pool { rows }).unwrap();

    let mut queue = batch_queue(&path, NOW).unwrap();

    assert!(!queue.needs_relays());
    assert_eq!(queue.pop().as_deref(), Some("wss://relay0.example"));
    assert_eq!(queue.pop(), None);
    assert!(queue
        .pool()
        .rows
        .iter()
        .any(|row| row.url == "wss://new.example" && row.visit.is_none()));
    assert!(!queue
        .pool()
        .rows
        .iter()
        .any(|row| row.url == "wss://relay.primal.net"));
}

#[test]
fn a_full_rotation_stores_a_discovered_url_without_queueing_it() {
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let rows: Vec<Row> = (0..CANDIDATE_CAP)
        .map(|index| {
            let mut row = visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            );
            row.next_try = if index == 0 { NOW } else { NOW + 24 * 60 * 60 };
            row
        })
        .collect();
    let mut queue = Queue::new(Pool { rows });
    queue.set_rotation(CANDIDATE_CAP, row_is_usable);
    assert!(queue.push("wss://relay0.example", NOW));
    let url = queue.pop().unwrap();

    queue.finish(
        &url,
        Visit {
            at: NOW,
            socket_answered: true,
            nip11: Some(open),
            probes: vec![ProbeResult {
                name: "30323".to_owned(),
                accepted: true,
                detail: "OK".to_owned(),
            }],
        },
        ["wss://found.example"],
        NOW,
    );

    assert!(queue
        .pool()
        .rows
        .iter()
        .any(|row| row.url == "wss://found.example" && row.visit.is_none()));
    assert_eq!(queue.pop(), None);
}

#[test]
fn a_short_pool_queues_a_cooling_door() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut rows: Vec<Row> = (0..CANDIDATE_CAP - 1)
        .map(|index| {
            let mut row = visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            );
            row.next_try = NOW + 24 * 60 * 60;
            row
        })
        .collect();
    rows.push(row("wss://nos.lol", NOW + 15 * 60, Some(silent())));
    save(&path, &Pool { rows }).unwrap();

    let mut queue = batch_queue(&path, NOW).unwrap();

    assert!(queue.needs_relays());
    assert_eq!(queue.pop().as_deref(), Some("wss://relay.primal.net"));
    assert_eq!(queue.pop().as_deref(), Some("wss://nos.lol"));
    assert_eq!(queue.pop().as_deref(), Some("wss://relay.damus.io"));
    assert_eq!(queue.pop(), None);
}

#[test]
fn run_crawl_visits_the_doors_and_the_due_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let cooling = row("wss://later.example", NOW + 15 * 60, Some(answered()));
    save(
        &path,
        &Pool {
            rows: vec![row("wss://relay.example", NOW + 60, None), cooling.clone()],
        },
    )
    .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_crawl(&path, NOW, {
        let seen = Arc::clone(&seen);
        move || Script {
            seen: Arc::clone(&seen),
        }
    })
    .unwrap();

    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            "wss://nos.lol".to_owned(),
            "wss://relay.damus.io".to_owned(),
            "wss://relay.example".to_owned(),
            "wss://relay.primal.net".to_owned(),
        ]
    );
    let pool = load(&path);
    assert_eq!(pool.rows.len(), 5);
    let kept = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://later.example")
        .unwrap();
    assert_eq!(kept, &cooling);
    assert!(pool
        .rows
        .iter()
        .filter(|row| row.url != "wss://later.example")
        .all(|row| row.visit.as_ref().is_some_and(|visit| visit.at == NOW)));
}

#[test]
fn a_missing_pool_file_crawls_the_doors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_crawl(&path, NOW, {
        let seen = Arc::clone(&seen);
        move || Script {
            seen: Arc::clone(&seen),
        }
    })
    .unwrap();

    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            "wss://nos.lol".to_owned(),
            "wss://relay.damus.io".to_owned(),
            "wss://relay.primal.net".to_owned(),
        ]
    );
}

struct Saved {
    body: String,
    urls: Vec<String>,
    calls: Vec<&'static str>,
    deadlines: Vec<Instant>,
}

impl RelayRead for Saved {
    fn read_list(&mut self, _url: &str, deadline: Instant) -> crate::nostr::RelayList {
        self.calls.push("list");
        self.deadlines.push(deadline);
        crate::nostr::RelayList {
            socket_answered: true,
            urls: self.urls.clone(),
        }
    }

    fn read_nip11(&mut self, _url: &str, deadline: Instant) -> Option<String> {
        self.calls.push("nip11");
        self.deadlines.push(deadline);
        Some(self.body.clone())
    }
}

#[test]
fn a_non_door_stores_the_document_and_skips_the_list() {
    let body = include_str!("../test/nip11/auth.json").to_owned();
    let document: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut hook = CrawlVisit {
        timeout_secs: 30,
        filling: true,
        read: Saved {
            body,
            urls: vec![
                "wss://relay.damus.io".to_owned(),
                "wss://nos.lol".to_owned(),
            ],
            calls: Vec::new(),
            deadlines: Vec::new(),
        },
    };
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    assert!(queue.push("wss://auth.example", NOW));

    assert_eq!(
        queue.visit_one(&mut hook, NOW).as_deref(),
        Some("wss://auth.example")
    );

    assert_eq!(hook.read.calls, ["nip11"]);
    let row = queue
        .pool()
        .rows
        .iter()
        .find(|row| row.url == "wss://auth.example")
        .unwrap();
    let visit = row.visit.as_ref().unwrap();
    assert_eq!(visit.at, NOW);
    assert!(visit.socket_answered);
    assert_eq!(visit.nip11.as_ref(), Some(&document));
    assert!(visit.probes.is_empty());
    assert_eq!(queue.pool().rows.len(), 1);
}

#[test]
fn a_door_reads_the_list_while_the_pool_is_filling() {
    let body = include_str!("../test/nip11/auth.json").to_owned();
    let mut hook = CrawlVisit {
        timeout_secs: 30,
        filling: true,
        read: Saved {
            body,
            urls: vec!["wss://other.example".to_owned()],
            calls: Vec::new(),
            deadlines: Vec::new(),
        },
    };
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    assert!(queue.push("wss://nos.lol", NOW));

    assert_eq!(
        queue.visit_one(&mut hook, NOW).as_deref(),
        Some("wss://nos.lol")
    );

    assert_eq!(hook.read.calls, ["list", "nip11"]);
    assert_eq!(hook.read.deadlines[0], hook.read.deadlines[1]);
    assert!(queue
        .pool()
        .rows
        .iter()
        .any(|row| row.url == "wss://other.example"));
}

#[test]
fn a_health_check_does_not_read_the_list() {
    let body = include_str!("../test/nip11/auth.json").to_owned();
    let mut hook = CrawlVisit {
        timeout_secs: 30,
        filling: false,
        read: Saved {
            body,
            urls: vec!["wss://other.example".to_owned()],
            calls: Vec::new(),
            deadlines: Vec::new(),
        },
    };
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    assert!(queue.push("wss://nos.lol", NOW));

    assert_eq!(
        queue.visit_one(&mut hook, NOW).as_deref(),
        Some("wss://nos.lol")
    );

    assert_eq!(hook.read.calls, ["nip11"]);
    assert!(!queue
        .pool()
        .rows
        .iter()
        .any(|row| row.url == "wss://other.example"));
}

#[test]
fn a_missed_socket_still_returns_the_visit() {
    let mut hook = CrawlVisit::new(30, crate::nostr::RelaySocks::Disabled, true);
    let started = Instant::now();
    let (visit, urls) = hook.visit("wss://warm.example", NOW);
    assert!(started.elapsed() < Duration::from_millis(50));
    assert_eq!(visit.at, NOW);
    assert!(!visit.socket_answered);
    assert!(visit.nip11.is_none());
    assert!(visit.probes.is_empty());
    assert!(urls.is_empty());
}

fn saved_document(raw: &str) -> Value {
    serde_json::from_str(raw).expect("nip11 fixture")
}

fn visited(url: &str, socket_answered: bool, nip11: Value, accepted: bool) -> Row {
    row(
        url,
        NOW,
        Some(Visit {
            at: NOW,
            socket_answered,
            nip11: Some(nip11),
            probes: vec![ProbeResult {
                name: "30323".to_owned(),
                accepted,
                detail: if accepted {
                    "OK".to_owned()
                } else {
                    "blocked: kind 30323 is not accepted".to_owned()
                },
            }],
        }),
    )
}

#[test]
fn usable_rows_keep_an_open_relay_that_accepted_the_probe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    save(
        &path,
        &Pool {
            rows: vec![
                visited(
                    "wss://auth.example",
                    true,
                    saved_document(include_str!("../test/nip11/auth.json")),
                    true,
                ),
                visited(
                    "wss://payment.example",
                    true,
                    saved_document(include_str!("../test/nip11/payment.json")),
                    true,
                ),
                visited(
                    "wss://restricted.example",
                    true,
                    saved_document(include_str!("../test/nip11/restricted.json")),
                    true,
                ),
                visited(
                    "wss://kinds.example",
                    true,
                    saved_document(include_str!("../test/nip11/kinds-without.json")),
                    true,
                ),
                visited("wss://refused.example", true, open.clone(), false),
                visited("wss://silent.example", false, open.clone(), true),
                row("wss://unvisited.example", NOW, None),
                visited("wss://open.example", true, open, true),
            ],
        },
    )
    .unwrap();

    let usable = usable_rows(&path);
    assert_eq!(usable.len(), 1);
    assert_eq!(usable[0].url, "wss://open.example");
}

#[test]
fn mark_publish_sets_a_refused_row_and_clears_an_accepted_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut marked = visited("wss://marked.example", true, open.clone(), true);
    marked.publish_refused = true;
    save(
        &path,
        &Pool {
            rows: vec![
                visited("wss://open.example", true, open.clone(), true),
                marked,
                visited("wss://kept.example", true, open, true),
            ],
        },
    )
    .unwrap();

    mark_publish(
        &path,
        &[
            "wss://Open.Example/".to_string(),
            "wss://missing.example".to_string(),
        ],
        &["wss://marked.example".to_string()],
    )
    .unwrap();

    let pool = load(&path);
    assert_eq!(pool.rows.len(), 3);
    let open_row = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://open.example")
        .unwrap();
    assert!(open_row.publish_refused);
    assert!(open_row.visit.is_some());
    let marked_row = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://marked.example")
        .unwrap();
    assert!(!marked_row.publish_refused);
    assert!(
        !pool
            .rows
            .iter()
            .find(|row| row.url == "wss://kept.example")
            .unwrap()
            .publish_refused
    );
    assert!(!pool
        .rows
        .iter()
        .any(|row| row.url == "wss://missing.example"));
}

#[test]
fn a_refused_path_or_query_marks_only_that_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    save(
        &path,
        &Pool {
            rows: vec![
                visited("wss://nos.lol", true, open.clone(), true),
                visited("wss://nos.lol/path", true, open, true),
            ],
        },
    )
    .unwrap();

    mark_publish(
        &path,
        &[
            "wss://Nos.lol/path/".to_string(),
            "wss://nos.lol?x=1".to_string(),
        ],
        &[],
    )
    .unwrap();

    let pool = load(&path);
    assert_eq!(pool.rows.len(), 2);
    let host = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://nos.lol")
        .unwrap();
    assert!(!host.publish_refused);
    assert!(host.visit.is_some());
    let listed = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://nos.lol/path")
        .unwrap();
    assert!(listed.publish_refused);
    assert!(listed.visit.is_some());
    assert!(!pool.rows.iter().any(|row| row.url.contains('?')));

    mark_publish(&path, &["wss://nos.lol".to_string()], &[]).unwrap();

    let pool = load(&path);
    assert_eq!(pool.rows.len(), 2);
    assert!(
        pool.rows
            .iter()
            .find(|row| row.url == "wss://nos.lol")
            .unwrap()
            .publish_refused
    );
    assert!(
        pool.rows
            .iter()
            .find(|row| row.url == "wss://nos.lol/path")
            .unwrap()
            .publish_refused
    );
}

#[test]
fn mark_publish_leaves_a_bad_pool_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(&path, "not a pool").unwrap();

    let err = mark_publish(&path, &["wss://open.example".to_string()], &[]).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read(&path).unwrap(), b"not a pool");
}

#[test]
fn candidates_return_the_open_relay_and_leave_out_an_unvisited_door() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    save(
        &path,
        &Pool {
            rows: vec![
                visited("wss://closed.example", false, open.clone(), true),
                row("wss://relay.primal.net", NOW, None),
                visited("wss://open.example", true, open, true),
            ],
        },
    )
    .unwrap();

    assert_eq!(
        candidates(&path, &[]),
        vec!["wss://open.example".to_string()]
    );
}

#[test]
fn candidates_stop_at_forty_eight() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let rows = (0..49)
        .map(|index| {
            visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            )
        })
        .collect();
    save(&path, &Pool { rows }).unwrap();

    let urls = candidates(&path, &[]);
    assert_eq!(urls.len(), 48);
    assert_eq!(urls[0], "wss://relay0.example");
    assert_eq!(urls[47], "wss://relay47.example");
    assert!(!urls.iter().any(|url| url == "wss://relay48.example"));
}

#[test]
fn a_marked_row_is_left_out_of_the_next_deal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut marked = visited("wss://marked.example", true, open.clone(), true);
    marked.publish_refused = true;
    save(
        &path,
        &Pool {
            rows: vec![marked, visited("wss://open.example", true, open, true)],
        },
    )
    .unwrap();

    let usable = usable_rows(&path);
    assert_eq!(usable.len(), 1);
    assert_eq!(usable[0].url, "wss://open.example");
    let urls = candidates(&path, &[]);
    assert_eq!(urls, vec!["wss://open.example".to_string()]);
    assert_eq!(crawler::deal(&urls, 3, &[], &[]), urls);
    assert_eq!(next_pool_relays(&path, &[]), urls);
}

#[test]
fn a_marked_host_deals_the_open_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let mut marked = visited("wss://nos.lol", true, open.clone(), true);
    marked.publish_refused = true;
    save(
        &path,
        &Pool {
            rows: vec![marked, visited("wss://nos.lol/path", true, open, true)],
        },
    )
    .unwrap();

    let urls = candidates(&path, &[]);
    assert_eq!(urls, vec!["wss://nos.lol/path".to_string()]);
    assert_eq!(crawler::deal(&urls, 3, &[], &[]), urls);
}

#[test]
fn a_bad_pool_file_shows_the_configured_relays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    std::fs::write(&path, "not a pool").unwrap();
    let configured = [
        "wss://Config.Example/".to_string(),
        "wss://10.0.0.1".to_string(),
        "wss://config.example".to_string(),
    ];

    assert_eq!(
        candidates(&path, &configured),
        vec!["wss://config.example".to_string()]
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"not a pool");
    assert!(candidates(&dir.path().join("missing.json"), &configured).is_empty());
}

#[test]
fn next_pool_relays_return_three_and_skip_the_link_relay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = saved_document(include_str!("../test/nip11/open.json"));
    let rows = std::iter::once(visited("wss://closed.example", false, open.clone(), true))
        .chain((0..5).map(|index| {
            visited(
                &format!("wss://relay{index}.example"),
                true,
                open.clone(),
                true,
            )
        }))
        .collect();
    save(&path, &Pool { rows }).unwrap();

    let relays = next_pool_relays(&path, &["wss://Relay0.Example/".to_string()]);
    assert_eq!(
        relays,
        vec![
            "wss://relay1.example".to_string(),
            "wss://relay2.example".to_string(),
            "wss://relay3.example".to_string(),
        ]
    );
}
