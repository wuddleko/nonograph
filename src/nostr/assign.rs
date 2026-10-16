use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;

use super::transport::{
    connect_tcp, reap_held_relays, socks5_connect, store_held, tls_connector, tls_handshake,
    SOCKS_MISCONFIGURED,
};
#[cfg(test)]
use super::{canonical_public_relay, same_public_relay, SendFailure};
use super::{public_relay_url, relay_socks, RelaySocks};

pub(super) fn assign_relay(
    relay: &str,
    deadline: Instant,
    socks: RelaySocks,
) -> Result<(), String> {
    let RelaySocks::Proxy(proxy) = socks else {
        return Err(SOCKS_MISCONFIGURED.to_string());
    };
    if !public_relay_url(relay) {
        return Err("relay address is not public".to_string());
    }
    let request = relay
        .into_client_request()
        .map_err(|error| error.to_string())?;
    let uri = request.uri().clone();
    let host = uri.host().ok_or("relay url has no host")?.to_string();
    let port = uri.port_u16().unwrap_or(443);
    let connector = tls_connector()?;
    let connect_by = super::transport::connect_deadline(deadline);
    let tcp = connect_tcp(proxy, connect_by, None)?;
    socks5_connect(&tcp, &host, port, relay, connect_by, None)?;
    let tls = tls_handshake(connector, &host, tcp, connect_by, None)?;
    store_held(relay, tls);
    Ok(())
}

const RELAY_CIRCUIT_WARM: Duration = Duration::from_secs(8 * 60);
const RELAY_CIRCUIT_RETRY: Duration = Duration::from_secs(30);

fn retry_wait(misses: u32) -> Duration {
    let doubled = misses.saturating_sub(1).min(4);
    RELAY_CIRCUIT_RETRY
        .saturating_mul(1_u32 << doubled)
        .min(RELAY_CIRCUIT_WARM)
}

#[cfg(test)]
pub(super) fn relay_needs_assignment(age: Option<Duration>, circuit: Option<bool>) -> bool {
    assignment_due(age, circuit, 1)
}

pub(super) fn assignment_due(age: Option<Duration>, circuit: Option<bool>, misses: u32) -> bool {
    match age {
        None => circuit != Some(true),
        Some(age) if age >= RELAY_CIRCUIT_WARM => true,
        Some(age) if circuit == Some(false) && age >= retry_wait(misses) => true,
        _ => false,
    }
}

pub(super) fn next_misses(previous: Option<u32>, circuit: Option<bool>) -> u32 {
    match circuit {
        Some(true) => 1,
        Some(false) => previous.unwrap_or(0).saturating_add(1),
        None => previous.unwrap_or(1),
    }
}

fn circuit_seen(paths: Option<&HashMap<String, String>>, relay: &str) -> Option<bool> {
    let paths = paths?;
    let key = crate::tor_circuits::relay_key(relay)?;
    Some(paths.contains_key(&key))
}

pub(super) fn relays_needing_assignment(
    relays: &[String],
    paths: Option<&HashMap<String, String>>,
    requested: &HashMap<String, (Duration, u32)>,
) -> Vec<String> {
    relays
        .iter()
        .filter(|relay| {
            let (age, misses) = requested
                .get(relay.as_str())
                .copied()
                .map(|(age, misses)| (Some(age), misses))
                .unwrap_or((None, 1));
            assignment_due(age, circuit_seen(paths, relay), misses)
        })
        .cloned()
        .collect()
}

pub(super) type OpenRelay = Arc<dyn Fn(&str, Duration) -> bool + Send + Sync>;
pub(super) type ReadCircuits = Arc<dyn Fn() -> Option<HashMap<String, String>> + Send + Sync>;

struct PendingAssignment {
    page: Vec<String>,
    configured: Vec<String>,
    timeout: Duration,
    open: OpenRelay,
    circuits: ReadCircuits,
}

// Hidden-service requests share the local address, so dials share one window.
pub(super) const ASSIGN_LIMIT: usize = 18;
pub(super) const ASSIGN_WINDOW: Duration = Duration::from_secs(60);

struct RelayAttempt {
    at: Instant,
    misses: u32,
    ok: bool,
}

pub(super) struct RelayPass {
    latest: Mutex<Option<PendingAssignment>>,
    requested: Mutex<HashMap<String, RelayAttempt>>,
    dialed: Mutex<Vec<Instant>>,
    running: AtomicBool,
}

