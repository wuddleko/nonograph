use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use socket2::{Domain, SockAddr, Socket, Type};

#[cfg(test)]
use bech32::{Bech32, Hrp};
use rand::{thread_rng, Rng};
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, XOnlyPublicKey, SECP256K1};
use tungstenite::client::IntoClientRequest;

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

type RelaySocket = tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>;
type HeldTls = native_tls::TlsStream<TcpStream>;

// A parked handshake is one socket. Relays close an idle one after about a minute,
// and a visitor can name a new set of relays faster than that.
const HELD_RELAY_LIMIT: usize = 16;
const HELD_RELAY_TTL: Duration = Duration::from_secs(45);

struct HeldRelay {
    at: Instant,
    tls: HeldTls,
}

fn held_relays() -> &'static Mutex<HashMap<String, HeldRelay>> {
    static HELD: OnceLock<Mutex<HashMap<String, HeldRelay>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

fn take_held(relay: &str) -> Option<HeldTls> {
    let key = crate::tor_circuits::relay_key(relay)?;
    let mut held = held_relays().lock().expect("held relay");
    let entry = held.remove(&key)?;
    if entry.at.elapsed() >= HELD_RELAY_TTL {
        drop(held);
        drop(entry);
        return None;
    }
    drop(held);
    Some(entry.tls)
}

fn store_held(relay: &str, tls: HeldTls) {
    let Some(key) = crate::tor_circuits::relay_key(relay) else {
        return;
    };
    let dropped = {
        let mut held = held_relays().lock().expect("held relay");
        let now = Instant::now();
        let replaced = held.insert(key, HeldRelay { at: now, tls });
        let mut dropped = reap_held(&mut held, now);
        if let Some(previous) = replaced {
            dropped.push(previous);
        }
        dropped
    };
    drop(dropped);
}

fn reap_held_relays() {
    let dropped = {
        let mut held = held_relays().lock().expect("held relay");
        reap_held(&mut held, Instant::now())
    };
    drop(dropped);
}

fn reap_held(held: &mut HashMap<String, HeldRelay>, now: Instant) -> Vec<HeldRelay> {
    let present: Vec<(String, Instant)> = held
        .iter()
        .map(|(key, entry)| (key.clone(), entry.at))
        .collect();
    let mut dropped = Vec::new();
    for key in held_keys_to_drop(&present, now, HELD_RELAY_TTL, HELD_RELAY_LIMIT) {
        if let Some(entry) = held.remove(&key) {
            dropped.push(entry);
        }
    }
    dropped
}

fn held_keys_to_drop(
    entries: &[(String, Instant)],
    now: Instant,
    ttl: Duration,
    limit: usize,
) -> Vec<String> {
    let mut live = Vec::new();
    let mut drop_keys = Vec::new();
    for (key, at) in entries {
        if now.saturating_duration_since(*at) >= ttl {
            drop_keys.push(key.clone());
        } else {
            live.push((key.clone(), *at));
        }
    }
    if live.len() > limit {
        live.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        let extra = live.len() - limit;
        drop_keys.extend(live.into_iter().take(extra).map(|(key, _)| key));
    }
    drop_keys
}

const RELAY_TIMEOUT: &str = "timed out waiting for the relay";
const RELAY_FRAME_TOO_BIG: &str = "relay frame exceeded the sealed size cap";
const SOCKS_MISCONFIGURED: &str = "socks proxy is misconfigured";
const FETCH_POLL: Duration = Duration::from_millis(100);

fn tls_connector() -> Result<&'static native_tls::TlsConnector, String> {
    #[cfg(test)]
    if let Some(connector) = test_tls_connector() {
        return Ok(connector);
    }
    static CONNECTOR: OnceLock<Result<native_tls::TlsConnector, String>> = OnceLock::new();
    match CONNECTOR.get_or_init(build_tls_connector) {
        Ok(connector) => Ok(connector),
        Err(error) => Err(error.clone()),
    }
}

