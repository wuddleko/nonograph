use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

use socket2::{Domain, SockAddr, Socket, Type};

use bech32::primitives::decode::CheckedHrpstring;
use bech32::{Bech32, Hrp};
use rand::{thread_rng, Rng};
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, XOnlyPublicKey, SECP256K1};
use sha2::{Digest, Sha256};
use tungstenite::client::IntoClientRequest;

pub const KIND_LONG_FORM: u32 = 30023;
const KIND_SEAL: u32 = 13;
pub const KIND_GIFT_WRAP: u32 = 1059;
pub(crate) const MAX_FETCH_RELAYS: usize = 6;
const TWO_DAYS_SECS: i64 = 2 * 24 * 60 * 60;

pub struct SignedNote {
    pub id: [u8; 32],
    pub pubkey: [u8; 32],
    pub event_json: String,
}

pub struct Nevent {
    pub event_id_hex: String,
    pub relays: Vec<String>,
    pub kind: Option<u32>,
}

#[derive(Clone)]
pub struct Naddr {
    pub identifier: String,
    pub pubkey: [u8; 32],
    pub kind: u32,
    pub relays: Vec<String>,
}

#[derive(Default)]
pub struct FetchedNote {
    pub id_hex: String,
    pub title: String,
    pub author: String,
    pub content: String,
    pub created_at: i64,
    pub pubkey: [u8; 32],
    pub identifier: String,
}

impl SignedNote {
    pub fn id_hex(&self) -> String {
        hex_encode(&self.id)
    }
}

#[cfg(test)]
pub fn sign_note(title: &str, author: &str, content: &str, created_at: i64) -> SignedNote {
    let keypair = Keypair::new(SECP256K1, &mut thread_rng());
    signed_event(
        &keypair,
        created_at,
        KIND_LONG_FORM,
        &long_form_tags(title, author, created_at),
        content,
    )
}

#[derive(Debug)]
pub struct WrapError;

pub struct WrappedNote {
    pub id: [u8; 32],
    pub pubkey: [u8; 32],
    pub event_json: String,
    pub recipient_secret: [u8; 32],
}

impl WrappedNote {
    pub fn id_hex(&self) -> String {
        hex_encode(&self.id)
    }

    pub fn signed_note(&self) -> SignedNote {
        SignedNote {
            id: self.id,
            pubkey: self.pubkey,
            event_json: self.event_json.clone(),
        }
    }
}

pub struct OpenedNote {
    pub title: String,
    pub author: String,
    pub content: String,
    pub created_at: i64,
}

pub fn wrap_note(
    title: &str,
    author: &str,
    content: &str,
    created_at: i64,
) -> Result<WrappedNote, WrapError> {
    let author_key = Keypair::new(SECP256K1, &mut thread_rng());
    let recipient_key = Keypair::new(SECP256K1, &mut thread_rng());
    let wrap_key = Keypair::new(SECP256K1, &mut thread_rng());
    let recipient_secret = recipient_key.secret_bytes();
    let recipient_pubkey = recipient_key.x_only_public_key().0.serialize();

    let rumor = rumor_json(
        &author_key,
        created_at,
        KIND_LONG_FORM,
        &long_form_tags(title, author, created_at),
        content,
    );
    let seal_content = nip44_encrypt(&rumor, &author_key.secret_bytes(), &recipient_pubkey)?;
    let seal = signed_event(
        &author_key,
        random_past(now_secs()),
        KIND_SEAL,
        &[],
        &seal_content,
    );
    let wrap_content = nip44_encrypt(
        &seal.event_json,
        &wrap_key.secret_bytes(),
        &recipient_pubkey,
    )?;
    let wrap = signed_event(
        &wrap_key,
        random_past(now_secs()),
        KIND_GIFT_WRAP,
        &[vec!["p".to_string(), hex_encode(&recipient_pubkey)]],
        &wrap_content,
    );
    Ok(WrappedNote {
        id: wrap.id,
        pubkey: wrap.pubkey,
        event_json: wrap.event_json,
        recipient_secret,
    })
}

