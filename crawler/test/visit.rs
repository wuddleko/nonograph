use super::*;
use serde_json::json;

const NOW: i64 = 1_700_000_100;

struct Script {
    visit: Visit,
    urls: Vec<String>,
    seen: Vec<String>,
}

impl VisitHook for Script {
    fn visit(&mut self, url: &str, now: i64) -> (Visit, Vec<String>) {
        self.seen.push(url.to_owned());
        let mut visit = self.visit.clone();
        visit.at = now;
        (visit, self.urls.clone())
    }
}

fn script(answered: bool, urls: &[&str]) -> Script {
    Script {
        visit: Visit {
            at: 0,
            socket_answered: answered,
            nip11: None,
            probes: Vec::new(),
        },
        urls: urls.iter().map(|url| (*url).to_owned()).collect(),
        seen: Vec::new(),
    }
}

#[test]
fn an_answer_waits_one_day_and_the_next_pop_is_the_first_url() {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    queue.push("wss://a.example", NOW);
    let mut hook = script(true, &["wss://b.example", "wss://c.example"]);

    queue.visit_one(&mut hook, NOW);

    let row = &queue.pool().rows[0];
    assert!(row.visit.as_ref().unwrap().socket_answered);
    assert_eq!(row.failures, 0);
    assert_eq!(row.next_try, NOW + 24 * 60 * 60);
    assert_eq!(queue.pop().as_deref(), Some("wss://b.example"));
    assert_eq!(queue.pop().as_deref(), Some("wss://c.example"));
}

#[test]
fn a_miss_keeps_the_row_and_waits_fifteen_minutes() {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    queue.push("wss://a.example", NOW);
    let mut hook = script(false, &[]);

    queue.visit_one(&mut hook, NOW);

    assert_eq!(queue.pool().rows.len(), 1);
    assert_eq!(queue.pool().rows[0].url, "wss://a.example");
    assert!(!queue.pool().rows[0].visit.as_ref().unwrap().socket_answered);
    assert_eq!(queue.pool().rows[0].failures, 1);
    assert_eq!(queue.pool().rows[0].next_try, NOW + 15 * 60);
    assert_eq!(queue.pop(), None);
}

#[test]
fn a_document_and_probe_are_stored_unchanged() {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    queue.push("wss://a.example", NOW);
    let visit = Visit {
        at: NOW,
        socket_answered: true,
        nip11: Some(json!({ "note": "kept" })),
        probes: vec![ProbeResult {
            name: "marker".to_owned(),
            accepted: false,
            detail: "left as sent".to_owned(),
        }],
    };
    let mut hook = Script {
        visit: visit.clone(),
        urls: Vec::new(),
        seen: Vec::new(),
    };

    queue.visit_one(&mut hook, NOW);

    assert_eq!(queue.pool().rows[0].visit.as_ref(), Some(&visit));
}

#[test]
fn the_fake_receives_the_canonical_url() {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    queue.push("wss://A.Example/", NOW);
    let mut hook = script(true, &[]);

    queue.visit_one(&mut hook, NOW);

    assert_eq!(hook.seen, vec!["wss://a.example".to_owned()]);
}

#[test]
fn an_empty_ready_list_returns_none_without_calling_the_hook() {
    let mut queue = Queue::new(Pool { rows: Vec::new() });
    let mut hook = script(true, &["wss://b.example"]);

    assert_eq!(queue.visit_one(&mut hook, NOW), None);
    assert!(hook.seen.is_empty());
    assert!(queue.pool().rows.is_empty());
}
