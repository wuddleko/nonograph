mod list;
mod queue;
mod relay_url;
mod schedule;
mod visit;

pub use list::{list_urls, list_urls_with};
pub use queue::Queue;
pub use relay_url::canonical_public_relay;
pub use schedule::{due, schedule};
pub use visit::VisitHook;

use std::fs;
use std::io;
use std::path::Path;

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
    fs::write(path, bytes)
}

pub fn load(path: &Path) -> Pool {
    let Ok(bytes) = fs::read(path) else {
        return Pool { rows: Vec::new() };
    };
    serde_json::from_slice(&bytes).unwrap_or(Pool { rows: Vec::new() })
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