pub fn open_wrapped_note(
    event_json: &str,
    recipient_secret: &[u8; 32],
) -> Result<OpenedNote, WrapError> {
    let wrap: serde_json::Value = serde_json::from_str(event_json).map_err(|_| WrapError)?;
    let wrap_event = verify_signed(&wrap)?;
    if wrap_event.kind != KIND_GIFT_WRAP {
        return Err(WrapError);
    }
    let recipient_pubkey = xonly_pubkey(recipient_secret)?;
    let tagged = wrap_event
        .tags
        .iter()
        .find(|tag| tag.first().map(String::as_str) == Some("p"))
        .and_then(|tag| tag.get(1))
        .and_then(|hex| decode_fixed_hex::<32>(hex))
        .ok_or(WrapError)?;
    if tagged != recipient_pubkey {
        return Err(WrapError);
    }
    let seal_json = nip44_decrypt(&wrap_event.content, recipient_secret, &wrap_event.pubkey)?;
    let seal: serde_json::Value = serde_json::from_str(&seal_json).map_err(|_| WrapError)?;
    let seal_event = verify_signed(&seal)?;
    if seal_event.kind != KIND_SEAL || !seal_event.tags.is_empty() {
        return Err(WrapError);
    }
    let rumor_json = nip44_decrypt(&seal_event.content, recipient_secret, &seal_event.pubkey)?;
    let rumor: serde_json::Value = serde_json::from_str(&rumor_json).map_err(|_| WrapError)?;
    if rumor.get("sig").is_some() {
        return Err(WrapError);
    }
    let rumor_event = verify_unsigned_id(&rumor)?;
    if rumor_event.kind != KIND_LONG_FORM || rumor_event.pubkey != seal_event.pubkey {
        return Err(WrapError);
    }
    let (title, author, created_at) = note_fields(&rumor_event.tags, rumor_event.created_at);
    Ok(OpenedNote {
        title,
        author,
        content: rumor_event.content,
        created_at,
    })
}

pub fn encode_nevent(id: &[u8; 32], relays: &[String], pubkey: &[u8; 32], kind: u32) -> String {
    let mut data = Vec::new();
    push_tlv(&mut data, 0, id);
    push_relay_tlvs(&mut data, relays);
    push_tlv(&mut data, 2, pubkey);
    push_tlv(&mut data, 3, &kind.to_be_bytes());
    encode_bech32("nevent", &data)
}

pub fn decode_nevent(value: &str) -> Option<Nevent> {
    let mut event_id = None;
    let mut relays = Vec::new();
    let mut kind = None;
    for (tag, bytes) in nip19_tlv(value, "nevent")? {
        match tag {
            0 if bytes.len() == 32 => event_id = Some(hex_encode(&bytes)),
            1 => relays.push(String::from_utf8(bytes).ok()?),
            3 if bytes.len() == 4 => {
                kind = Some(u32::from_be_bytes(bytes.try_into().ok()?));
            }
            _ => {}
        }
    }
    Some(Nevent {
        event_id_hex: event_id?,
        relays,
        kind,
    })
}

pub fn encode_naddr(identifier: &str, relays: &[String], pubkey: &[u8; 32], kind: u32) -> String {
    let ident = identifier.as_bytes();
    let mut data = Vec::new();
    if ident.len() <= 255 {
        push_tlv(&mut data, 0, ident);
    }
    push_relay_tlvs(&mut data, relays);
    push_tlv(&mut data, 2, pubkey);
    push_tlv(&mut data, 3, &kind.to_be_bytes());
    encode_bech32("naddr", &data)
}

pub fn decode_naddr(value: &str) -> Option<Naddr> {
    let mut identifier = None;
    let mut relays = Vec::new();
    let mut pubkey = None;
    let mut kind = None;
    for (tag, bytes) in nip19_tlv(value, "naddr")? {
        match tag {
            0 => identifier = Some(String::from_utf8(bytes).ok()?),
            1 => relays.push(String::from_utf8(bytes).ok()?),
            2 if bytes.len() == 32 => pubkey = Some(bytes.try_into().ok()?),
            3 if bytes.len() == 4 => {
                kind = Some(u32::from_be_bytes(bytes.try_into().ok()?));
            }
            _ => {}
        }
    }
    Some(Naddr {
        identifier: identifier?,
        pubkey: pubkey?,
        kind: kind?,
        relays,
    })
}

