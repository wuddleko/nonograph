use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
use bech32::{Bech32, Hrp};
use rand::{thread_rng, Rng};
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, XOnlyPublicKey, SECP256K1};

mod assign;
mod nip11;
mod transport;

use transport::{
    arm_stream, connect_relay, open_relay_socket, read_incoming, wait_budget, Incoming,
    RELAY_TIMEOUT,
};
#[cfg(test)]
use transport::{
    build_tls_connector, held_keys_to_drop, resolve_host, store_held, take_held, tls_handshake,
    use_invalid_test_certs, HELD_RELAY_LIMIT, SOCKS_MISCONFIGURED,
};

pub(crate) use assign::schedule_relay_assignments;
#[cfg(test)]
use assign::{
    assign_relay, assignment_due, next_misses, relay_needs_assignment, relays_needing_assignment,
    schedule_relay_assignments_via, scripted_relay_answer, scripted_send, shared_relay_pass,
    OpenRelay, ReadCircuits, RelayPass, ASSIGN_LIMIT, ASSIGN_WINDOW,
};
#[cfg(test)]
pub(crate) use assign::{PublishScript, RelayAnswer, RelayAssignmentWatch};

pub(crate) use nip11::fetch_nip11;
#[cfg(test)]
use nip11::{decode_http_message, NIP11_BODY_MAX};

pub(crate) const MAX_FETCH_RELAYS: usize = 6;
const SEALED_EVENT_OVERHEAD: usize = 8192;
const SEALED_FRAME_OVERHEAD: usize = 1024;

pub struct SignedNote {
    pub id: [u8; 32],
    pub pubkey: [u8; 32],
    pub event_json: String,
}

impl SignedNote {
    pub fn id_hex(&self) -> String {
        hex_encode(&self.id)
    }
}

#[cfg(test)]
pub fn sign_sealed_note(content: &str, locator: &str, created_at: i64) -> SignedNote {
    let keypair = Keypair::new(SECP256K1, &mut thread_rng());
    let pubkey_hex = hex_encode(&keypair.x_only_public_key().0.serialize());
    let mined = crate::seal::mine_stamp(
        &pubkey_hex,
        created_at,
        locator,
        content,
        crate::seal::STAMP_ATTEMPTS,
    )
    .expect("no stamp in range");
    signed_event(
        &keypair,
        mined.created_at,
        nonograph_nip44::KIND,
        &mined.tags,
        content,
    )
}

#[derive(Clone, Copy)]
pub enum RelaySocks {
    Direct,
    Proxy(SocketAddr),
    Disabled,
}

static RELAY_SOCKS: Mutex<RelaySocks> = Mutex::new(RelaySocks::Direct);

pub fn set_relay_socks(setting: RelaySocks) {
    *RELAY_SOCKS.lock().expect("relay socks lock") = setting;
}

fn relay_socks() -> RelaySocks {
    *RELAY_SOCKS.lock().expect("relay socks lock")
}

pub(crate) fn relays_use_proxy() -> bool {
    matches!(relay_socks(), RelaySocks::Proxy(_))
}

pub fn sealed_note_from_json(
    value: &serde_json::Value,
    title_max: usize,
    alias_max: usize,
    content_max: usize,
) -> Option<(SignedNote, String)> {
    sealed_note_capped(
        value,
        crate::seal::sealed_payload_max(title_max, alias_max, content_max),
    )
}

fn sealed_note_capped(
    value: &serde_json::Value,
    payload_max: usize,
) -> Option<(SignedNote, String)> {
    let parsed = parse_event(value)?;
    if !verify_sig(&parsed) {
        return None;
    }
    if nonograph_nip44::check_stamp(
        &parsed.id,
        &parsed.pubkey_hex,
        parsed.created_at,
        parsed.kind,
        &parsed.tags,
        &parsed.content,
    )
    .is_err()
    {
        return None;
    }
    if parsed.content.len() > payload_max {
        return None;
    }
    let locator = stamp_locator(&parsed.tags)?;
    let sig = parsed.sig?;
    Some((
        SignedNote {
            id: parsed.id,
            pubkey: parsed.pubkey,
            event_json: event_wire(
                &hex_encode(&parsed.id),
                &parsed.pubkey_hex,
                parsed.created_at,
                parsed.kind,
                &parsed.tags,
                &parsed.content,
                Some(&hex_encode(&sig)),
            ),
        },
        locator,
    ))
}

