use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::*;

const NOW: i64 = 1_700_000_100;

struct Script {
    seen: Arc<Mutex<Vec<String>>>,
    overlap: Option<Arc<Overlap>>,
    discover: bool,
    boom: bool,
}

struct Overlap {
    arrived: Mutex<usize>,
    wake: Condvar,
    need: usize,
}

impl VisitHook for Script {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        if self.boom {
            panic!("visit failed");
        }
        self.seen.lock().expect("seen").push(url.to_owned());
        if let Some(overlap) = &self.overlap {
            let mut arrived = overlap.arrived.lock().expect("overlap");
            *arrived += 1;
            while *arrived < overlap.need {
                let (guard, status) = overlap
                    .wake
                    .wait_timeout(arrived, Duration::from_secs(2))
                    .expect("overlap");
                arrived = guard;
                if status.timed_out() {
                    panic!("visits did not overlap");
                }
            }
            overlap.wake.notify_all();
        }
        let discovered = if self.discover && url == "wss://a.example" {
            vec!["wss://b.example".to_owned()]
        } else {
            Vec::new()
        };
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: None,
                probes: Vec::new(),
            },
            discovered,
        )
    }
}

fn queue_with(urls: &[&str]) -> Queue {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    for url in urls {
        queue.push(url, NOW);
    }
    queue
}

#[test]
fn workers_visit_at_the_same_time_and_the_file_matches_the_pool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&[
        "wss://a.example",
        "wss://b.example",
        "wss://c.example",
        "wss://d.example",
    ]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let overlap = Arc::new(Overlap {
        arrived: Mutex::new(0),
        wake: Condvar::new(),
        need: 4,
    });
    let built = Arc::new(Mutex::new(0usize));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let seen = Arc::clone(&seen);
            let overlap = Arc::clone(&overlap);
            let built = Arc::clone(&built);
            move || {
                *built.lock().expect("built") += 1;
                Script {
                    seen: Arc::clone(&seen),
                    overlap: Some(Arc::clone(&overlap)),
                    discover: false,
                    boom: false,
                }
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(*built.lock().expect("built"), 4);
    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            "wss://a.example".to_owned(),
            "wss://b.example".to_owned(),
            "wss://c.example".to_owned(),
            "wss://d.example".to_owned(),
        ]
    );
    assert!(queue.idle());
    for row in &queue.pool().rows {
        assert!(row.visit.as_ref().unwrap().socket_answered);
        assert_eq!(row.visit.as_ref().unwrap().at, NOW);
        assert_eq!(row.failures, 0);
        assert_eq!(row.next_try, NOW + 24 * 60 * 60);
    }
    assert_eq!(load(&path), *queue.pool());
}

#[test]
fn a_url_found_during_a_visit_is_visited_in_the_same_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://a.example"]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Script {
                seen: Arc::clone(&seen),
                overlap: None,
                discover: true,
                boom: false,
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(
        seen.lock().expect("seen").clone(),
        vec!["wss://a.example".to_owned(), "wss://b.example".to_owned()]
    );
    assert_eq!(queue.pool().rows.len(), 2);
    assert!(queue.idle());
    assert_eq!(load(&path), *queue.pool());
}

