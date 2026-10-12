use crate::Row;

pub fn due(rows: &[Row], now: i64) -> Vec<String> {
    rows.iter()
        .filter(|row| is_due(row, now))
        .map(|row| row.url.clone())
        .collect()
}

pub(crate) fn is_due(row: &Row, now: i64) -> bool {
    row.visit.is_none() || row.next_try <= now
}

const MINUTE: i64 = 60;
const WAIT_ANSWERED: i64 = 24 * 60 * MINUTE;
const WAIT_FIRST: i64 = 15 * MINUTE;
const WAIT_SECOND: i64 = 60 * MINUTE;
const WAIT_LATER: i64 = 6 * 60 * MINUTE;

pub fn schedule(row: &mut Row, now: i64) -> i64 {
    let at = match row.visit.as_ref() {
        Some(visit) if visit.socket_answered => {
            row.failures = 0;
            now.saturating_add(WAIT_ANSWERED)
        }
        Some(_) => {
            row.failures = row.failures.saturating_add(1);
            now.saturating_add(wait_for_failure(row.failures))
        }
        None => now,
    };
    row.next_try = at;
    at
}

fn wait_for_failure(failures: u32) -> i64 {
    match failures {
        0 | 1 => WAIT_FIRST,
        2 => WAIT_SECOND,
        _ => WAIT_LATER,
    }
}