fn stamp_locator(tags: &[Vec<String>]) -> Option<String> {
    tags.iter().find_map(|tag| match tag.as_slice() {
        [name, locator] if name == "d" => Some(locator.clone()),
        _ => None,
    })
}

pub struct PublishedRelays {
    pub accepted: Vec<String>,
    pub refused: Vec<String>,
}

pub fn publish_to_relays(
    relays: &[String],
    note: &SignedNote,
    timeout: Duration,
) -> PublishedRelays {
    publish_to_relays_inner(relays, note, timeout, true)
}

enum SendFailure {
    Refused(String),
    Unanswered(String),
}

impl SendFailure {
    fn detail(self) -> String {
        match self {
            SendFailure::Refused(reason) | SendFailure::Unanswered(reason) => reason,
        }
    }
}

fn publish_to_relays_inner(
    relays: &[String],
    note: &SignedNote,
    timeout: Duration,
    public_only: bool,
) -> PublishedRelays {
    let relays: Vec<String> = relays
        .iter()
        .filter(|relay| {
            if public_only {
                public_relay_url(relay)
            } else {
                crawler::valid_relay_url(relay)
            }
        })
        .cloned()
        .collect();
    if relays.is_empty() {
        return PublishedRelays {
            accepted: Vec::new(),
            refused: Vec::new(),
        };
    }

    let deadline = Instant::now() + timeout;
    let event_id = note.id_hex();
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(relays.len());
        for relay in &relays {
            let relay = relay.clone();
            let event_json = note.event_json.clone();
            let event_id = event_id.clone();
            handles.push(scope.spawn(move || {
                match send_event(&relay, &event_json, &event_id, deadline, public_only) {
                    Ok(()) => Some(Ok(relay)),
                    Err(SendFailure::Refused(error)) => {
                        eprintln!("Nonograph: relay {relay} did not accept the note: {error}");
                        if durable_refusal(&error) {
                            Some(Err(relay))
                        } else {
                            None
                        }
                    }
                    Err(SendFailure::Unanswered(error)) => {
                        eprintln!("Nonograph: relay {relay} did not accept the note: {error}");
                        None
                    }
                }
            }));
        }
        let mut accepted = Vec::new();
        let mut refused = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Some(Ok(relay))) => accepted.push(relay),
                Ok(Some(Err(relay))) => refused.push(relay),
                _ => {}
            }
        }
        PublishedRelays { accepted, refused }
    })
}

fn parse_ok(message: &str, event_id_hex: &str) -> Option<Result<(), String>> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    let items = value.as_array()?;
    if items.first()?.as_str()? != "OK" {
        return None;
    }
    if items.get(1)?.as_str()? != event_id_hex {
        return None;
    }
    if items.get(2)?.as_bool()? {
        Some(Ok(()))
    } else {
        let reason = items.get(3).and_then(|item| item.as_str()).unwrap_or("");
        Some(Err(reason.to_string()))
    }
}

fn durable_refusal(reason: &str) -> bool {
    reason.starts_with("blocked:") || reason.starts_with("restricted:")
}