impl RelayPass {
    pub(super) fn new() -> Self {
        Self {
            latest: Mutex::new(None),
            requested: Mutex::new(HashMap::new()),
            dialed: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
        }
    }

    pub(super) fn submit(
        self: &Arc<Self>,
        page: Vec<String>,
        configured: Vec<String>,
        timeout: Duration,
        open: OpenRelay,
        circuits: ReadCircuits,
    ) {
        {
            let mut latest = self.latest.lock().expect("relay circuit pass");
            *latest = Some(PendingAssignment {
                page,
                configured,
                timeout,
                open,
                circuits,
            });
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let pass = Arc::clone(self);
        if std::thread::Builder::new()
            .name("relay-circuits".to_string())
            .spawn(move || pass.run())
            .is_err()
        {
            self.running.store(false, Ordering::Release);
            eprintln!("Nonograph: could not start relay circuit assignment");
        }
    }

    // A newer list replaces the one waiting. It is used when this pass finishes.
    fn run(self: Arc<Self>) {
        loop {
            let job = {
                let mut latest = self.latest.lock().expect("relay circuit pass");
                latest.take()
            };
            let Some(job) = job else {
                self.running.store(false, Ordering::Release);
                let pending = self.latest.lock().expect("relay circuit pass").is_some();
                if !pending || self.running.swap(true, Ordering::AcqRel) {
                    break;
                }
                continue;
            };
            self.apply(&job);
        }
    }

    fn apply(&self, job: &PendingAssignment) {
        reap_held_relays();
        let chosen = crate::relay_set::relays_to_assign(&job.page, &job.configured);
        let paths = (job.circuits)();
        let noted = self.noted(&chosen, Instant::now());
        for relay in relays_needing_assignment(&chosen, paths.as_ref(), &noted) {
            let now = Instant::now();
            if !self.allow_assignment(now) {
                break;
            }
            let seen = circuit_seen(paths.as_ref(), &relay);
            // A parked handshake that aged out is dialed again, and that refresh
            // does not count as another miss.
            let misses = if seen == Some(false) && self.opened_ok(&relay) {
                1
            } else {
                next_misses(noted.get(relay.as_str()).map(|(_, misses)| *misses), seen)
            };
            self.mark_requested(&relay, now, misses);
            let ok = (job.open)(&relay, job.timeout);
            self.note_result(&relay, ok);
        }
        self.retain_requested(&chosen);
    }

    pub(super) fn allow_assignment(&self, now: Instant) -> bool {
        let mut dialed = self.dialed.lock().expect("relay circuit pass");
        dialed.retain(|at| now.saturating_duration_since(*at) < ASSIGN_WINDOW);
        if dialed.len() >= ASSIGN_LIMIT {
            return false;
        }
        dialed.push(now);
        true
    }

    fn retain_requested(&self, relays: &[String]) {
        let keep: Vec<String> = relays
            .iter()
            .filter_map(|relay| crate::tor_circuits::relay_key(relay))
            .collect();
        self.requested
            .lock()
            .expect("relay circuit pass")
            .retain(|key, _| keep.iter().any(|kept| kept == key));
    }

    fn noted(&self, relays: &[String], now: Instant) -> HashMap<String, (Duration, u32)> {
        let requested = self.requested.lock().expect("relay circuit pass");
        let mut noted = HashMap::new();
        for relay in relays {
            let Some(key) = crate::tor_circuits::relay_key(relay) else {
                continue;
            };
            let Some(attempt) = requested.get(&key) else {
                continue;
            };
            noted.insert(
                relay.clone(),
                (now.saturating_duration_since(attempt.at), attempt.misses),
            );
        }
        noted
    }

    pub(super) fn mark_requested(&self, relay: &str, at: Instant, misses: u32) {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return;
        };
        self.requested.lock().expect("relay circuit pass").insert(
            key,
            RelayAttempt {
                at,
                misses,
                ok: false,
            },
        );
    }