#[test]
fn extra_workers_leave_when_the_queue_is_idle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://a.example", "wss://b.example"]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let built = Arc::new(Mutex::new(0usize));

    run_batch(
        &mut queue,
        8,
        NOW,
        {
            let seen = Arc::clone(&seen);
            let built = Arc::clone(&built);
            move || {
                *built.lock().expect("built") += 1;
                Script {
                    seen: Arc::clone(&seen),
                    overlap: None,
                    discover: false,
                    boom: false,
                }
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(*built.lock().expect("built"), 8);
    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    assert_eq!(
        seen,
        vec!["wss://a.example".to_owned(), "wss://b.example".to_owned()]
    );
    assert!(queue.idle());
}

#[test]
fn an_empty_queue_builds_hooks_and_does_not_visit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let built = Arc::new(Mutex::new(0usize));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let seen = Arc::clone(&seen);
            let built = Arc::clone(&built);
            move || {
                *built.lock().expect("built") += 1;
                Script {
                    seen: Arc::clone(&seen),
                    overlap: None,
                    discover: false,
                    boom: false,
                }
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(*built.lock().expect("built"), 4);
    assert!(seen.lock().expect("seen").is_empty());
    assert!(queue.idle());
    assert_eq!(load(&path), Pool { rows: Vec::new() });
}

#[test]
fn the_hook_receives_the_canonical_url() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://A.Example/"]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_batch(
        &mut queue,
        2,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Script {
                seen: Arc::clone(&seen),
                overlap: None,
                discover: false,
                boom: false,
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(
        seen.lock().expect("seen").clone(),
        vec!["wss://a.example".to_owned()]
    );
}

#[test]
fn a_panicking_visit_releases_the_url_and_the_batch_ends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://a.example"]);

    let result = catch_unwind(AssertUnwindSafe(|| {
        run_batch(
            &mut queue,
            2,
            NOW,
            || Script {
                seen: Arc::new(Mutex::new(Vec::new())),
                overlap: None,
                discover: false,
                boom: true,
            },
            &path,
        )
        .ok();
    }));

    assert!(result.is_err());
    assert_eq!(queue.pool().rows.len(), 1);
    assert!(!queue.pool().rows[0].visit.as_ref().unwrap().socket_answered);
    assert_eq!(queue.pool().rows[0].failures, 1);
    assert_eq!(queue.pool().rows[0].next_try, NOW + 15 * 60);
    assert!(queue.idle());
}

struct FileWatch {
    path: std::path::PathBuf,
    before: Vec<u8>,
    checks: Arc<Mutex<usize>>,
}

impl VisitHook for FileWatch {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        assert_eq!(std::fs::read(&self.path).unwrap(), self.before);
        *self.checks.lock().expect("checks") += 1;
        let discovered = if url == "wss://a.example" {
            vec!["wss://b.example".to_owned()]
        } else {
            Vec::new()
        };
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: None,
                probes: Vec::new(),
            },
            discovered,
        )
    }
}

#[test]
fn a_visit_sees_the_file_from_before_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://a.example"]);
    save(&path, queue.pool()).unwrap();
    let before = std::fs::read(&path).unwrap();
    let checks = Arc::new(Mutex::new(0usize));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let path = path.clone();
            let before = before.clone();
            let checks = Arc::clone(&checks);
            move || FileWatch {
                path: path.clone(),
                before: before.clone(),
                checks: Arc::clone(&checks),
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(*checks.lock().expect("checks"), 2);
    assert_eq!(load(&path), *queue.pool());
    assert_eq!(queue.pool().rows.len(), 2);
    assert_eq!(queue.pool().rows[1].url, "wss://b.example");
    assert!(queue.pool().rows.iter().all(|row| row.visit.is_some()));
    assert_ne!(std::fs::read(&path).unwrap(), before);
}

struct Repeat {
    seen: Arc<Mutex<Vec<String>>>,
}

impl VisitHook for Repeat {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        self.seen.lock().expect("seen").push(url.to_owned());
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: None,
                probes: Vec::new(),
            },
            vec![
                url.to_owned(),
                "wss://A.Example/".to_owned(),
                "wss://b.example".to_owned(),
                "wss://B.Example/".to_owned(),
            ],
        )
    }
}

#[test]
fn the_same_url_is_visited_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://a.example", "wss://A.Example/"]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Repeat {
                seen: Arc::clone(&seen),
            }
        },
        &path,
    )
    .unwrap();

    let mut seen = seen.lock().expect("seen").clone();
    seen.sort();
    assert_eq!(
        seen,
        vec!["wss://a.example".to_owned(), "wss://b.example".to_owned()]
    );
    assert_eq!(queue.pool().rows.len(), 2);
    assert!(queue.idle());
    assert_eq!(load(&path), *queue.pool());
}