fn send_event(
    relay: &str,
    event_json: &str,
    event_id_hex: &str,
    deadline: Instant,
    public_only: bool,
) -> Result<(), SendFailure> {
    #[cfg(test)]
    if let Some(result) = scripted_send(relay) {
        return result;
    }
    let mut socket = open_relay_socket(relay, deadline, public_only, relay_socks())
        .map_err(SendFailure::Unanswered)?;
    let payload = format!("[\"EVENT\",{event_json}]");
    arm_stream(
        socket.get_ref().get_ref(),
        wait_budget(deadline, None).map_err(SendFailure::Unanswered)?,
    )
    .map_err(SendFailure::Unanswered)?;
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| SendFailure::Unanswered(error.to_string()))?;

    loop {
        match read_incoming(&mut socket, deadline, None).map_err(SendFailure::Unanswered)? {
            Incoming::Closed => {
                return Err(SendFailure::Unanswered(
                    "relay closed the connection".to_string(),
                ))
            }
            Incoming::Text(text) => {
                if let Some(result) = parse_ok(&text, event_id_hex) {
                    return result.map_err(|reason| {
                        let reason = if reason.is_empty() {
                            "relay rejected the note".to_string()
                        } else {
                            reason
                        };
                        SendFailure::Refused(reason)
                    });
                }
            }
        }
    }
}

fn minimal_probe_note(created_at: i64) -> SignedNote {
    let keypair = Keypair::new(SECP256K1, &mut thread_rng());
    signed_event(&keypair, created_at, nonograph_nip44::KIND, &[], "")
}

fn probe_result(sent: Result<(), String>) -> crawler::ProbeResult {
    match sent {
        Ok(()) => crawler::ProbeResult {
            name: nonograph_nip44::KIND.to_string(),
            accepted: true,
            detail: "OK".to_owned(),
        },
        Err(reason) => crawler::ProbeResult {
            name: nonograph_nip44::KIND.to_string(),
            accepted: false,
            detail: reason,
        },
    }
}

#[cfg(test)]
fn probe_from_ok(message: &str, event_id_hex: &str) -> Option<crawler::ProbeResult> {
    let sent = parse_ok(message, event_id_hex)?.map_err(|reason| {
        if reason.is_empty() {
            "relay rejected the note".to_string()
        } else {
            reason
        }
    });
    Some(probe_result(sent))
}

pub(crate) fn probe_kind(
    relay: &str,
    document: Option<&serde_json::Value>,
    deadline: Instant,
) -> Vec<crawler::ProbeResult> {
    if !document.is_some_and(crate::nip11::document_is_public) {
        return Vec::new();
    }
    let note = minimal_probe_note(now_secs());
    vec![probe_result(
        send_event(relay, &note.event_json, &note.id_hex(), deadline, true)
            .map_err(|failure| failure.detail()),
    )]
}

pub fn fetch_sealed(
    relays: &[String],
    event_id_hex: &str,
    locator: &str,
    title_max: usize,
    alias_max: usize,
    content_max: usize,
    timeout: Duration,
) -> Option<String> {
    let payload_max = crate::seal::sealed_payload_max(title_max, alias_max, content_max);
    Some(fetch_from_relays(
        relays,
        event_id_hex,
        locator,
        payload_max,
        timeout,
    )?)
}

fn fetch_from_relays(
    relays: &[String],
    event_id_hex: &str,
    locator: &str,
    payload_max: usize,
    timeout: Duration,
) -> Option<String> {
    if decode_fixed_hex::<32>(event_id_hex).is_none() || !sealed_locator(locator) {
        return None;
    }
    let relays: Vec<String> = relays
        .iter()
        .filter(|relay| public_relay_url(relay))
        .take(MAX_FETCH_RELAYS)
        .cloned()
        .collect();
    if relays.is_empty() {
        return None;
    }

    let deadline = Instant::now() + timeout;
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(relays.len());
    for relay in relays {
        let tx = tx.clone();
        let event_id_hex = event_id_hex.to_string();
        let locator = locator.to_string();
        let cancel = Arc::clone(&cancel);
        handles.push(std::thread::spawn(move || {
            let found = match fetch_from_relay(
                &relay,
                &event_id_hex,
                &locator,
                payload_max,
                deadline,
                &cancel,
            ) {
                Ok(note) => note,
                Err(error) => {
                    if error != RELAY_TIMEOUT {
                        eprintln!("Nonograph: relay {relay} did not return the note: {error}");
                    }
                    None
                }
            };
            let _ = tx.send(found);
        }));
    }
    drop(tx);
    let note = take_first_note(rx, deadline);
    cancel.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.join();
    }
    note
}

