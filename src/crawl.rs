use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crawler::{due, load, run_batch, Queue, Visit, VisitHook};
use serde_json::Value;

use crate::nostr::{fetch_nip11, probe_kind, read_relay_list, RelayList, RelaySocks};

pub(crate) const DOORS: [&str; 3] = [
    "wss://relay.primal.net",
    "wss://nos.lol",
    "wss://relay.damus.io",
];

pub(crate) const WORKERS: usize = 4;

pub(crate) fn pool_path(content_dir: &Path) -> PathBuf {
    content_dir.join("relay-pool.json")
}

pub(crate) fn batch_queue(path: &Path, now: i64) -> Queue {
    let pool = load(path);
    let due_urls = due(&pool.rows, now);
    let mut queue = Queue::new(pool);
    for door in DOORS {
        queue.push(door, now);
    }
    for url in due_urls {
        queue.push(&url, now);
    }
    queue
}

pub(crate) fn run_crawl<H, F>(path: &Path, now: i64, make_hook: F) -> io::Result<()>
where
    H: VisitHook,
    F: Fn() -> H + Sync,
{
    let mut queue = batch_queue(path, now);
    run_batch(&mut queue, WORKERS, now, make_hook, path)
}

pub(crate) struct CrawlVisit<R> {
    timeout_secs: u64,
    read: R,
}

pub(crate) struct LiveRead {
    socks: RelaySocks,
}

impl CrawlVisit<LiveRead> {
    pub(crate) fn new(timeout_secs: u64, socks: RelaySocks) -> Self {
        Self {
            timeout_secs,
            read: LiveRead { socks },
        }
    }
}

trait RelayRead {
    fn read_list(&mut self, url: &str, deadline: Instant) -> RelayList;
    fn read_nip11(&mut self, url: &str, deadline: Instant) -> Option<String>;
}

impl RelayRead for LiveRead {
    fn read_list(&mut self, url: &str, deadline: Instant) -> RelayList {
        read_relay_list(url, deadline, self.socks)
    }

    fn read_nip11(&mut self, url: &str, deadline: Instant) -> Option<String> {
        fetch_nip11(url, deadline, self.socks)
    }
}

impl<R: RelayRead> VisitHook for CrawlVisit<R> {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        let deadline = Instant::now() + Duration::from_secs(self.timeout_secs.max(1));
        let list = self.read.read_list(url, deadline);
        let nip11 = nip11_document(self.read.read_nip11(url, deadline));
        let probes = probe_kind(url, nip11.as_ref(), deadline);
        (
            Visit {
                at: now,
                socket_answered: list.socket_answered,
                nip11,
                probes,
            },
            list.urls,
        )
    }
}

fn nip11_document(body: Option<String>) -> Option<Value> {
    let value: Value = serde_json::from_str(&body?).ok()?;
    if value.is_object() {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crawler::{save, Pool, ProbeResult, Row, Visit, VisitHook};
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

    fn row(url: &str, next_try: i64, visit: Option<Visit>) -> Row {
        Row {
            url: url.to_owned(),
            first_seen: 1_700_000_000,
            next_try,
            failures: 0,
            visit,
        }
    }

    struct Script {
        seen: Arc<Mutex<Vec<String>>>,
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

        let mut queue = batch_queue(&path, NOW);

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
    fn a_non_public_document_is_stored_with_its_urls_and_no_probe() {
        let body = include_str!("../test/nip11/auth.json").to_owned();
        let document: serde_json::Value = serde_json::from_str(&body).unwrap();
        let mut hook = CrawlVisit {
            timeout_secs: 30,
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

        assert_eq!(hook.read.calls, ["list", "nip11"]);
        assert_eq!(hook.read.deadlines[0], hook.read.deadlines[1]);
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
        assert!(queue
            .pool()
            .rows
            .iter()
            .any(|row| row.url == "wss://relay.damus.io"));
        assert!(queue
            .pool()
            .rows
            .iter()
            .any(|row| row.url == "wss://nos.lol"));
    }

    #[test]
    fn a_missed_socket_still_returns_the_visit() {
        let mut hook = CrawlVisit::new(30, crate::nostr::RelaySocks::Disabled);
        let started = Instant::now();
        let (visit, urls) = hook.visit("wss://warm.example", NOW);
        assert!(started.elapsed() < Duration::from_millis(50));
        assert_eq!(visit.at, NOW);
        assert!(!visit.socket_answered);
        assert!(visit.nip11.is_none());
        assert!(visit.probes.is_empty());
        assert!(urls.is_empty());
    }
}