fn build_tls_connector() -> Result<native_tls::TlsConnector, String> {
    native_tls::TlsConnector::new().map_err(|error| error.to_string())
}

#[cfg(test)]
fn test_tls_slot() -> &'static OnceLock<native_tls::TlsConnector> {
    static SLOT: OnceLock<native_tls::TlsConnector> = OnceLock::new();
    &SLOT
}

#[cfg(test)]
fn test_tls_connector() -> Option<&'static native_tls::TlsConnector> {
    test_tls_slot().get()
}

#[cfg(test)]
fn use_invalid_test_certs() {
    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .expect("test tls connector");
    let _ = test_tls_slot().set(connector);
}

fn relay_wait_is_over(deadline: Instant, cancel: Option<&AtomicBool>) -> bool {
    Instant::now() >= deadline || cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn wait_budget(deadline: Instant, cancel: Option<&AtomicBool>) -> Result<Duration, String> {
    if relay_wait_is_over(deadline, cancel) {
        return Err(RELAY_TIMEOUT.to_string());
    }
    let wait = deadline.saturating_duration_since(Instant::now());
    if wait.is_zero() {
        Err(RELAY_TIMEOUT.to_string())
    } else {
        Ok(wait)
    }
}

fn io_slice(deadline: Instant, cancel: Option<&AtomicBool>) -> Result<Duration, String> {
    let wait = wait_budget(deadline, cancel)?;
    Ok(if cancel.is_some() {
        wait.min(FETCH_POLL)
    } else {
        wait
    })
}

fn arm_stream(stream: &TcpStream, wait: Duration) -> Result<(), String> {
    stream
        .set_read_timeout(Some(wait))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(wait))
        .map_err(|error| error.to_string())?;
    Ok(())
}

struct Ready {
    readable: bool,
    writable: bool,
    failed: bool,
}

impl Ready {
    fn idle() -> Self {
        Self {
            readable: false,
            writable: false,
            failed: false,
        }
    }

    fn waiting(&self) -> bool {
        !self.readable && !self.writable && !self.failed
    }
}

trait PollReady {
    fn poll_ready(&self, wait: Duration, read: bool, write: bool) -> Result<Ready, String>;
}

#[cfg(unix)]
impl<S: std::os::fd::AsRawFd> PollReady for S {
    fn poll_ready(&self, wait: Duration, read: bool, write: bool) -> Result<Ready, String> {
        let mut events = libc::POLLERR | libc::POLLHUP;
        if read {
            events |= libc::POLLIN;
        }
        if write {
            events |= libc::POLLOUT;
        }
        let mut poll_fd = libc::pollfd {
            fd: self.as_raw_fd(),
            events,
            revents: 0,
        };
        let millis = wait.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
        let ready = unsafe { libc::poll(&mut poll_fd, 1, millis) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(Ready::idle());
            }
            return Err(error.to_string());
        }
        if ready == 0 {
            return Ok(Ready::idle());
        }
        Ok(Ready {
            readable: poll_fd.revents & libc::POLLIN != 0,
            writable: poll_fd.revents & libc::POLLOUT != 0,
            failed: poll_fd.revents & (libc::POLLERR | libc::POLLHUP) != 0,
        })
    }
}

#[cfg(windows)]
impl<S: std::os::windows::io::AsRawSocket> PollReady for S {
    fn poll_ready(&self, wait: Duration, read: bool, write: bool) -> Result<Ready, String> {
        const POLLERR: i16 = 0x0001;
        const POLLHUP: i16 = 0x0002;
        const POLLNVAL: i16 = 0x0004;
        const POLLWRNORM: i16 = 0x0010;
        const POLLRDNORM: i16 = 0x0100;
        let mut events = POLLERR | POLLHUP | POLLNVAL;
        if read {
            events |= POLLRDNORM;
        }
        if write {
            events |= POLLWRNORM;
        }
        let mut poll_fd = WsaPollFd {
            fd: self.as_raw_socket(),
            events,
            revents: 0,
        };
        let millis = wait.as_millis().clamp(1, i32::MAX as u128) as i32;
        let ready = unsafe { WSAPoll(&mut poll_fd, 1, millis) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(Ready::idle());
            }
            return Err(error.to_string());
        }
        if ready == 0 {
            return Ok(Ready::idle());
        }
        Ok(Ready {
            readable: poll_fd.revents & POLLRDNORM != 0,
            writable: poll_fd.revents & POLLWRNORM != 0,
            failed: poll_fd.revents & (POLLERR | POLLHUP | POLLNVAL) != 0,
        })
    }
}