    fn opened_ok(&self, relay: &str) -> bool {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return false;
        };
        self.requested
            .lock()
            .expect("relay circuit pass")
            .get(&key)
            .is_some_and(|attempt| attempt.ok)
    }

    pub(super) fn note_result(&self, relay: &str, ok: bool) {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return;
        };
        if let Some(attempt) = self
            .requested
            .lock()
            .expect("relay circuit pass")
            .get_mut(&key)
        {
            attempt.ok = ok;
        }
    }

    #[cfg(test)]
    pub(super) fn settled(&self) -> bool {
        !self.running.load(Ordering::Acquire)
            && self.latest.lock().expect("relay circuit pass").is_none()
    }

    #[cfg(test)]
    pub(super) fn remembers(&self, relay: &str) -> bool {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return false;
        };
        self.requested
            .lock()
            .expect("relay circuit pass")
            .contains_key(&key)
    }

    #[cfg(test)]
    pub(super) fn misses(&self, relay: &str) -> Option<u32> {
        let key = crate::tor_circuits::relay_key(relay)?;
        self.requested
            .lock()
            .expect("relay circuit pass")
            .get(&key)
            .map(|attempt| attempt.misses)
    }
}

pub(super) fn shared_relay_pass() -> &'static Arc<RelayPass> {
    static PASS: OnceLock<Arc<RelayPass>> = OnceLock::new();
    PASS.get_or_init(|| Arc::new(RelayPass::new()))
}

pub(crate) fn schedule_relay_assignments(
    page: Vec<String>,
    configured: Vec<String>,
    timeout: Duration,
) {
    #[cfg(test)]
    if let Some(state) = watched_assignment() {
        let chosen = crate::relay_set::relays_to_assign(&page, &configured);
        *state.chosen.lock().expect("relay assignment watch") = chosen;
        let open: OpenRelay = Arc::new(|_: &str, _: Duration| true);
        state.pass.apply(&PendingAssignment {
            page,
            configured,
            timeout,
            open,
            circuits: Arc::new(|| None),
        });
        return;
    }
    schedule_relay_assignments_via(page, configured, timeout, relay_socks());
}

#[cfg(test)]
struct AssignmentWatchState {
    pass: Arc<RelayPass>,
    chosen: Mutex<Vec<String>>,
}

#[cfg(test)]
thread_local! {
    static ASSIGNMENT_WATCH: std::cell::RefCell<Option<Arc<AssignmentWatchState>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn watched_assignment() -> Option<Arc<AssignmentWatchState>> {
    ASSIGNMENT_WATCH.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
pub(crate) struct RelayAssignmentWatch {
    state: Arc<AssignmentWatchState>,
}

#[cfg(test)]
impl RelayAssignmentWatch {
    pub(crate) fn begin(seed: &[&str]) -> Self {
        let pass = Arc::new(RelayPass::new());
        for relay in seed {
            pass.mark_requested(relay, Instant::now(), 1);
        }
        let state = Arc::new(AssignmentWatchState {
            pass,
            chosen: Mutex::new(Vec::new()),
        });
        ASSIGNMENT_WATCH.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&state)));
        Self { state }
    }

    pub(crate) fn remembers(&self, relay: &str) -> bool {
        self.state.pass.remembers(relay)
    }

    pub(crate) fn assigned(&self) -> Vec<String> {
        self.state
            .chosen
            .lock()
            .expect("relay assignment watch")
            .clone()
    }
}

#[cfg(test)]
struct RelayAnswerBook {
    notes: Mutex<HashMap<String, String>>,
    asked: Mutex<Vec<String>>,
}

#[cfg(test)]
static RELAY_ANSWERS: Mutex<Option<Arc<RelayAnswerBook>>> = Mutex::new(None);

#[cfg(test)]
pub(super) fn scripted_relay_answer(relay: &str) -> Option<Option<String>> {
    let book = RELAY_ANSWERS
        .lock()
        .expect("relay answers")
        .as_ref()?
        .clone();
    book.asked
        .lock()
        .expect("relay reads")
        .push(relay.to_string());
    let notes = book.notes.lock().expect("relay answer notes");
    let found = notes
        .iter()
        .find(|(have, _)| same_public_relay(have, relay))
        .map(|(_, note)| note.clone());
    Some(found)
}

#[cfg(test)]
pub(crate) struct RelayAnswer {
    book: Arc<RelayAnswerBook>,
}

#[cfg(test)]
struct PublishScriptBook {
    refused: Mutex<Vec<(String, String)>>,
    accepted: Mutex<Vec<String>>,
}

#[cfg(test)]
static PUBLISH_SCRIPTS: Mutex<Option<Arc<PublishScriptBook>>> = Mutex::new(None);