#[test]
fn a_pass_stops_after_forty_eight_visits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let urls: Vec<String> = (0..=VISIT_CAP)
        .map(|index| format!("wss://r{index}.example"))
        .collect();
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    for url in &urls {
        assert!(queue.push(url, NOW));
    }
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_batch(
        &mut queue,
        4,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Script {
                seen: Arc::clone(&seen),
                overlap: None,
                discover: false,
                boom: false,
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(seen.lock().expect("seen").len(), VISIT_CAP);
    assert!(!queue.idle());
    assert_eq!(
        queue
            .pool()
            .rows
            .iter()
            .filter(|row| row.visit.is_none())
            .count(),
        1
    );
    assert_eq!(load(&path), *queue.pool());
}

struct Chain {
    seen: Arc<Mutex<Vec<String>>>,
}

impl VisitHook for Chain {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        let mut seen = self.seen.lock().expect("seen");
        seen.push(url.to_owned());
        let found = format!("wss://n{}.example", seen.len());
        drop(seen);
        (
            Visit {
                at: now,
                socket_answered: true,
                nip11: None,
                probes: Vec::new(),
            },
            vec![found],
        )
    }
}

#[test]
fn a_discovery_past_the_cap_is_saved_and_not_visited() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut queue = queue_with(&["wss://start.example"]);
    let seen = Arc::new(Mutex::new(Vec::new()));

    run_batch(
        &mut queue,
        1,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Chain {
                seen: Arc::clone(&seen),
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(seen.lock().expect("seen").len(), VISIT_CAP);
    let pending: Vec<_> = queue
        .pool()
        .rows
        .iter()
        .filter(|row| row.visit.is_none())
        .map(|row| row.url.as_str())
        .collect();
    assert_eq!(pending, vec!["wss://n48.example"]);
    assert_eq!(load(&path), *queue.pool());
}

#[test]
fn a_crawl_save_keeps_a_refusal_and_a_cleared_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let mut open = stored("wss://open.example", false);
    let mut marked = stored("wss://marked.example", true);
    let found = stored("wss://found.example", false);
    let mut queue = Queue::new(Pool {
        rows: vec![open.clone(), marked.clone(), found],
    });
    assert!(queue.push("wss://open.example", NOW));
    assert!(queue.push("wss://marked.example", NOW));
    assert!(queue.push("wss://found.example", NOW));

    open.url = "wss://Open.Example/".to_owned();
    open.publish_refused = true;
    open.next_try = 1;
    open.visit.as_mut().unwrap().at = NOW - 40;
    marked.publish_refused = false;
    marked.next_try = 1;
    marked.visit.as_mut().unwrap().at = NOW - 40;
    save(
        &path,
        &Pool {
            rows: vec![open, marked],
        },
    )
    .unwrap();

    let seen = Arc::new(Mutex::new(Vec::new()));
    run_batch(
        &mut queue,
        2,
        NOW,
        {
            let seen = Arc::clone(&seen);
            move || Script {
                seen: Arc::clone(&seen),
                overlap: None,
                discover: false,
                boom: false,
            }
        },
        &path,
    )
    .unwrap();

    assert_eq!(load(&path), *queue.pool());
    let pool = load(&path);
    let open = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://open.example")
        .unwrap();
    assert!(open.publish_refused);
    assert!(open.visit.as_ref().unwrap().socket_answered);
    assert_eq!(open.visit.as_ref().unwrap().at, NOW);
    assert_eq!(open.next_try, NOW + 24 * 60 * 60);
    let marked = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://marked.example")
        .unwrap();
    assert!(!marked.publish_refused);
    assert_eq!(marked.visit.as_ref().unwrap().at, NOW);
    assert_eq!(marked.next_try, NOW + 24 * 60 * 60);
    let found = pool
        .rows
        .iter()
        .find(|row| row.url == "wss://found.example")
        .unwrap();
    assert!(!found.publish_refused);
    assert_eq!(found.visit.as_ref().unwrap().at, NOW);
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["relay-pool.json".to_owned()]);
}

fn stored(url: &str, refused: bool) -> Row {
    Row {
        url: url.to_owned(),
        first_seen: 1_700_000_000,
        next_try: NOW,
        failures: 0,
        publish_refused: refused,
        seen_from: Vec::new(),
        visit: Some(Visit {
            at: NOW - 5,
            socket_answered: false,
            nip11: None,
            probes: Vec::new(),
        }),
    }
}