#[cfg(windows)]
#[repr(C)]
struct WsaPollFd {
    fd: std::os::windows::io::RawSocket,
    events: i16,
    revents: i16,
}

#[cfg(windows)]
#[link(name = "ws2_32")]
extern "system" {
    fn WSAPoll(fds: *mut WsaPollFd, nfds: u32, timeout: i32) -> i32;
}

fn wait_for_handshake(
    socket: &impl PollReady,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
    saw_immediate_write: &mut bool,
) -> Result<(), String> {
    let wait = io_slice(deadline, cancel)?;
    let started = Instant::now();
    let ready = socket.poll_ready(wait, true, !*saw_immediate_write)?;
    let immediate = started.elapsed() < Duration::from_millis(5);
    let write_only = ready.writable && !ready.readable && !ready.failed;
    *saw_immediate_write = immediate && write_only;
    Ok(())
}

fn set_blocking(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())
}

fn connection_pending(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
    ) {
        return true;
    }
    match error.raw_os_error() {
        #[cfg(unix)]
        Some(code)
            if code == libc::EINPROGRESS || code == libc::EALREADY || code == libc::EWOULDBLOCK =>
        {
            true
        }
        // WSAEWOULDBLOCK, WSAEINPROGRESS, WSAEALREADY
        #[cfg(windows)]
        Some(10035 | 10036 | 10037) => true,
        _ => false,
    }
}

fn already_connected(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        #[cfg(unix)]
        Some(code) if code == libc::EISCONN => true,
        #[cfg(windows)]
        Some(10056) => true,
        _ => false,
    }
}

