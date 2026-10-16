mod batch;
mod deal;
mod list;
mod queue;
mod relay_url;
mod schedule;
mod visit;

pub use batch::{run_batch, VISIT_CAP};
pub use deal::deal;
pub use list::list_urls;
pub use queue::Queue;
pub use relay_url::{blocked_relay_ip, canonical_public_relay, valid_relay_url};
pub use schedule::{due, schedule};
pub use visit::VisitHook;

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pool {
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub url: String,
    pub first_seen: i64,
    pub next_try: i64,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub publish_refused: bool,
    #[serde(default)]
    pub seen_from: Vec<String>,
    pub visit: Option<Visit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    pub at: i64,
    pub socket_answered: bool,
    pub nip11: Option<Value>,
    pub probes: Vec<ProbeResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub name: String,
    pub accepted: bool,
    pub detail: String,
}

pub fn save(path: &Path, pool: &Pool) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(pool)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

pub fn load(path: &Path) -> Pool {
    let Ok(bytes) = fs::read(path) else {
        return Pool { rows: Vec::new() };
    };
    serde_json::from_slice(&bytes).unwrap_or(Pool { rows: Vec::new() })
}

pub fn read_pool(path: &Path) -> io::Result<Pool> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Pool { rows: Vec::new() }),
        Err(err) => Err(err),
    }
}

fn pool_write_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

pub fn edit_pool(path: &Path, edit: impl FnOnce(&mut Pool) -> bool) -> io::Result<()> {
    let _guard = pool_write_lock()
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let mut pool = read_pool(path)?;
    if edit(&mut pool) {
        save(path, &pool)?;
    }
    Ok(())
}

pub fn save_crawl(path: &Path, crawl: &mut Pool) -> io::Result<()> {
    let mut merged = crawl.clone();
    edit_pool(path, |disk| {
        for row in &mut merged.rows {
            if let Some(flag) = publish_refused_on(&disk.rows, &row.url) {
                row.publish_refused = flag;
            }
        }
        *disk = merged.clone();
        true
    })?;
    *crawl = merged;
    Ok(())
}

fn publish_refused_on(rows: &[Row], url: &str) -> Option<bool> {
    rows.iter()
        .find(|row| same_pool_url(&row.url, url))
        .map(|row| row.publish_refused)
}

fn same_pool_url(left: &str, right: &str) -> bool {
    relay_url::same_publish_relay(left, right)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pool");
    static TEMPS: AtomicU64 = AtomicU64::new(0);
    let tmp = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        TEMPS.fetch_add(1, Ordering::Relaxed)
    ));
    let result = write_temp(&tmp, bytes).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_temp(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
#[path = "../test/row.rs"]
mod tests;

#[cfg(test)]
#[path = "../test/url.rs"]
mod url_tests;

#[cfg(test)]
#[path = "../test/list.rs"]
mod list_tests;

#[cfg(test)]
#[path = "../test/schedule.rs"]
mod schedule_tests;

#[cfg(test)]
#[path = "../test/due.rs"]
mod due_tests;

#[cfg(test)]
#[path = "../test/queue.rs"]
mod queue_tests;

#[cfg(test)]
#[path = "../test/visit.rs"]
mod visit_tests;

#[cfg(test)]
#[path = "../test/batch.rs"]
mod batch_tests;

#[cfg(test)]
#[path = "../test/deal.rs"]
mod deal_tests;
