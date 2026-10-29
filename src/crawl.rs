use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

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
const FOUND_GOAL: usize = 100;
const POOL_FETCH: usize = 3;
const SAVE_RETRY: Duration = Duration::from_secs(60);
const BAD_FILE_WAIT: Duration = Duration::from_secs(15 * 60);
const FIRST_CRAWL: Duration = Duration::from_secs(60 * 60);
const STEADY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const USABLE_FLOOR: usize = 2;
const FLOOR_WAIT: Duration = Duration::from_secs(15 * 60);

pub(crate) fn pool_path(content_dir: &Path) -> PathBuf {
    content_dir.join("relay-pool.json")
}

pub(crate) fn batch_queue(path: &Path, now: i64) -> io::Result<Queue> {
    let pool = read_pool(path)?;
    let first_build = pool.rows.len() < FOUND_GOAL;
    let answered: Vec<String> = pool
        .rows
        .iter()
        .filter(|row| row_answered(row))
        .map(|row| row.url.clone())
        .collect();
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
    if first_build {
        for url in answered {
            queue.revisit(&url, now);
        }
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
            let first_build = found_in(&path) < FOUND_GOAL;
            run_crawl(&path, now, || {
                CrawlVisit::new(timeout_secs, socks, filling, first_build)
            })
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
    let boot = Instant::now();
    loop {
        let now = crate::nostr::now_secs();
        eprintln!("Nostorgraph: {}", crawl_start_line(path));
        let crawled = crawl(now);
        if let Err(error) = &crawled {
            eprintln!("Nostorgraph: relay pool was not saved: {error}");
        }
        let now = crate::nostr::now_secs();
        let wait = match &crawled {
            Err(error) if error.kind() == io::ErrorKind::InvalidData => BAD_FILE_WAIT,
            Err(_) => pool_file_wait(path, now).max(SAVE_RETRY),
            Ok(()) => pass_wait(path, boot.elapsed(), now),
        };
        eprintln!("Nostorgraph: {}", crawl_pause_line(path, wait));
        if !sleep(wait) {
            return;
        }
    }
}

fn crawl_start_line(path: &Path) -> String {
    format!("relay crawl starting, {}", crawl_progress(path))
}

fn crawl_pause_line(path: &Path, wait: Duration) -> String {
    if wait.is_zero() {
        return format!("relay crawl continuing, {}", crawl_progress(path));
    }
    format!(
        "relay crawl paused, {}, next pass in {}",
        crawl_progress(path),
        wait_words(wait)
    )
}

fn crawl_progress(path: &Path) -> String {
    let found = found_in(path);
    let usable = usable_in(path);
    if found < FOUND_GOAL {
        format!("{found}/{FOUND_GOAL} relays found, {usable}/{CANDIDATE_CAP} usable")
    } else {
        format!("{usable}/{CANDIDATE_CAP} relays usable")
    }
}

fn found_in(path: &Path) -> usize {
    read_pool(path).map(|pool| pool.rows.len()).unwrap_or(0)
}

fn usable_in(path: &Path) -> usize {
    read_pool(path)
        .map(|pool| pool.rows.iter().filter(|row| row_is_usable(row)).count())
        .unwrap_or(0)
}

fn wait_words(wait: Duration) -> String {
    let secs = wait.as_secs();
    if secs == 0 {
        return "0s".to_string();
    }
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 && hours == 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

fn pool_file_wait(path: &Path, now: i64) -> Duration {
    match read_pool(path) {
        Ok(pool) => pool_wait(&pool.rows, now),
        Err(_) => Duration::ZERO,
    }
}

fn pass_wait(path: &Path, elapsed: Duration, now: i64) -> Duration {
    if found_in(path) < FOUND_GOAL && elapsed < FIRST_CRAWL {
        return Duration::ZERO;
    }
    if usable_in(path) < USABLE_FLOOR {
        return pool_file_wait(path, now).min(FLOOR_WAIT);
    }
    STEADY_INTERVAL
}

fn row_answered(row: &Row) -> bool {
    row.visit
        .as_ref()
        .is_some_and(|visit| visit.socket_answered)
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
    first_build: bool,
    read: R,
}

pub(crate) struct LiveRead {
    socks: RelaySocks,
}

impl CrawlVisit<LiveRead> {
    pub(crate) fn new(
        timeout_secs: u64,
        socks: RelaySocks,
        filling: bool,
        first_build: bool,
    ) -> Self {
        Self {
            timeout_secs,
            filling,
            first_build,
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
        let list = if self.first_build || (self.filling && is_door(url)) {
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
        let visit = Visit {
            at: now,
            socket_answered: list.socket_answered || body.is_some(),
            nip11,
            probes,
        };
        let learned = admit_discovered(url, list.urls);
        eprintln!("Nostorgraph: {}", visit_line(url, &visit, learned.len()));
        (visit, learned)
    }
}

fn visit_line(url: &str, visit: &Visit, learned: usize) -> String {
    format!(
        "relay {url} {}{}",
        visit_outcome(visit),
        learned_words(learned)
    )
}

fn learned_words(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => ", learned 1 relay".to_string(),
        count => format!(", learned {count} relays"),
    }
}

fn visit_outcome(visit: &Visit) -> &'static str {
    if !visit.socket_answered {
        return "did not answer";
    }
    match visit.nip11.as_ref() {
        Some(document) if crate::nip11::document_is_public(document) => {}
        Some(_) => return "is not public",
        None => return "sent no relay document",
    }
    let name = nostorgraph_nip44::KIND.to_string();
    if visit
        .probes
        .iter()
        .any(|probe| probe.accepted && probe.name == name)
    {
        "accepted a public note"
    } else {
        "does not accept public notes"
    }
}

const SIGHTING_CAP: usize = 4096;

struct Sighting {
    sources: Vec<String>,
    seen_at: u64,
}

struct SightingBook {
    next: u64,
    rows: HashMap<String, Sighting>,
}

fn sightings() -> &'static Mutex<SightingBook> {
    static SEEN: OnceLock<Mutex<SightingBook>> = OnceLock::new();
    SEEN.get_or_init(|| {
        Mutex::new(SightingBook {
            next: 0,
            rows: HashMap::new(),
        })
    })
}

#[cfg(test)]
fn clear_sightings() {
    let mut seen = sightings()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    seen.next = 0;
    seen.rows.clear();
}

fn parent_source(url: &str) -> Option<String> {
    let url = crawler::canonical_public_relay(url)?;
    let rest = url.strip_prefix("wss://")?;
    let host = rest.split(['/', ':']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub(crate) fn admit_discovered(from: &str, urls: Vec<String>) -> Vec<String> {
    let Some(source) = parent_source(from) else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    let mut seen = sightings()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for url in urls {
        let Some(url) = crawler::canonical_public_relay(&url) else {
            continue;
        };
        if note_sighting(&mut seen, &url, &source) {
            kept.push(url);
        }
    }
    kept
}

fn note_sighting(book: &mut SightingBook, url: &str, source: &str) -> bool {
    book.next = book.next.saturating_add(1);
    let seen_at = book.next;
    if let Some(row) = book.rows.get_mut(url) {
        row.seen_at = seen_at;
        if row.sources.len() < 2 && !row.sources.iter().any(|have| have == source) {
            row.sources.push(source.to_string());
        }
        return row.sources.len() >= 2;
    }
    if book.rows.len() >= SIGHTING_CAP {
        if let Some(oldest) = oldest_sighting(&book.rows) {
            book.rows.remove(&oldest);
        }
    }
    book.rows.insert(
        url.to_string(),
        Sighting {
            sources: vec![source.to_string()],
            seen_at,
        },
    );
    false
}

fn oldest_sighting(rows: &HashMap<String, Sighting>) -> Option<String> {
    rows.iter()
        .min_by(|left, right| {
            let left_single = left.1.sources.len() < 2;
            let right_single = right.1.sources.len() < 2;
            right_single
                .cmp(&left_single)
                .then(left.1.seen_at.cmp(&right.1.seen_at))
        })
        .map(|(url, _)| url.clone())
}

struct CachedPool {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    generation: u64,
    rows: Vec<Row>,
}

fn pool_cache() -> &'static Mutex<Option<CachedPool>> {
    static CACHE: OnceLock<Mutex<Option<CachedPool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn read_cached_rows(path: &Path) -> io::Result<Vec<Row>> {
    let meta = std::fs::metadata(path).ok();
    let modified = meta.as_ref().and_then(|meta| meta.modified().ok());
    let len = meta.as_ref().map(|meta| meta.len()).unwrap_or(0);
    let generation = crawler::pool_generation();
    let mut slot = pool_cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(hit) = slot.as_ref() {
        if hit.path == path
            && hit.modified == modified
            && hit.len == len
            && hit.generation == generation
        {
            return Ok(hit.rows.clone());
        }
    }
    let rows = read_pool(path)?.rows;
    *slot = Some(CachedPool {
        path: path.to_path_buf(),
        modified,
        len,
        generation,
        rows: rows.clone(),
    });
    Ok(rows)
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
    let rows = match read_cached_rows(path) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!(
                "Nostorgraph: relay pool {} was not read: {error}",
                path.display()
            );
            Vec::new()
        }
    };
    rows.into_iter().filter(row_is_usable).collect()
}

pub(crate) fn candidates(path: &Path, configured: &[String]) -> Vec<String> {
    match read_cached_rows(path) {
        Ok(rows) => rows
            .into_iter()
            .filter(row_is_usable)
            .take(CANDIDATE_CAP)
            .map(|row| row.url)
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            eprintln!(
                "Nostorgraph: relay pool {} was not read: {error}",
                path.display()
            );
            configured_relays(configured)
        }
        Err(error) => {
            eprintln!(
                "Nostorgraph: relay pool {} was not read: {error}",
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
    let name = nostorgraph_nip44::KIND.to_string();
    visit
        .probes
        .iter()
        .any(|probe| probe.accepted && probe.name == name)
}

#[cfg(test)]
#[path = "../test/crawl.rs"]
mod tests;
