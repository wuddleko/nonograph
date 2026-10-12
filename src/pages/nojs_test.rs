use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

thread_local! {
    static NOJS_TEST_ACCEPTED: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn nojs_test_accepted() -> Option<Vec<String>> {
    NOJS_TEST_ACCEPTED.with(|slot| slot.borrow().clone())
}

pub(super) fn set_nojs_test_accepted(accepted: Option<Vec<String>>) {
    NOJS_TEST_ACCEPTED.with(|slot| *slot.borrow_mut() = accepted);
}

pub(super) enum NojsTestFetch {
    Live,
    Missing,
    Capture,
    Event(String),
}

static NOJS_TEST_FETCH: Mutex<NojsTestFetch> = Mutex::new(NojsTestFetch::Live);

thread_local! {
    static NOJS_TEST_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) static NOJS_FETCH_HINTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(super) static NOJS_RECORD_HINTS: AtomicBool = AtomicBool::new(false);

pub(super) fn note_nojs_fetch_hints(hints: &[String]) {
    if NOJS_RECORD_HINTS.load(Ordering::Relaxed) {
        *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = hints.to_vec();
    }
}

pub(super) fn nojs_fetch_hints_seen() -> Vec<String> {
    NOJS_FETCH_HINTS.lock().expect("nojs fetch hints").clone()
}

pub(super) fn set_nojs_test_fetch(next: NojsTestFetch) {
    let capture = matches!(next, NojsTestFetch::Capture);
    NOJS_TEST_CAPTURE.with(|flag| flag.set(capture));
    *NOJS_TEST_FETCH.lock().expect("nojs test fetch") = next;
}

pub(super) fn remember_nojs_test_event(event_json: &str) {
    if !NOJS_TEST_CAPTURE.with(|flag| flag.get()) {
        return;
    }
    let mut slot = NOJS_TEST_FETCH.lock().expect("nojs test fetch");
    if matches!(*slot, NojsTestFetch::Capture) {
        *slot = NojsTestFetch::Event(event_json.to_string());
    }
}

pub(super) fn canned_nojs_event() -> Option<Option<String>> {
    match &*NOJS_TEST_FETCH.lock().expect("nojs test fetch") {
        NojsTestFetch::Live | NojsTestFetch::Capture => None,
        NojsTestFetch::Missing => Some(None),
        NojsTestFetch::Event(event) => Some(Some(event.clone())),
    }
}
