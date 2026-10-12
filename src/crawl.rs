use std::io;
use std::path::{Path, PathBuf};

use crawler::{due, load, run_batch, Queue, VisitHook};

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
}
