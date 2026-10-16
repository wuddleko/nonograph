use std::collections::{HashSet, VecDeque};

use crate::schedule::{is_due, schedule};
use crate::{canonical_public_relay, Pool, Row, Visit, VisitHook};

pub const MAX_POOL_ROWS: usize = 512;

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
            let Some(url) = self.consider(raw.as_ref(), now, false) else {
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
            if self.pool.rows.len() >= MAX_POOL_ROWS {
                return None;
            }
            self.pool.rows.push(Row {
                url: url.clone(),
                first_seen: now,
                next_try: now,
                failures: 0,
                publish_refused: false,
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