fn connect_tcp(
    address: SocketAddr,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<TcpStream, String> {
    let socket = Socket::new(Domain::for_address(address), Type::STREAM, None)
        .map_err(|error| error.to_string())?;
    socket
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    match socket.connect(&SockAddr::from(address)) {
        Ok(()) => {}
        Err(error) if already_connected(&error) => {}
        Err(error) if connection_pending(&error) => loop {
            let wait = io_slice(deadline, cancel)?;
            let ready = socket.poll_ready(wait, false, true)?;
            if ready.waiting() {
                continue;
            }
            match socket.take_error().map_err(|error| error.to_string())? {
                Some(error) => return Err(error.to_string()),
                None => break,
            }
        },
        Err(error) => return Err(error.to_string()),
    }
    socket
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    Ok(TcpStream::from(socket))
}

fn tls_handshake(
    connector: &native_tls::TlsConnector,
    host: &str,
    tcp: TcpStream,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<native_tls::TlsStream<TcpStream>, String> {
    tcp.set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let mut pending = match connector.connect(host, tcp) {
        Ok(stream) => {
            set_blocking(stream.get_ref())?;
            return Ok(stream);
        }
        Err(native_tls::HandshakeError::WouldBlock(mid)) => mid,
        Err(native_tls::HandshakeError::Failure(error)) => return Err(error.to_string()),
    };
    let mut saw_immediate_write = false;
    loop {
        wait_for_handshake(
            pending.get_ref(),
            deadline,
            cancel,
            &mut saw_immediate_write,
        )?;
        match pending.handshake() {
            Ok(stream) => {
                set_blocking(stream.get_ref())?;
                return Ok(stream);
            }
            Err(native_tls::HandshakeError::WouldBlock(mid)) => pending = mid,
            Err(native_tls::HandshakeError::Failure(error)) => return Err(error.to_string()),
        }
    }
}

fn websocket_handshake(
    request: tungstenite::http::Request<()>,
    tls: native_tls::TlsStream<TcpStream>,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<RelaySocket, String> {
    tls.get_ref()
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let mut saw_immediate_write = false;
    let mut pending = match tungstenite::client::client(request, tls) {
        Ok((socket, _)) => {
            set_blocking(socket.get_ref().get_ref())?;
            return Ok(socket);
        }
        Err(tungstenite::HandshakeError::Interrupted(mid)) => mid,
        Err(tungstenite::HandshakeError::Failure(error)) => return Err(error.to_string()),
    };
    loop {
        wait_for_handshake(
            pending.get_ref().get_ref().get_ref(),
            deadline,
            cancel,
            &mut saw_immediate_write,
        )?;
        match pending.handshake() {
            Ok((socket, _)) => {
                set_blocking(socket.get_ref().get_ref())?;
                return Ok(socket);
            }
            Err(tungstenite::HandshakeError::Interrupted(mid)) => pending = mid,
            Err(tungstenite::HandshakeError::Failure(error)) => return Err(error.to_string()),
        }
    }
}

fn resolve_host(
    host: &str,
    port: u16,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<SocketAddr>, String> {
    wait_budget(deadline, cancel)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }

    let host = host.to_string();
    let (tx, rx) = mpsc::channel();
    // The system resolver cannot be interrupted. The relay thread returns at the
    // deadline, and this thread exits when that call returns.
    std::thread::spawn(move || {
        let resolved = (host.as_str(), port)
            .to_socket_addrs()
            .map(|addresses| addresses.collect::<Vec<_>>())
            .map_err(|error| error.to_string());
        let _ = tx.send(resolved);
    });

    loop {
        let wait = io_slice(deadline, cancel)?;
        match rx.recv_timeout(wait) {
            Ok(Ok(addresses)) if addresses.is_empty() => {
                return Err("relay address did not resolve".to_string());
            }
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("relay address did not resolve".to_string());
            }
        }
    }
}

fn with_relay_tls<T>(
    relay: &str,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
    public_only: bool,
    socks: RelaySocks,
    mut use_tls: impl FnMut(native_tls::TlsStream<TcpStream>) -> Result<T, String>,
) -> Result<T, String> {
    let proxy = match socks {
        RelaySocks::Direct => None,
        RelaySocks::Proxy(addr) => Some(addr),
        RelaySocks::Disabled => return Err(SOCKS_MISCONFIGURED.to_string()),
    };
    let request = relay
        .into_client_request()
        .map_err(|error| error.to_string())?;
    let uri = request.uri().clone();
    let host = uri.host().ok_or("relay url has no host")?.to_string();
    let port = uri.port_u16().unwrap_or(443);
    if public_only && !public_relay_url(relay) {
        return Err("relay address is not public".to_string());
    }
    wait_budget(deadline, cancel)?;
    let connector = tls_connector()?;
    if let Some(proxy) = proxy {
        let tcp = connect_tcp(proxy, deadline, cancel)?;
        socks5_connect(&tcp, &host, port, relay, deadline, cancel)?;
        let tls = tls_handshake(connector, &host, tcp, deadline, cancel)?;
        return use_tls(tls);
    }
    let addresses = resolve_host(&host, port, deadline, cancel)?;
    let addresses = if public_only {
        public_relay_addresses(addresses)?
    } else {
        addresses
    };
    let mut last_error = "relay address did not resolve".to_string();
    for address in addresses {
        let tcp = match connect_tcp(address, deadline, cancel) {
            Ok(tcp) => tcp,
            Err(error) if error == RELAY_TIMEOUT => return Err(error),
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        let tls = match tls_handshake(connector, &host, tcp, deadline, cancel) {
            Ok(tls) => tls,
            Err(error) if error == RELAY_TIMEOUT => return Err(error),
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        match use_tls(tls) {
            Ok(value) => return Ok(value),
            Err(error) if error == RELAY_TIMEOUT => return Err(error),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn connect_relay(
    relay: &str,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
    public_only: bool,
    socks: RelaySocks,
) -> Result<RelaySocket, String> {
    if matches!(socks, RelaySocks::Disabled) {
        return Err(SOCKS_MISCONFIGURED.to_string());
    }
    let request = relay
        .into_client_request()
        .map_err(|error| error.to_string())?;
    with_relay_tls(relay, deadline, cancel, public_only, socks, |tls| {
        websocket_handshake(request.clone(), tls, deadline, cancel)
    })
}

const NIP11_BODY_MAX: usize = 64 * 1024;
const NIP11_HEAD_MAX: usize = 8 * 1024;

pub(crate) fn fetch_nip11(relay: &str, deadline: Instant, socks: RelaySocks) -> Option<String> {
    let request = nip11_request(relay).ok()?;
    with_relay_tls(relay, deadline, None, true, socks, |mut tls| {
        write_tls(&mut tls, request.as_bytes(), deadline)?;
        read_nip11_body(&mut tls, deadline)
    })
    .ok()
}

fn nip11_request(relay: &str) -> Result<String, String> {
    let url = url::Url::parse(relay).map_err(|error| error.to_string())?;
    if url.scheme() != "wss" {
        return Err("relay url has no host".to_string());
    }
    let host = url.host_str().ok_or("relay url has no host")?;
    let mut target = url.path().to_string();
    if target.is_empty() {
        target.push('/');
    }
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    let mut authority = host.to_string();
    if let Some(port) = url.port() {
        if port != 443 {
            authority.push(':');
            authority.push_str(&port.to_string());
        }
    }
    let mut request = String::from("GET ");
    request.push_str(&target);
    request.push_str(" HTTP/1.1\r\nHost: ");
    request.push_str(&authority);
    request.push_str("\r\nAccept: application/nostr+json\r\nConnection: close\r\n\r\n");
    Ok(request)
}

fn write_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), String> {
    arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
    tls.write_all(bytes).map_err(nip11_io)?;
    arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
    tls.flush().map_err(nip11_io)
}

fn read_tls(
    tls: &mut native_tls::TlsStream<TcpStream>,
    buf: &mut [u8],
    deadline: Instant,
) -> Result<usize, String> {
    loop {
        arm_stream(tls.get_ref(), wait_budget(deadline, None)?)?;
        match tls.read(buf) {
            Ok(read) => return Ok(read),
            Err(error)
                if error.kind() == std::io::ErrorKind::TimedOut
                    || error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::Interrupted =>
            {
                if relay_wait_is_over(deadline, None) {
                    return Err(RELAY_TIMEOUT.to_string());
                }
            }
            Err(error) => return Err(nip11_io(error)),
        }
    }
}

fn nip11_io(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::TimedOut
        || error.kind() == std::io::ErrorKind::WouldBlock
    {
        RELAY_TIMEOUT.to_string()
    } else {
        error.to_string()
    }
}

fn read_nip11_body(
    tls: &mut native_tls::TlsStream<TcpStream>,
    deadline: Instant,
) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        if let Some(body) = decode_http_message(&buf, false)? {
            return Ok(body);
        }
        let read = read_tls(tls, &mut tmp, deadline)?;
        if read == 0 {
            return decode_http_message(&buf, true)?
                .ok_or_else(|| "relay sent no nip11 document".to_string());
        }
        buf.extend_from_slice(&tmp[..read]);
        if buf.len() > NIP11_HEAD_MAX + NIP11_BODY_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
    }
}

fn decode_http_message(bytes: &[u8], eof: bool) -> Result<Option<String>, String> {
    let Some(split) = header_end(bytes) else {
        if eof || bytes.len() > NIP11_HEAD_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
        return Ok(None);
    };
    if split > NIP11_HEAD_MAX {
        return Err("relay sent no nip11 document".to_string());
    }
    let head = std::str::from_utf8(&bytes[..split])
        .map_err(|_| "relay sent no nip11 document".to_string())?;
    let (status, headers) = parse_http_head(head)?;
    if status != 200 || content_encoded(&headers) {
        return Err("relay sent no nip11 document".to_string());
    }
    let body = &bytes[split..];
    if transfer_chunked(&headers) {
        return match read_chunks(body)? {
            Some(body) => nip11_text(body),
            None if eof => Err("relay sent no nip11 document".to_string()),
            None => Ok(None),
        };
    }
    if let Some(length) = content_length(&headers)? {
        if length > NIP11_BODY_MAX || (eof && body.len() < length) {
            return Err("relay sent no nip11 document".to_string());
        }
        if body.len() < length {
            return Ok(None);
        }
        return nip11_text(body[..length].to_vec());
    }
    if body.len() > NIP11_BODY_MAX {
        return Err("relay sent no nip11 document".to_string());
    }
    if eof {
        nip11_text(body.to_vec())
    } else {
        Ok(None)
    }
}

fn nip11_text(body: Vec<u8>) -> Result<Option<String>, String> {
    let body = String::from_utf8(body).map_err(|_| "relay sent no nip11 document".to_string())?;
    if body.is_empty() {
        Err("relay sent no nip11 document".to_string())
    } else {
        Ok(Some(body))
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn parse_http_head(head: &str) -> Result<(u16, Vec<(String, String)>), String> {
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "relay sent no nip11 document".to_string())?;
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("relay sent no nip11 document".to_string());
        };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok((status, headers))
}

fn content_encoded(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name == "content-encoding" && !value.is_empty() && !value.eq_ignore_ascii_case("identity")
    })
}

fn transfer_chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name == "transfer-encoding"
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    })
}