pub fn naddr_cache_id(naddr: &Naddr) -> String {
    let mut hasher = Sha256::new();
    hasher.update(naddr.pubkey);
    hasher.update(naddr.kind.to_be_bytes());
    hasher.update(naddr.identifier.as_bytes());
    hex_encode(&hasher.finalize())
}

pub fn encode_nsec(secret: &[u8; 32]) -> String {
    encode_bech32("nsec", secret)
}

pub fn decode_nsec(value: &str) -> Option<[u8; 32]> {
    if !value.starts_with("nsec1") {
        return None;
    }
    let parsed = CheckedHrpstring::new::<Bech32>(value).ok()?;
    if parsed.hrp().as_str() != "nsec" {
        return None;
    }
    parsed.byte_iter().collect::<Vec<u8>>().try_into().ok()
}

pub fn publish_to_relays(relays: &[String], note: &SignedNote, timeout: Duration) -> Vec<String> {
    let relays: Vec<String> = relays
        .iter()
        .filter(|relay| valid_relay_url(relay))
        .cloned()
        .collect();
    if relays.is_empty() {
        return Vec::new();
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
                match send_event(&relay, &event_json, &event_id, deadline) {
                    Ok(()) => Some(relay),
                    Err(error) => {
                        eprintln!("Nonograph: relay {relay} did not accept the note: {error}");
                        None
                    }
                }
            }));
        }
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok().flatten())
            .collect()
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

type RelaySocket = tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>;

const RELAY_TIMEOUT: &str = "timed out waiting for the relay";
const FETCH_POLL: Duration = Duration::from_millis(100);

fn tls_connector() -> Result<&'static native_tls::TlsConnector, String> {
    static CONNECTOR: OnceLock<Result<native_tls::TlsConnector, String>> = OnceLock::new();
    match CONNECTOR
        .get_or_init(|| native_tls::TlsConnector::new().map_err(|error| error.to_string()))
    {
        Ok(connector) => Ok(connector),
        Err(error) => Err(error.clone()),
    }
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

