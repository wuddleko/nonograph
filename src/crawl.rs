use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crawler::{due, edit_pool, read_pool, run_batch, Queue, Row, Visit, VisitHook};
use serde_json::Value;

use crate::nostr::{fetch_nip11, probe_kind, read_relay_list, RelayList, RelaySocks};

pub(crate) const DOORS: [&str; 3] = [
    "wss://relay.primal.net",
    "wss://nos.lol",
    "wss://relay.damus.io",
];

pub(crate) const WORKERS: usize = 4;

pub(crate) const CANDIDATE_CAP: usize = 48;
const POOL_FETCH: usize = 3;
const SAVE_RETRY: Duration = Duration::from_secs(60);
const BAD_FILE_WAIT: Duration = Duration::from_secs(15 * 60);
const PASS_PAUSE: Duration = Duration::from_secs(15 * 60);

pub(crate) fn pool_path(content_dir: &Path) -> PathBuf {
    content_dir.join("relay-pool.json")
}

pub(crate) fn batch_queue(path: &Path, now: i64) -> io::Result<Queue> {
    let pool = read_pool(path)?;
    let due_urls = due(&pool.rows, now);
    let mut queue = Queue::new(pool);
    queue.set_rotation(CANDIDATE_CAP, row_is_usable);
    if queue.needs_relays() {
        for door in DOORS {
            queue.revisit(door, now);
        }
    }
    for url in due_urls {
        queue.push(&url, now);
    }
    Ok(queue)
}

pub(crate) fn run_crawl<H, F>(path: &Path, now: i64, make_hook: F) -> io::Result<()>
where
    H: VisitHook,
    F: Fn() -> H + Sync,
{
    let mut queue = batch_queue(path, now)?;
    run_batch(&mut queue, WORKERS, now, make_hook, path)
}

pub(crate) fn start_pool<F>(socks: RelaySocks, start: F) -> bool
where
    F: FnOnce() + Send + 'static,
{
    if !matches!(socks, RelaySocks::Proxy(_)) {
        return false;
    }
    std::thread::spawn(start);
    true
}

pub(crate) fn run_pool(socks: RelaySocks, timeout_secs: u64) {
    let path = pool_path(Path::new("content"));
    serve_pool(
        &path,
        |now| {
            let filling = pool_is_filling(&path);
            run_crawl(&path, now, || CrawlVisit::new(timeout_secs, socks, filling))
        },
        |wait| {
            std::thread::sleep(wait);
            true
        },
    );
}

fn serve_pool<C, S>(path: &Path, mut crawl: C, mut sleep: S)
where
    C: FnMut(i64) -> io::Result<()>,
    S: FnMut(Duration) -> bool,
{
    loop {
        let now = crate::nostr::now_secs();
        let crawled = crawl(now);
        if let Err(error) = &crawled {
            eprintln!("Nonograph: relay pool was not saved: {error}");
        }
        let now = crate::nostr::now_secs();
        let wait = match &crawled {
            Err(error) if error.kind() == io::ErrorKind::InvalidData => BAD_FILE_WAIT,
            Err(_) => pool_file_wait(path, now).max(SAVE_RETRY),
            Ok(()) => pass_wait(path, now),
        };
        if !sleep(wait) {
            return;
        }
    }
}

fn pool_file_wait(path: &Path, now: i64) -> Duration {
    match read_pool(path) {
        Ok(pool) => pool_wait(&pool.rows, now),
        Err(_) => Duration::ZERO,
    }
}

fn pass_wait(path: &Path, now: i64) -> Duration {
    let Ok(pool) = read_pool(path) else {
        return Duration::ZERO;
    };
    if rows_fill_rotation(&pool.rows) {
        return usable_wait(&pool.rows, now);
    }
    let wait = pool_wait(&pool.rows, now);
    if wait.is_zero() && !due(&pool.rows, now).is_empty() {
        PASS_PAUSE
    } else {
        wait
    }
}

fn rows_fill_rotation(rows: &[Row]) -> bool {
    rows.iter().filter(|row| row_is_usable(row)).count() >= CANDIDATE_CAP
}

fn pool_is_filling(path: &Path) -> bool {
    match read_pool(path) {
        Ok(pool) => !rows_fill_rotation(&pool.rows),
        Err(_) => false,
    }
}

fn usable_wait(rows: &[Row], now: i64) -> Duration {
    let Some(next) = rows
        .iter()
        .filter(|row| row_is_usable(row))
        .take(CANDIDATE_CAP)
        .map(|row| row.next_try)
        .min()
    else {
        return Duration::ZERO;
    };
    let delay = next.saturating_sub(now);
    if delay <= 0 {
        PASS_PAUSE
    } else {
        Duration::from_secs(delay as u64)
    }
}