fn take_first_note(rx: mpsc::Receiver<Option<String>>, deadline: Instant) -> Option<String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match rx.recv_timeout(remaining) {
            Ok(Some(note)) => return Some(note),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
}

pub(crate) struct RelayList {
    pub socket_answered: bool,
    pub urls: Vec<String>,
}

struct ListRead {
    socket_answered: bool,
    urls: Vec<String>,
    finished: bool,
}

impl ListRead {
    fn unanswered() -> Self {
        Self {
            socket_answered: false,
            urls: Vec::new(),
            finished: true,
        }
    }

    fn answered() -> Self {
        Self {
            socket_answered: true,
            urls: Vec::new(),
            finished: false,
        }
    }

    fn push(&mut self, sub_id: &str, frame: &str) -> bool {
        if self.finished {
            return true;
        }
        if let Some(event) = event_for_sub(frame, sub_id) {
            for url in crawler::list_urls(&event) {
                if !self.urls.iter().any(|have| have == &url) {
                    self.urls.push(url);
                }
            }
            return false;
        }
        if relay_has_no_event(frame, sub_id) {
            self.finished = true;
        }
        self.finished
    }

    fn finish(self) -> RelayList {
        RelayList {
            socket_answered: self.socket_answered,
            urls: self.urls,
        }
    }
}

fn list_req(sub_id: &str) -> String {
    serde_json::json!(["REQ", sub_id, { "kinds": [10002], "limit": 50 }]).to_string()
}

fn event_for_sub(frame: &str, sub_id: &str) -> Option<String> {
    let (kind, sub, event): (String, String, Box<serde_json::value::RawValue>) =
        serde_json::from_str(frame).ok()?;
    if kind != "EVENT" || sub != sub_id {
        return None;
    }
    Some(event.get().to_string())
}

pub(crate) fn read_relay_list(relay: &str, deadline: Instant, socks: RelaySocks) -> RelayList {
    let mut socket = match connect_relay(relay, deadline, None, true, socks) {
        Ok(socket) => socket,
        Err(_) => return ListRead::unanswered().finish(),
    };
    // Kind 10002 lists are larger than a sealed note, so leave the default frame cap.
    let mut read = ListRead::answered();
    let sub_id = random_hex(8);
    let payload = list_req(&sub_id);
    let Ok(wait) = wait_budget(deadline, None) else {
        return read.finish();
    };
    if arm_stream(socket.get_ref().get_ref(), wait).is_err()
        || socket
            .send(tungstenite::Message::Text(payload.into()))
            .is_err()
    {
        return read.finish();
    }
    loop {
        match read_incoming(&mut socket, deadline, None) {
            Ok(Incoming::Text(frame)) => {
                if read.push(&sub_id, &frame) {
                    return read.finish();
                }
            }
            Ok(Incoming::Closed) | Err(_) => return read.finish(),
        }
    }
}

fn req_payload(sub_id: &str, event_id_hex: &str) -> String {
    serde_json::json!([
        "REQ",
        sub_id,
        {
            "ids": [event_id_hex],
        }
    ])
    .to_string()
}