fn connect_relay(
    relay: &str,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
    public_only: bool,
) -> Result<RelaySocket, String> {
    let request = relay
        .into_client_request()
        .map_err(|error| error.to_string())?;
    let uri = request.uri().clone();
    let host = uri.host().ok_or("relay url has no host")?.to_string();
    let port = uri.port_u16().unwrap_or(443);
    let addresses = resolve_host(&host, port, deadline, cancel)?;
    let addresses = if public_only {
        public_relay_addresses(addresses)?
    } else {
        addresses
    };
    let connector = tls_connector()?;
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
        match websocket_handshake(request.clone(), tls, deadline, cancel) {
            Ok(socket) => return Ok(socket),
            Err(error) if error == RELAY_TIMEOUT => return Err(error),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
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
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn send_event(
    relay: &str,
    event_json: &str,
    event_id_hex: &str,
    deadline: Instant,
) -> Result<(), String> {
    let mut socket = connect_relay(relay, deadline, None, false)?;
    let payload = format!("[\"EVENT\",{event_json}]");
    arm_stream(socket.get_ref().get_ref(), wait_budget(deadline, None)?)?;
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| error.to_string())?;

    loop {
        match read_incoming(&mut socket, deadline, None)? {
            Incoming::Closed => return Err("relay closed the connection".to_string()),
            Incoming::Text(text) => {
                if let Some(result) = parse_ok(&text, event_id_hex) {
                    return result.map_err(|reason| {
                        if reason.is_empty() {
                            "relay rejected the note".to_string()
                        } else {
                            reason
                        }
                    });
                }
            }
        }
    }
}

pub fn fetch_note(
    relays: &[String],
    event_id_hex: &str,
    timeout: Duration,
    recipient_secret: Option<[u8; 32]>,
) -> Option<FetchedNote> {
    fetch_from_relays(
        relays,
        RelayQuery::Id {
            event_id_hex: event_id_hex.to_string(),
            recipient_secret,
        },
        timeout,
        false,
    )
}

pub fn fetch_public_note(
    relays: &[String],
    event_id_hex: &str,
    timeout: Duration,
) -> Option<FetchedNote> {
    fetch_from_relays(
        relays,
        RelayQuery::Id {
            event_id_hex: event_id_hex.to_string(),
            recipient_secret: None,
        },
        timeout,
        true,
    )
}

pub fn fetch_public_addr(
    relays: &[String],
    naddr: &Naddr,
    timeout: Duration,
) -> Option<FetchedNote> {
    fetch_from_relays(relays, RelayQuery::Addr(naddr.clone()), timeout, true)
}

#[derive(Clone)]
enum RelayQuery {
    Id {
        event_id_hex: String,
        recipient_secret: Option<[u8; 32]>,
    },
    Addr(Naddr),
}

fn fetch_from_relays(
    relays: &[String],
    query: RelayQuery,
    timeout: Duration,
    public_only: bool,
) -> Option<FetchedNote> {
    match &query {
        RelayQuery::Id { event_id_hex, .. } => {
            if decode_fixed_hex::<32>(event_id_hex).is_none() {
                return None;
            }
        }
        RelayQuery::Addr(naddr) => {
            if naddr.kind != KIND_LONG_FORM || naddr.identifier.len() > 255 {
                return None;
            }
        }
    }
    let limit = if public_only {
        MAX_FETCH_RELAYS
    } else {
        usize::MAX
    };
    let relays: Vec<String> = relays
        .iter()
        .filter(|relay| {
            if public_only {
                public_relay_url(relay)
            } else {
                valid_relay_url(relay)
            }
        })
        .take(limit)
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
        let query = query.clone();
        let cancel = Arc::clone(&cancel);
        handles.push(std::thread::spawn(move || {
            let found = match fetch_from_relay(&relay, &query, deadline, &cancel, public_only) {
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

fn take_first_note(
    rx: mpsc::Receiver<Option<FetchedNote>>,
    deadline: Instant,
) -> Option<FetchedNote> {
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

fn req_payload(sub_id: &str, query: &RelayQuery) -> String {
    match query {
        RelayQuery::Id { event_id_hex, .. } => {
            format!(r#"["REQ","{sub_id}",{{"ids":["{event_id_hex}"]}}]"#)
        }
        RelayQuery::Addr(naddr) => serde_json::json!([
            "REQ",
            sub_id,
            {
                "authors": [hex_encode(&naddr.pubkey)],
                "kinds": [naddr.kind],
                "#d": [naddr.identifier],
            }
        ])
        .to_string(),
    }
}

fn fetch_from_relay(
    relay: &str,
    query: &RelayQuery,
    deadline: Instant,
    cancel: &AtomicBool,
    public_only: bool,
) -> Result<Option<FetchedNote>, String> {
    let mut socket = connect_relay(relay, deadline, Some(cancel), public_only)?;
    let sub_id = random_hex(8);
    let payload = req_payload(&sub_id, query);
    arm_stream(
        socket.get_ref().get_ref(),
        wait_budget(deadline, Some(cancel))?,
    )?;
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| error.to_string())?;

    let newest_until_eose = matches!(query, RelayQuery::Addr(_));
    let mut best = None;
    let mut best_at = i64::MIN;
    loop {
        let incoming = match read_incoming(&mut socket, deadline, Some(cancel)) {
            Ok(incoming) => incoming,
            Err(error) if error == RELAY_TIMEOUT && newest_until_eose => return Ok(best),
            Err(error) => return Err(error),
        };
        match incoming {
            Incoming::Closed => {
                return if newest_until_eose && best.is_some() {
                    Ok(best)
                } else {
                    Err("relay closed the connection".to_string())
                };
            }
            Incoming::Text(text) => {
                if let Some((created_at, note)) = note_for_query(&text, query) {
                    if !newest_until_eose {
                        return Ok(Some(note));
                    }
                    if created_at >= best_at {
                        best_at = created_at;
                        best = Some(note);
                    }
                }
                if relay_has_no_event(&text, &sub_id) {
                    return Ok(best);
                }
            }
        }
    }
}

fn note_for_query(message: &str, query: &RelayQuery) -> Option<(i64, FetchedNote)> {
    match query {
        RelayQuery::Id {
            event_id_hex,
            recipient_secret,
        } => note_from_relay_message(message, event_id_hex, recipient_secret.as_ref())
            .map(|note| (0, note)),
        RelayQuery::Addr(naddr) => addr_note_from_relay_message(message, naddr),
    }
}

fn note_from_relay_message(
    message: &str,
    event_id_hex: &str,
    recipient_secret: Option<&[u8; 32]>,
) -> Option<FetchedNote> {
    fetched_from_event(
        &event_from_relay_message(message)?,
        event_id_hex,
        recipient_secret,
    )
}

fn event_from_relay_message(message: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    let items = value.as_array()?;
    if items.first()?.as_str()? != "EVENT" {
        return None;
    }
    items.get(2).cloned()
}

fn fetched_from_event(
    value: &serde_json::Value,
    expected_id_hex: &str,
    recipient_secret: Option<&[u8; 32]>,
) -> Option<FetchedNote> {
    let kind = json_kind(value)?;
    if kind == KIND_LONG_FORM {
        return note_from_value(value, expected_id_hex);
    }
    if kind != KIND_GIFT_WRAP {
        return None;
    }
    let secret = recipient_secret?;
    let id = decode_fixed_hex::<32>(value.get("id")?.as_str()?)?;
    if decode_fixed_hex::<32>(expected_id_hex)? != id {
        return None;
    }
    let opened = open_wrapped_note(&value.to_string(), secret).ok()?;
    Some(FetchedNote {
        id_hex: hex_encode(&id),
        title: opened.title,
        author: opened.author,
        content: opened.content,
        created_at: opened.created_at,
        ..FetchedNote::default()
    })
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

fn note_from_value(value: &serde_json::Value, expected_id_hex: &str) -> Option<FetchedNote> {
    let parsed = verified_long_form(value)?;
    if decode_fixed_hex::<32>(expected_id_hex)? != parsed.id {
        return None;
    }
    Some(fetched_from_parsed(parsed))
}

fn addr_note_from_relay_message(message: &str, naddr: &Naddr) -> Option<(i64, FetchedNote)> {
    fetched_public_addr(&event_from_relay_message(message)?, naddr)
}

fn fetched_public_addr(value: &serde_json::Value, naddr: &Naddr) -> Option<(i64, FetchedNote)> {
    if naddr.kind != KIND_LONG_FORM {
        return None;
    }
    let parsed = verified_long_form(value)?;
    if parsed.pubkey != naddr.pubkey {
        return None;
    }
    if tag_value(&parsed.tags, "d").as_deref() != Some(naddr.identifier.as_str()) {
        return None;
    }
    let created_at = parsed.created_at;
    Some((created_at, fetched_from_parsed(parsed)))
}

fn verified_long_form(value: &serde_json::Value) -> Option<ParsedEvent> {
    let parsed = parse_event(value)?;
    if parsed.kind != KIND_LONG_FORM || !check_id(&parsed) || !verify_sig(&parsed) {
        return None;
    }
    Some(parsed)
}

fn fetched_from_parsed(parsed: ParsedEvent) -> FetchedNote {
    let (title, author, created_at) = note_fields(&parsed.tags, parsed.created_at);
    FetchedNote {
        id_hex: hex_encode(&parsed.id),
        title,
        author,
        content: parsed.content,
        created_at,
        pubkey: parsed.pubkey,
        identifier: tag_value(&parsed.tags, "d").unwrap_or_default(),
    }
}

fn tag_value(tags: &[Vec<String>], name: &str) -> Option<String> {
    tags.iter()
        .find(|tag| tag.first().map(String::as_str) == Some(name))
        .and_then(|tag| tag.get(1).cloned())
}

fn note_fields(tags: &[Vec<String>], event_created_at: i64) -> (String, String, i64) {
    let title = tag_value(tags, "title").unwrap_or_else(|| "Untitled".to_string());
    let author = tag_value(tags, "author").unwrap_or_default();
    let created_at = tag_value(tags, "published_at")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(event_created_at);
    (title, author, created_at)
}

fn decode_fixed_hex<const N: usize>(hex: &str) -> Option<[u8; N]> {
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

fn valid_relay_url(relay: &str) -> bool {
    let Some(host) = relay.strip_prefix("wss://") else {
        return false;
    };
    !host.is_empty()
        && relay.is_ascii()
        && relay.len() <= 255
        && !relay.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub(crate) fn public_relay_url(relay: &str) -> bool {
    if !valid_relay_url(relay) {
        return false;
    }
    let Ok(url) = url::Url::parse(relay) else {
        return false;
    };
    if url.scheme() != "wss" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if url.port_or_known_default() != Some(443) {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            let host = domain.trim_end_matches('.').to_ascii_lowercase();
            if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
                return false;
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                return !blocked_relay_ip(ip);
            }
            true
        }
        Some(url::Host::Ipv4(v4)) => !blocked_relay_ip(IpAddr::V4(v4)),
        Some(url::Host::Ipv6(v6)) => !blocked_relay_ip(IpAddr::V6(v6)),
        None => false,
    }
}

fn blocked_relay_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => blocked_relay_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return blocked_relay_ipv4(v4);
            }
            if let Some(v4) = v6.to_ipv4() {
                return blocked_relay_ipv4(v4);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                || nat64_prefix(v6)
        }
    }
}

fn blocked_relay_ipv4(v4: Ipv4Addr) -> bool {
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.octets()[0] == 0
        || carrier_grade_nat(v4)
}

fn carrier_grade_nat(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn nat64_prefix(v6: Ipv6Addr) -> bool {
    let segments = v6.segments();
    segments[0] == 0x64
        && segments[1] == 0xff9b
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0
}

fn public_relay_addresses(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let public: Vec<SocketAddr> = addresses
        .into_iter()
        .filter(|address| address.port() == 443 && !blocked_relay_ip(address.ip()))
        .collect();
    if public.is_empty() {
        Err("relay address is not public".to_string())
    } else {
        Ok(public)
    }
}

fn push_tlv(out: &mut Vec<u8>, tag: u8, value: &[u8]) {
    out.push(tag);
    out.push(value.len() as u8);
    out.extend_from_slice(value);
}

fn push_relay_tlvs(out: &mut Vec<u8>, relays: &[String]) {
    for relay in relays {
        if valid_relay_url(relay) {
            push_tlv(out, 1, relay.as_bytes());
        }
    }
}

fn encode_bech32(hrp: &str, data: &[u8]) -> String {
    let hrp = Hrp::parse(hrp).expect("known hrp");
    bech32::encode::<Bech32>(hrp, data).expect("payload fits in a bech32 string")
}

fn nip19_tlv(value: &str, hrp: &str) -> Option<Vec<(u8, Vec<u8>)>> {
    if !value.starts_with(hrp) || value.as_bytes().get(hrp.len()) != Some(&b'1') {
        return None;
    }
    let parsed = CheckedHrpstring::new::<Bech32>(value).ok()?;
    if parsed.hrp().as_str() != hrp {
        return None;
    }
    let data: Vec<u8> = parsed.byte_iter().collect();
    let mut fields = Vec::new();
    let mut index = 0;
    while index + 2 <= data.len() {
        let tag = data[index];
        let len = data[index + 1] as usize;
        index += 2;
        if index + len > data.len() {
            return None;
        }
        fields.push((tag, data[index..index + len].to_vec()));
        index += len;
    }
    (index == data.len()).then_some(fields)
}

fn canonical_event(
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let mut out = String::new();
    out.push_str("[0,\"");
    out.push_str(pubkey_hex);
    out.push_str("\",");
    out.push_str(&created_at.to_string());
    out.push(',');
    out.push_str(&kind.to_string());
    out.push(',');
    push_tags(&mut out, tags);
    out.push(',');
    push_json_string(&mut out, content);
    out.push(']');
    out
}

fn long_form_tags(title: &str, author: &str, created_at: i64) -> Vec<Vec<String>> {
    let mut tags = vec![
        vec!["d".to_string(), random_hex(16)],
        vec!["title".to_string(), title.to_string()],
        vec!["published_at".to_string(), created_at.to_string()],
    ];
    if !author.is_empty() {
        tags.push(vec!["author".to_string(), author.to_string()]);
    }
    tags
}

fn signed_event(
    keypair: &Keypair,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> SignedNote {
    let pubkey = keypair.x_only_public_key().0.serialize();
    let pubkey_hex = hex_encode(&pubkey);
    let id = event_id(&pubkey_hex, created_at, kind, tags, content);
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

fn rumor_json(
    keypair: &Keypair,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let pubkey_hex = hex_encode(&keypair.x_only_public_key().0.serialize());
    let id = event_id(&pubkey_hex, created_at, kind, tags, content);
    event_wire(
        &hex_encode(&id),
        &pubkey_hex,
        created_at,
        kind,
        tags,
        content,
        None,
    )
}

fn event_id(
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> [u8; 32] {
    Sha256::digest(canonical_event(pubkey_hex, created_at, kind, tags, content).as_bytes()).into()
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
    push_json_string(&mut out, id_hex);
    out.push_str(",\"pubkey\":");
    push_json_string(&mut out, pubkey_hex);
    out.push_str(",\"created_at\":");
    out.push_str(&created_at.to_string());
    out.push_str(",\"kind\":");
    out.push_str(&kind.to_string());
    out.push_str(",\"tags\":");
    push_tags(&mut out, tags);
    out.push_str(",\"content\":");
    push_json_string(&mut out, content);
    if let Some(sig_hex) = sig_hex {
        out.push_str(",\"sig\":");
        push_json_string(&mut out, sig_hex);
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

fn check_id(parsed: &ParsedEvent) -> bool {
    event_id(
        &parsed.pubkey_hex,
        parsed.created_at,
        parsed.kind,
        &parsed.tags,
        &parsed.content,
    ) == parsed.id
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

fn verify_signed(value: &serde_json::Value) -> Result<ParsedEvent, WrapError> {
    let parsed = parse_event(value).ok_or(WrapError)?;
    if check_id(&parsed) && verify_sig(&parsed) {
        Ok(parsed)
    } else {
        Err(WrapError)
    }
}

fn verify_unsigned_id(value: &serde_json::Value) -> Result<ParsedEvent, WrapError> {
    let parsed = parse_event(value).ok_or(WrapError)?;
    if check_id(&parsed) {
        Ok(parsed)
    } else {
        Err(WrapError)
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn random_past(now: i64) -> i64 {
    now.saturating_sub(thread_rng().gen_range(0..=TWO_DAYS_SECS))
}

fn nip44_encrypt(
    plaintext: &str,
    private_key: &[u8; 32],
    public_key: &[u8; 32],
) -> Result<String, WrapError> {
    let conversation =
        crate::nip44::conversation_key(private_key, public_key).map_err(|_| WrapError)?;
    let mut nonce = [0u8; 32];
    thread_rng().fill(&mut nonce);
    crate::nip44::encrypt(plaintext, &conversation, &nonce).map_err(|_| WrapError)
}

fn nip44_decrypt(
    payload: &str,
    private_key: &[u8; 32],
    public_key: &[u8; 32],
) -> Result<String, WrapError> {
    let conversation =
        crate::nip44::conversation_key(private_key, public_key).map_err(|_| WrapError)?;
    crate::nip44::decrypt(payload, &conversation).map_err(|_| WrapError)
}

fn xonly_pubkey(secret: &[u8; 32]) -> Result<[u8; 32], WrapError> {
    let secret_key = secp256k1::SecretKey::from_slice(secret).map_err(|_| WrapError)?;
    let keypair = Keypair::from_secret_key(SECP256K1, &secret_key);
    Ok(keypair.x_only_public_key().0.serialize())
}

fn push_tags(out: &mut String, tags: &[Vec<String>]) {
    out.push('[');
    for (tag_index, tag) in tags.iter().enumerate() {
        if tag_index > 0 {
            out.push(',');
        }
        out.push('[');
        for (item_index, item) in tag.iter().enumerate() {
            if item_index > 0 {
                out.push(',');
            }
            push_json_string(out, item);
        }
        out.push(']');
    }
    out.push(']');
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn random_hex(bytes: usize) -> String {
    let mut rng = thread_rng();
    let raw: Vec<u8> = (0..bytes).map(|_| rng.gen()).collect();
    hex_encode(&raw)
}

fn hex_encode(bytes: &[u8]) -> String {
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