fn content_length(headers: &[(String, String)]) -> Result<Option<usize>, String> {
    let mut found = None;
    for (name, value) in headers {
        if name != "content-length" {
            continue;
        }
        let length = value
            .parse()
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        if matches!(found, Some(previous) if previous != length) {
            return Err("relay sent no nip11 document".to_string());
        }
        found = Some(length);
    }
    Ok(found)
}

fn read_chunks(mut rest: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = rest.windows(2).position(|window| window == b"\r\n") else {
            return Ok(None);
        };
        let line = std::str::from_utf8(&rest[..line_end])
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        let size_hex = line.split(';').next().unwrap_or("").trim();
        if size_hex.is_empty() {
            return Err("relay sent no nip11 document".to_string());
        }
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| "relay sent no nip11 document".to_string())?;
        if size > NIP11_BODY_MAX || out.len().saturating_add(size) > NIP11_BODY_MAX {
            return Err("relay sent no nip11 document".to_string());
        }
        let data_at = line_end + 2;
        if size == 0 {
            if rest.len() < data_at + 2 {
                return Ok(None);
            }
            if &rest[data_at..data_at + 2] == b"\r\n" {
                return Ok(Some(out));
            }
            if rest[data_at..]
                .windows(4)
                .any(|window| window == b"\r\n\r\n")
            {
                return Ok(Some(out));
            }
            return Ok(None);
        }
        let data_end = data_at + size;
        if rest.len() < data_end + 2 {
            return Ok(None);
        }
        if &rest[data_end..data_end + 2] != b"\r\n" {
            return Err("relay sent no nip11 document".to_string());
        }
        out.extend_from_slice(&rest[data_at..data_end]);
        rest = &rest[data_end + 2..];
    }
}

