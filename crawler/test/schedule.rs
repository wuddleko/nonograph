use super::*;
use serde_json::json;

fn row_at(failures: u32, answered: bool) -> Row {
    Row {
        url: "wss://relay.example".to_owned(),
        first_seen: 1_700_000_000,
        next_try: 1_700_000_000,
        failures,
        visit: Some(Visit {
            at: 1_700_000_000,
            socket_answered: answered,
            nip11: Some(json!({})),
            probes: Vec::new(),
        }),
    }
}

#[test]
fn an_answer_waits_one_day_and_clears_failures() {
    let now = 1_700_000_100;
    let mut row = row_at(4, true);

    assert_eq!(schedule(&mut row, now), now + 24 * 60 * 60);
    assert_eq!(row.next_try, now + 24 * 60 * 60);
    assert_eq!(row.failures, 0);
}

#[test]
fn the_first_miss_waits_fifteen_minutes() {
    let now = 1_700_000_100;
    let mut row = row_at(0, false);

    assert_eq!(schedule(&mut row, now), now + 15 * 60);
    assert_eq!(row.next_try, now + 15 * 60);
    assert_eq!(row.failures, 1);
}

#[test]
fn the_second_miss_waits_one_hour() {
    let now = 1_700_000_100;
    let mut row = row_at(1, false);

    assert_eq!(schedule(&mut row, now), now + 60 * 60);
    assert_eq!(row.next_try, now + 60 * 60);
    assert_eq!(row.failures, 2);
}

#[test]
fn the_third_miss_waits_six_hours() {
    let now = 1_700_000_100;
    let mut row = row_at(2, false);

    assert_eq!(schedule(&mut row, now), now + 6 * 60 * 60);
    assert_eq!(row.next_try, now + 6 * 60 * 60);
    assert_eq!(row.failures, 3);
}

#[test]
fn later_misses_stay_at_six_hours() {
    let now = 1_700_000_100;
    let mut row = row_at(3, false);

    assert_eq!(schedule(&mut row, now), now + 6 * 60 * 60);
    assert_eq!(row.next_try, now + 6 * 60 * 60);
    assert_eq!(row.failures, 4);
}

#[test]
fn a_row_with_no_visit_is_due_at_the_timestamp() {
    let now = 1_700_000_100;
    let mut row = Row {
        url: "wss://relay.example".to_owned(),
        first_seen: 1_700_000_000,
        next_try: 1_800_000_000,
        failures: 2,
        visit: None,
    };

    assert_eq!(schedule(&mut row, now), now);
    assert_eq!(row.next_try, now);
    assert_eq!(row.failures, 2);
    assert_eq!(row.url, "wss://relay.example");
    assert_eq!(row.first_seen, 1_700_000_000);
    assert!(row.visit.is_none());
}

#[test]
fn scheduling_one_miss_twice_counts_two_misses() {
    let now = 1_700_000_100;
    let mut row = row_at(0, false);

    assert_eq!(schedule(&mut row, now), now + 15 * 60);
    assert_eq!(row.failures, 1);
    assert_eq!(schedule(&mut row, now), now + 60 * 60);
    assert_eq!(row.next_try, now + 60 * 60);
    assert_eq!(row.failures, 2);
}

#[test]
fn scheduling_one_answer_twice_stays_at_one_day() {
    let now = 1_700_000_100;
    let mut row = row_at(4, true);

    assert_eq!(schedule(&mut row, now), now + 24 * 60 * 60);
    assert_eq!(schedule(&mut row, now), now + 24 * 60 * 60);
    assert_eq!(row.next_try, now + 24 * 60 * 60);
    assert_eq!(row.failures, 0);
}

#[test]
fn a_refused_probe_on_an_open_socket_waits_one_day() {
    let now = 1_700_000_100;
    let mut row = row_at(3, true);
    row.visit.as_mut().unwrap().probes = vec![ProbeResult {
        name: "30323".to_owned(),
        accepted: false,
        detail: "rejected".to_owned(),
    }];

    assert_eq!(schedule(&mut row, now), now + 24 * 60 * 60);
    assert_eq!(row.next_try, now + 24 * 60 * 60);
    assert_eq!(row.failures, 0);
    assert!(!row.visit.as_ref().unwrap().probes[0].accepted);
}

#[test]
fn the_wait_uses_the_given_timestamp() {
    let now = 1_700_000_100;
    let mut row = row_at(0, false);
    row.visit.as_mut().unwrap().at = 1_600_000_000;

    assert_eq!(schedule(&mut row, now), now + 15 * 60);
    assert_eq!(row.visit.as_ref().unwrap().at, 1_600_000_000);
}

#[test]
fn an_answer_restarts_the_miss_ladder() {
    let now = 1_700_000_100;
    let mut row = row_at(4, true);
    schedule(&mut row, now);
    row.visit.as_mut().unwrap().socket_answered = false;

    assert_eq!(schedule(&mut row, now), now + 15 * 60);
    assert_eq!(row.next_try, now + 15 * 60);
    assert_eq!(row.failures, 1);
}