fn fetch_from_relay(
    relay: &str,
    event_id_hex: &str,
    locator: &str,
    payload_max: usize,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Option<String>, String> {
    #[cfg(test)]
    if let Some(answer) = scripted_relay_answer(relay) {
        return Ok(answer);
    }
    let mut socket = connect_relay(relay, deadline, Some(cancel), true, relay_socks())?;
    limit_sealed_socket(&mut socket, payload_max);
    let sub_id = random_hex(8);
    let payload = req_payload(&sub_id, event_id_hex);
    arm_stream(
        socket.get_ref().get_ref(),
        wait_budget(deadline, Some(cancel))?,
    )?;
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| error.to_string())?;

    loop {
        let incoming = match read_incoming(&mut socket, deadline, Some(cancel)) {
            Ok(incoming) => incoming,
            Err(error) => return Err(error),
        };
        match incoming {
            Incoming::Closed => {
                return Err("relay closed the connection".to_string());
            }
            Incoming::Text(text) => {
                if text.len() > sealed_frame_max(payload_max) {
                    continue;
                }
                if let Some(event_json) = note_for_query(&text, event_id_hex, locator, payload_max)
                {
                    return Ok(Some(event_json));
                }
                if relay_has_no_event(&text, &sub_id) {
                    return Ok(None);
                }
            }
        }
    }
}

fn note_for_query(
    message: &str,
    event_id_hex: &str,
    locator: &str,
    payload_max: usize,
) -> Option<String> {
    sealed_from_relay_message(message, event_id_hex, locator, payload_max)
        .map(|(_created_at, event_json)| event_json)
}

fn relay_has_no_event(message: &str, sub_id: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(message) else {
        return false;
    };
    let Some(items) = value.as_array() else {
        return false;
    };
    let Some(kind) = items.first().and_then(|item| item.as_str()) else {
        return false;
    };
    if kind != "EOSE" && kind != "CLOSED" {
        return false;
    }
    items.get(1).and_then(|item| item.as_str()) == Some(sub_id)
}

fn sealed_from_relay_message(
    message: &str,
    event_id: &str,
    locator: &str,
    payload_max: usize,
) -> Option<(i64, String)> {
    if message.len() > sealed_frame_max(payload_max) {
        return None;
    }
    let event_json = unchanged_relay_event(message)?;
    if event_json.len() > sealed_event_max(payload_max) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&event_json).ok()?;
    if value.get("id")?.as_str()? != event_id {
        return None;
    }
    let (_, found) = sealed_note_capped(&value, payload_max)?;
    if found != locator {
        return None;
    }
    Some((value.get("created_at")?.as_i64()?, event_json))
}

fn unchanged_relay_event(message: &str) -> Option<String> {
    let (kind, _sub, event): (String, String, Box<serde_json::value::RawValue>) =
        serde_json::from_str(message).ok()?;
    if kind != "EVENT" {
        return None;
    }
    Some(event.get().to_string())
}

fn sealed_event_max(content_max: usize) -> usize {
    content_max.saturating_add(SEALED_EVENT_OVERHEAD)
}

fn sealed_frame_max(content_max: usize) -> usize {
    sealed_event_max(content_max).saturating_add(SEALED_FRAME_OVERHEAD)
}

fn limit_sealed_socket<Stream>(socket: &mut tungstenite::WebSocket<Stream>, content_max: usize) {
    let max = sealed_frame_max(content_max);
    socket.set_config(|config| {
        config.max_message_size = Some(max);
        config.max_frame_size = Some(max);
    });
}