pub(crate) fn assign_relay(
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
    let tcp = connect_tcp(proxy, deadline, None)?;
    socks5_connect(&tcp, &host, port, relay, deadline, None)?;
    let tls = tls_handshake(connector, &host, tcp, deadline, None)?;
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

fn relay_needs_assignment(age: Option<Duration>, circuit: Option<bool>) -> bool {
    assignment_due(age, circuit, 1)
}

fn assignment_due(age: Option<Duration>, circuit: Option<bool>, misses: u32) -> bool {
    match age {
        None => circuit != Some(true),
        Some(age) if age >= RELAY_CIRCUIT_WARM => true,
        Some(age) if circuit == Some(false) && age >= retry_wait(misses) => true,
        _ => false,
    }
}

fn next_misses(previous: Option<u32>, circuit: Option<bool>) -> u32 {
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

pub(crate) fn relays_needing_assignment(
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

type OpenRelay = Arc<dyn Fn(&str, Duration) + Send + Sync>;
type ReadCircuits = Arc<dyn Fn() -> Option<HashMap<String, String>> + Send + Sync>;

struct PendingAssignment {
    page: Vec<String>,
    configured: Vec<String>,
    timeout: Duration,
    open: OpenRelay,
    circuits: ReadCircuits,
}

// Hidden-service requests share the local address, so dials share one window.
const ASSIGN_LIMIT: usize = 18;
const ASSIGN_WINDOW: Duration = Duration::from_secs(60);

struct RelayAttempt {
    at: Instant,
    misses: u32,
}

struct RelayPass {
    latest: Mutex<Option<PendingAssignment>>,
    requested: Mutex<HashMap<String, RelayAttempt>>,
    dialed: Mutex<Vec<Instant>>,
    running: AtomicBool,
}

impl RelayPass {
    fn new() -> Self {
        Self {
            latest: Mutex::new(None),
            requested: Mutex::new(HashMap::new()),
            dialed: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
        }
    }

    fn submit(
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
            let misses = next_misses(
                noted.get(relay.as_str()).map(|(_, misses)| *misses),
                circuit_seen(paths.as_ref(), &relay),
            );
            self.mark_requested(&relay, now, misses);
            (job.open)(&relay, job.timeout);
        }
        self.retain_requested(&chosen);
    }

    fn allow_assignment(&self, now: Instant) -> bool {
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

    fn mark_requested(&self, relay: &str, at: Instant, misses: u32) {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return;
        };
        self.requested
            .lock()
            .expect("relay circuit pass")
            .insert(key, RelayAttempt { at, misses });
    }

    #[cfg(test)]
    fn settled(&self) -> bool {
        !self.running.load(Ordering::Acquire)
            && self.latest.lock().expect("relay circuit pass").is_none()
    }

    #[cfg(test)]
    fn remembers(&self, relay: &str) -> bool {
        let Some(key) = crate::tor_circuits::relay_key(relay) else {
            return false;
        };
        self.requested
            .lock()
            .expect("relay circuit pass")
            .contains_key(&key)
    }

    #[cfg(test)]
    fn misses(&self, relay: &str) -> Option<u32> {
        let key = crate::tor_circuits::relay_key(relay)?;
        self.requested
            .lock()
            .expect("relay circuit pass")
            .get(&key)
            .map(|attempt| attempt.misses)
    }
}

fn shared_relay_pass() -> &'static Arc<RelayPass> {
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
        let open: OpenRelay = Arc::new(|_: &str, _: Duration| {});
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
fn scripted_relay_answer(relay: &str) -> Option<Option<String>> {
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
#[cfg(test)]
struct PublishScriptBook {
    refused: Mutex<Vec<(String, String)>>,
    accepted: Mutex<Vec<String>>,
}

#[cfg(test)]
static PUBLISH_SCRIPTS: Mutex<Option<Arc<PublishScriptBook>>> = Mutex::new(None);

#[cfg(test)]
fn scripted_send(relay: &str) -> Option<Result<(), SendFailure>> {
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

fn schedule_relay_assignments_via(
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
        if let Err(error) = assign_relay(relay, deadline, socks) {
            eprintln!("Nonograph: relay {relay} was not assigned a circuit: {error}");
        }
    });
    let circuits: ReadCircuits = Arc::new(|| crate::tor_circuits::reported());
    shared_relay_pass().submit(page, configured, timeout, open, circuits);
}

fn socks5_connect(
    stream: &TcpStream,
    host: &str,
    port: u16,
    username: &str,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    let mut stream = stream;
    let user = username.as_bytes();
    if user.is_empty() || user.len() > 255 {
        return Err("socks user is invalid".to_string());
    }
    arm_stream(stream, wait_budget(deadline, cancel)?)?;
    stream
        .write_all(&[0x05, 0x01, 0x02])
        .map_err(|error| error.to_string())?;
    let mut method = [0u8; 2];
    stream
        .read_exact(&mut method)
        .map_err(|error| error.to_string())?;
    if method != [0x05, 0x02] {
        return Err("socks proxy refused authentication".to_string());
    }

    let mut auth = Vec::with_capacity(3 + user.len());
    auth.push(0x01);
    auth.push(user.len() as u8);
    auth.extend_from_slice(user);
    auth.push(0);
    arm_stream(stream, wait_budget(deadline, cancel)?)?;
    stream.write_all(&auth).map_err(|error| error.to_string())?;
    let mut status = [0u8; 2];
    stream
        .read_exact(&mut status)
        .map_err(|error| error.to_string())?;
    if status != [0x01, 0x00] {
        return Err("socks proxy refused authentication".to_string());
    }

    let mut request = vec![0x05, 0x01, 0x00];
    if let Ok(ip) = host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                request.push(0x01);
                request.extend_from_slice(&v4.octets());
            }
            IpAddr::V6(v6) => {
                request.push(0x04);
                request.extend_from_slice(&v6.octets());
            }
        }
    } else {
        let bytes = host.as_bytes();
        if bytes.is_empty() || bytes.len() > 255 {
            return Err("socks host is invalid".to_string());
        }
        request.push(0x03);
        request.push(bytes.len() as u8);
        request.extend_from_slice(bytes);
    }
    request.extend_from_slice(&port.to_be_bytes());
    arm_stream(stream, wait_budget(deadline, cancel)?)?;
    stream
        .write_all(&request)
        .map_err(|error| error.to_string())?;

    let mut head = [0u8; 4];
    stream
        .read_exact(&mut head)
        .map_err(|error| error.to_string())?;
    if head[0] != 0x05 {
        return Err("socks proxy protocol error".to_string());
    }
    if head[1] != 0x00 {
        return Err("socks proxy connect failed".to_string());
    }
    let skip = match head[3] {
        0x01 => 6,
        0x04 => 18,
        0x03 => {
            let mut len = [0u8; 1];
            stream
                .read_exact(&mut len)
                .map_err(|error| error.to_string())?;
            len[0] as usize + 2
        }
        _ => return Err("socks proxy protocol error".to_string()),
    };
    let mut rest = vec![0u8; skip];
    stream
        .read_exact(&mut rest)
        .map_err(|error| error.to_string())?;
    Ok(())
}

