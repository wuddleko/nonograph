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