pub(crate) fn sealed_locator(locator: &str) -> bool {
    locator.len() == nonograph_nip44::LOCATOR_LEN
        && locator
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn sealed_event_id(event_id: &str) -> bool {
    event_id.len() == 64
        && event_id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn decode_fixed_hex<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (index, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let text = std::str::from_utf8(chunk).ok()?;
        out[index] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(out)
}

pub(crate) fn canonical_public_relay(relay: &str) -> Option<String> {
    crawler::canonical_public_relay(relay)
}

pub(crate) fn same_public_relay(left: &str, right: &str) -> bool {
    matches!(
        (canonical_public_relay(left), canonical_public_relay(right)),
        (Some(left), Some(right)) if left == right
    )
}

pub(crate) fn public_relay_url(relay: &str) -> bool {
    canonical_public_relay(relay).is_some()
}

fn public_relay_addresses(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let public: Vec<SocketAddr> = addresses
        .into_iter()
        .filter(|address| address.port() == 443 && !crawler::blocked_relay_ip(address.ip()))
        .collect();
    if public.is_empty() {
        Err("relay address is not public".to_string())
    } else {
        Ok(public)
    }
}

pub(crate) fn signed_event(
    keypair: &Keypair,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> SignedNote {
    let pubkey = keypair.x_only_public_key().0.serialize();
    let pubkey_hex = hex_encode(&pubkey);
    let id = nonograph_nip44::event_id(&pubkey_hex, created_at, kind, tags, content);
    let sig = SECP256K1.sign_schnorr(&Message::from_digest(id), keypair);
    let event_json = event_wire(
        &hex_encode(&id),
        &pubkey_hex,
        created_at,
        kind,
        tags,
        content,
        Some(&hex_encode(&sig.serialize())),
    );
    SignedNote {
        id,
        pubkey,
        event_json,
    }
}

fn event_wire(
    id_hex: &str,
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
    sig_hex: Option<&str>,
) -> String {
    let mut out = String::from("{\"id\":");
    nonograph_nip44::push_json_string(&mut out, id_hex);
    out.push_str(",\"pubkey\":");
    nonograph_nip44::push_json_string(&mut out, pubkey_hex);
    out.push_str(",\"created_at\":");
    out.push_str(&created_at.to_string());
    out.push_str(",\"kind\":");
    out.push_str(&kind.to_string());
    out.push_str(",\"tags\":");
    nonograph_nip44::push_tags(&mut out, tags);
    out.push_str(",\"content\":");
    nonograph_nip44::push_json_string(&mut out, content);
    if let Some(sig_hex) = sig_hex {
        out.push_str(",\"sig\":");
        nonograph_nip44::push_json_string(&mut out, sig_hex);
    }
    out.push('}');
    out
}

struct ParsedEvent {
    id: [u8; 32],
    pubkey: [u8; 32],
    pubkey_hex: String,
    created_at: i64,
    kind: u32,
    tags: Vec<Vec<String>>,
    content: String,
    sig: Option<[u8; 64]>,
}

fn parse_event(value: &serde_json::Value) -> Option<ParsedEvent> {
    let id_hex = value.get("id")?.as_str()?;
    let pubkey_hex = value.get("pubkey")?.as_str()?.to_string();
    let created_at = value.get("created_at")?.as_i64()?;
    let kind = json_kind(value)?;
    let content = json_content(value)?.to_string();
    let tags = event_tags(value)?;
    let id = decode_fixed_hex::<32>(id_hex)?;
    let pubkey = decode_fixed_hex::<32>(&pubkey_hex)?;
    let sig = match value.get("sig") {
        Some(item) => Some(decode_fixed_hex::<64>(item.as_str()?)?),
        None => None,
    };
    Some(ParsedEvent {
        id,
        pubkey,
        pubkey_hex,
        created_at,
        kind,
        tags,
        content,
        sig,
    })
}

fn json_kind(value: &serde_json::Value) -> Option<u32> {
    value.get("kind")?.as_u64()?.try_into().ok()
}

fn json_content(value: &serde_json::Value) -> Option<&str> {
    value.get("content")?.as_str()
}

fn event_tags(value: &serde_json::Value) -> Option<Vec<Vec<String>>> {
    value
        .get("tags")?
        .as_array()?
        .iter()
        .map(|tag| {
            tag.as_array()?
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
        })
        .collect()
}

fn verify_sig(parsed: &ParsedEvent) -> bool {
    let Some(sig) = parsed.sig else {
        return false;
    };
    let Ok(xonly) = XOnlyPublicKey::from_slice(&parsed.pubkey) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&sig) else {
        return false;
    };
    SECP256K1
        .verify_schnorr(&signature, &Message::from_digest(parsed.id), &xonly)
        .is_ok()
}

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    let mut rng = thread_rng();
    let raw: Vec<u8> = (0..bytes).map(|_| rng.gen()).collect();
    hex_encode(&raw)
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
#[path = "../../test/nostr.rs"]
mod tests;