fn pool_wait(rows: &[Row], now: i64) -> Duration {
    let Some(next) = rows.iter().map(|row| row.next_try).min() else {
        return Duration::ZERO;
    };
    let delay = next.saturating_sub(now);
    if delay <= 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(delay as u64)
    }
}

pub(crate) struct CrawlVisit<R> {
    timeout_secs: u64,
    filling: bool,
    read: R,
}

pub(crate) struct LiveRead {
    socks: RelaySocks,
}

impl CrawlVisit<LiveRead> {
    pub(crate) fn new(timeout_secs: u64, socks: RelaySocks, filling: bool) -> Self {
        Self {
            timeout_secs,
            filling,
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
        let list = if self.filling && is_door(url) {
            self.read.read_list(url, deadline)
        } else {
            RelayList {
                socket_answered: false,
                urls: Vec::new(),
            }
        };
        let body = self.read.read_nip11(url, deadline);
        let nip11 = nip11_document(body.as_deref());
        let probes = probe_kind(url, nip11.as_ref(), deadline);
        (
            Visit {
                at: now,
                socket_answered: list.socket_answered || body.is_some(),
                nip11,
                probes,
            },
            list.urls,
        )
    }
}

fn is_door(url: &str) -> bool {
    let Some(url) = crawler::canonical_public_relay(url) else {
        return false;
    };
    DOORS.iter().any(|door| *door == url)
}

fn nip11_document(body: Option<&str>) -> Option<Value> {
    let value: Value = serde_json::from_str(body?).ok()?;
    if value.is_object() {
        Some(value)
    } else {
        None
    }
}

pub(crate) fn usable_rows(path: &Path) -> Vec<Row> {
    let rows = match read_pool(path) {
        Ok(pool) => pool.rows,
        Err(error) => {
            eprintln!(
                "Nonograph: relay pool {} was not read: {error}",
                path.display()
            );
            Vec::new()
        }
    };
    rows.into_iter().filter(row_is_usable).collect()
}

pub(crate) fn candidates(path: &Path, configured: &[String]) -> Vec<String> {
    match read_pool(path) {
        Ok(pool) => pool
            .rows
            .into_iter()
            .filter(row_is_usable)
            .take(CANDIDATE_CAP)
            .map(|row| row.url)
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            eprintln!(
                "Nonograph: relay pool {} was not read: {error}",
                path.display()
            );
            configured_relays(configured)
        }
        Err(error) => {
            eprintln!(
                "Nonograph: relay pool {} was not read: {error}",
                path.display()
            );
            Vec::new()
        }
    }
}

fn configured_relays(configured: &[String]) -> Vec<String> {
    let mut urls = Vec::new();
    for relay in configured {
        let Some(relay) = crate::nostr::canonical_public_relay(relay) else {
            continue;
        };
        if urls.iter().any(|have| have == &relay) || urls.len() == CANDIDATE_CAP {
            continue;
        }
        urls.push(relay);
    }
    urls
}

pub(crate) fn mark_publish(path: &Path, refused: &[String], accepted: &[String]) -> io::Result<()> {
    edit_pool(path, |pool| {
        let mut changed = false;
        for row in &mut pool.rows {
            if pool_url_listed(&row.url, refused) && !row.publish_refused {
                row.publish_refused = true;
                changed = true;
            }
            if pool_url_listed(&row.url, accepted) && row.publish_refused {
                row.publish_refused = false;
                changed = true;
            }
        }
        changed
    })
}

fn pool_url_listed(url: &str, urls: &[String]) -> bool {
    urls.iter()
        .any(|raw| crate::nostr::same_public_relay(raw, url))
}

pub(crate) fn next_pool_relays(path: &Path, link: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for row in usable_rows(path) {
        if relays.len() == POOL_FETCH {
            break;
        }
        if link
            .iter()
            .any(|have| crate::nostr::same_public_relay(have, &row.url))
        {
            continue;
        }
        relays.push(row.url);
    }
    relays
}

fn row_is_usable(row: &Row) -> bool {
    if row.publish_refused {
        return false;
    }
    let Some(visit) = row.visit.as_ref() else {
        return false;
    };
    if !visit.socket_answered {
        return false;
    }
    let Some(document) = visit.nip11.as_ref() else {
        return false;
    };
    if !crate::nip11::document_is_public(document) {
        return false;
    }
    let name = nonograph_nip44::KIND.to_string();
    visit
        .probes
        .iter()
        .any(|probe| probe.accepted && probe.name == name)
}

#[cfg(test)]
#[path = "../test/crawl.rs"]
mod tests;
