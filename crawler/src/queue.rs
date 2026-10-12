use std::collections::{HashSet, VecDeque};

use crate::schedule::{is_due, schedule};
use crate::{canonical_public_relay, Pool, Row, Visit};

#[derive(Debug)]
pub struct Queue {
    pool: Pool,
    ready: VecDeque<String>,
    in_flight: HashSet<String>,
}

impl Queue {
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            ready: VecDeque::new(),
            in_flight: HashSet::new(),
        }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub fn push(&mut self, url: &str, now: i64) -> bool {
        let Some(url) = self.consider(url, now) else {
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
            let Some(url) = self.consider(raw.as_ref(), now) else {
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

    fn consider(&mut self, raw: &str, now: i64) -> Option<String> {
        let url = canonical_public_relay(raw)?;
        if !self.pool.rows.iter().any(|row| row.url == url) {
            self.pool.rows.push(Row {
                url: url.clone(),
                first_seen: now,
                next_try: now,
                failures: 0,
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
        if due {
            Some(url)
        } else {
            None
        }
    }
}