#[cfg(test)]
pub(super) fn scripted_send(relay: &str) -> Option<Result<(), SendFailure>> {
    let book = PUBLISH_SCRIPTS
        .lock()
        .expect("publish scripts")
        .as_ref()?
        .clone();
    let refused = book.refused.lock().expect("publish refusals");
    if let Some((_, reason)) = refused
        .iter()
        .find(|(have, _)| same_public_relay(have, relay))
    {
        return Some(Err(SendFailure::Refused(reason.clone())));
    }
    drop(refused);
    let accepted = book.accepted.lock().expect("publish accepts");
    if accepted.iter().any(|have| same_public_relay(have, relay)) {
        return Some(Ok(()));
    }
    None
}

#[cfg(test)]
pub(crate) struct PublishScript {
    book: Arc<PublishScriptBook>,
}

#[cfg(test)]
impl PublishScript {
    pub(crate) fn begin() -> Self {
        let book = Arc::new(PublishScriptBook {
            refused: Mutex::new(Vec::new()),
            accepted: Mutex::new(Vec::new()),
        });
        *PUBLISH_SCRIPTS.lock().expect("publish scripts") = Some(Arc::clone(&book));
        Self { book }
    }

    pub(crate) fn refuse(&self, relay: &str) {
        self.push_refusal(relay, "blocked: relay rejected the note");
    }

    pub(crate) fn limit(&self, relay: &str) {
        self.push_refusal(relay, "rate-limited: slow down");
    }

    fn push_refusal(&self, relay: &str, reason: &str) {
        let Some(url) = canonical_public_relay(relay) else {
            return;
        };
        self.book
            .refused
            .lock()
            .expect("publish refusals")
            .push((url, reason.to_string()));
    }

    pub(crate) fn accept(&self, relay: &str) {
        let Some(url) = canonical_public_relay(relay) else {
            return;
        };
        self.book
            .accepted
            .lock()
            .expect("publish accepts")
            .push(url);
    }
}

#[cfg(test)]
impl Drop for PublishScript {
    fn drop(&mut self) {
        let mut slot = PUBLISH_SCRIPTS.lock().expect("publish scripts");
        let clear = slot
            .as_ref()
            .is_some_and(|book| Arc::ptr_eq(book, &self.book));
        if clear {
            *slot = None;
        }
    }
}

#[cfg(test)]
impl RelayAnswer {
    pub(crate) fn begin() -> Self {
        let book = Arc::new(RelayAnswerBook {
            notes: Mutex::new(HashMap::new()),
            asked: Mutex::new(Vec::new()),
        });
        *RELAY_ANSWERS.lock().expect("relay answers") = Some(Arc::clone(&book));
        Self { book }
    }

    pub(crate) fn answer(&self, relay: &str, note: &str) {
        let Some(url) = canonical_public_relay(relay) else {
            return;
        };
        self.book
            .notes
            .lock()
            .expect("relay answer notes")
            .insert(url, note.to_string());
    }

    pub(crate) fn asked(&self, relay: &str) -> bool {
        self.book
            .asked
            .lock()
            .expect("relay reads")
            .iter()
            .any(|have| same_public_relay(have, relay))
    }
}

#[cfg(test)]
impl Drop for RelayAnswer {
    fn drop(&mut self) {
        let mut slot = RELAY_ANSWERS.lock().expect("relay answers");
        let clear = slot
            .as_ref()
            .is_some_and(|book| Arc::ptr_eq(book, &self.book));
        if clear {
            *slot = None;
        }
    }
}

#[cfg(test)]
impl Drop for RelayAssignmentWatch {
    fn drop(&mut self) {
        ASSIGNMENT_WATCH.with(|slot| {
            let clear = slot
                .borrow()
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(state, &self.state));
            if clear {
                *slot.borrow_mut() = None;
            }
        });
    }
}

pub(super) fn schedule_relay_assignments_via(
    page: Vec<String>,
    configured: Vec<String>,
    timeout: Duration,
    socks: RelaySocks,
) {
    if !matches!(socks, RelaySocks::Proxy(_)) {
        return;
    }
    let open: OpenRelay = Arc::new(move |relay: &str, timeout: Duration| {
        let deadline = Instant::now() + timeout;
        match assign_relay(relay, deadline, socks) {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "Nonograph: relay {} was not assigned a circuit: {}",
                    super::log_text(relay),
                    super::log_text(&error)
                );
                false
            }
        }
    });
    // Keys are relays with a parked handshake. Values stay empty: hop nicknames
    // are not available once the control port is gone.
    let circuits: ReadCircuits = Arc::new(|| Some(super::transport::warm_relay_map()));
    shared_relay_pass().submit(page, configured, timeout, open, circuits);
}
