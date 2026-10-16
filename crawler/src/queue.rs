use std::collections::{HashSet, VecDeque};

use crate::schedule::{is_due, schedule};
use crate::{canonical_public_relay, Pool, Row, Visit, VisitHook};

const MAX_POOL_ROWS: usize = 512;

#[derive(Debug)]
pub struct Queue {
    pool: Pool,
    ready: VecDeque<String>,
    in_flight: HashSet<String>,
    rotation: usize,
    usable: fn(&Row) -> bool,
}

fn outside_rotation(_: &Row) -> bool {
    false
}

/// A discovery may name its author as `pubkey\nurl`. Anything else is a bare
/// URL, and the relay that returned it stays the recorded source.
fn list_name<'a>(raw: &'a str, visited: &'a str) -> (&'a str, &'a str) {
    if let Some((source, url)) = raw.split_once('\n') {
        if pubkey_hex(source) && !url.is_empty() {
            return (url, source);
        }
    }
    (raw, visited)
}

fn source_id(from: &str) -> Option<String> {
    if pubkey_hex(from) {
        return Some(from.to_ascii_lowercase());
    }
    canonical_public_relay(from)
}

fn pubkey_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
}

impl Queue {
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            ready: VecDeque::new(),
            in_flight: HashSet::new(),
            rotation: usize::MAX,
            usable: outside_rotation,
        }
    }

    pub fn set_rotation(&mut self, cap: usize, usable: fn(&Row) -> bool) {
        self.rotation = cap;
        self.usable = usable;
    }

    pub fn needs_relays(&self) -> bool {
        self.usable_count() < self.rotation
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub(crate) fn pool_mut(&mut self) -> &mut Pool {
        &mut self.pool
    }

    pub fn push(&mut self, url: &str, now: i64) -> bool {
        self.enqueue(url, now, false)
    }

    pub fn revisit(&mut self, url: &str, now: i64) -> bool {
        self.enqueue(url, now, true)
    }

    fn enqueue(&mut self, url: &str, now: i64, force: bool) -> bool {
        let Some(url) = self.consider(url, now, force) else {
            return false;
        };
        self.ready.push_back(url);
        true
    }

    pub fn pop(&mut self) -> Option<String> {
        let url = self.ready.pop_front()?;
        self.in_flight.insert(url.clone());
        Some(url)
    }

    pub fn idle(&self) -> bool {
        self.ready.is_empty() && self.in_flight.is_empty()
    }

    pub fn visit_one(&mut self, hook: &mut impl VisitHook, now: i64) -> Option<String> {
        let url = self.pop()?;
        let (visit, discovered) = hook.visit(&url, now);
        self.finish(&url, visit, discovered, now);
        Some(url)
    }

    pub fn finish(
        &mut self,
        url: &str,
        visit: Visit,
        discovered: impl IntoIterator<Item = impl AsRef<str>>,
        now: i64,
    ) {
        let Some(url) = canonical_public_relay(url) else {
            return;
        };
        if !self.in_flight.contains(&url) {
            return;
        }
        if let Some(row) = self.pool.rows.iter_mut().find(|row| row.url == url) {
            row.visit = Some(visit);
            schedule(row, now);
        }
        self.in_flight.remove(&url);

        let mut found = Vec::new();
        for raw in discovered {
            let (url_text, source) = list_name(raw.as_ref(), &url);
            let discovered_url = self.consider(url_text, now, false);
            self.note_source(url_text, source);
            let Some(url) = discovered_url else {
                continue;
            };
            if found.iter().any(|queued: &String| queued == &url) {
                continue;
            }
            found.push(url);
        }
        for url in found.into_iter().rev() {
            self.ready.push_front(url);
        }
    }

    fn consider(&mut self, raw: &str, now: i64, force: bool) -> Option<String> {
        let url = canonical_public_relay(raw)?;
        if !self.pool.rows.iter().any(|row| row.url == url) {
            if self.pool.rows.len() >= MAX_POOL_ROWS && !self.evict_advertisement() {
                return None;
            }
            self.pool.rows.push(Row {
                url: url.clone(),
                first_seen: now,
                next_try: now,
                failures: 0,
                publish_refused: false,
                seen_from: Vec::new(),
                visit: None,
            });
        }
        if self.in_flight.contains(&url) || self.ready.iter().any(|queued| queued == &url) {
            return None;
        }
        let due = self
            .pool
            .rows
            .iter()
            .any(|row| row.url == url && is_due(row, now));
        if self.needs_relays() {
            if force || due {
                Some(url)
            } else {
                None
            }
        } else if due && self.in_rotation(&url) {
            Some(url)
        } else {
            None
        }
    }

    fn evict_advertisement(&mut self) -> bool {
        // One list named these. They are the first rows to give up a slot.
        if let Some(index) = self.evictable_index(|row| row.seen_from.len() == 1) {
            self.remove_row(index);
            return true;
        }
        // A row nobody has named yet is not corroborated.
        if let Some(index) = self.evictable_index(|row| row.seen_from.is_empty()) {
            self.remove_row(index);
            return true;
        }
        // Two lists can fill every slot. Drop the oldest of those so the
        // file stays capped and a later relay can still enter.
        let Some(index) = self.evictable_index(|row| row.seen_from.len() >= 2) else {
            return false;
        };
        self.remove_row(index);
        true
    }

    fn evictable_index(&self, keep: impl Fn(&Row) -> bool) -> Option<usize> {
        self.pool
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| keep(row) && !self.in_flight.contains(&row.url))
            .min_by_key(|(index, row)| (row.first_seen, *index))
            .map(|(index, _)| index)
    }

    fn remove_row(&mut self, index: usize) {
        let url = self.pool.rows.remove(index).url;
        self.ready.retain(|queued| queued != &url);
    }

    fn note_source(&mut self, raw: &str, from: &str) {
        let Some(url) = canonical_public_relay(raw) else {
            return;
        };
        let Some(from) = source_id(from) else {
            return;
        };
        if url == from {
            return;
        }
        let Some(row) = self.pool.rows.iter_mut().find(|row| row.url == url) else {
            return;
        };
        if row.seen_from.len() < 2 && !row.seen_from.iter().any(|have| have == &from) {
            row.seen_from.push(from);
        }
    }

    fn usable_count(&self) -> usize {
        self.pool
            .rows
            .iter()
            .filter(|row| (self.usable)(row))
            .count()
    }

    fn in_rotation(&self, url: &str) -> bool {
        self.pool
            .rows
            .iter()
            .filter(|row| (self.usable)(row))
            .take(self.rotation)
            .any(|row| row.url == url)
    }
}