enum Incoming {
    Text(String),
    Closed,
}

fn read_incoming(
    socket: &mut RelaySocket,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<Incoming, String> {
    loop {
        let wait = io_slice(deadline, cancel)?;
        arm_stream(socket.get_ref().get_ref(), wait)?;
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => return Ok(Incoming::Text(text.to_string())),
            Ok(tungstenite::Message::Ping(payload)) => {
                socket
                    .send(tungstenite::Message::Pong(payload))
                    .map_err(|error| error.to_string())?;
            }
            Ok(tungstenite::Message::Close(_)) => return Ok(Incoming::Closed),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::TimedOut
                    || error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::Capacity(_)) => {
                return Err(RELAY_FRAME_TOO_BIG.to_string());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn open_relay_socket(
    relay: &str,
    deadline: Instant,
    public_only: bool,
    socks: RelaySocks,
) -> Result<RelaySocket, String> {
    if let Some(tls) = take_held(relay) {
        // A relay can close this parked handshake while Tor still lists its circuit.
        if let Ok(request) = relay.into_client_request() {
            if let Ok(socket) = websocket_handshake(request, tls, deadline, None) {
                return Ok(socket);
            }
        }
    }
    connect_relay(relay, deadline, None, public_only, socks)
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
    serde_json::json!(["REQ", sub_id, { "kinds": [10002] }]).to_string()
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
#[path = "../test/nostr.rs"]
mod tests;
