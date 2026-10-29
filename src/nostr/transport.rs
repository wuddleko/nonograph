use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(not(windows))]
use socket2::{Domain, SockAddr, Socket, Type};
use tungstenite::client::IntoClientRequest;

use super::{public_relay_addresses, public_relay_url, RelaySocks};

pub(super) type RelaySocket = tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>;
pub(super) type HeldTls = native_tls::TlsStream<TcpStream>;

// A parked handshake is one socket. Relays close an idle one after about a minute,
// and a visitor can name a new set of relays faster than that.
pub(super) const HELD_RELAY_LIMIT: usize = 16;
const HELD_RELAY_TTL: Duration = Duration::from_secs(45);

struct HeldRelay {
    at: Instant,
    tls: HeldTls,
}

fn held_relays() -> &'static Mutex<HashMap<String, HeldRelay>> {
    static HELD: OnceLock<Mutex<HashMap<String, HeldRelay>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn take_held(relay: &str) -> Option<HeldTls> {
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

pub(super) fn store_held(relay: &str, tls: HeldTls) {
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

pub(super) fn reap_held_relays() {
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

pub(super) fn held_keys_to_drop(
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

pub(super) const RELAY_TIMEOUT: &str = "timed out waiting for the relay";
const RELAY_FRAME_TOO_BIG: &str = "relay frame exceeded the sealed size cap";
pub(super) const SOCKS_MISCONFIGURED: &str = "socks proxy is misconfigured";
const FETCH_POLL: Duration = Duration::from_millis(100);

pub(super) fn tls_connector() -> Result<&'static native_tls::TlsConnector, String> {
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

pub(super) fn build_tls_connector() -> Result<native_tls::TlsConnector, String> {
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
pub(super) fn use_invalid_test_certs() {
    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .expect("test tls connector");
    let _ = test_tls_slot().set(connector);
}

pub(super) fn relay_wait_is_over(deadline: Instant, cancel: Option<&AtomicBool>) -> bool {
    Instant::now() >= deadline || cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

pub(super) fn wait_budget(
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<Duration, String> {
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

pub(super) fn arm_stream(stream: &TcpStream, wait: Duration) -> Result<(), String> {
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
        let socket = self.as_raw_socket();
        let mut read_set = FdSet::one(socket);
        let mut write_set = FdSet::one(socket);
        let mut except_set = FdSet::one(socket);
        let timeout = windows_timeval(wait);
        let ready = unsafe {
            select(
                1,
                if read {
                    &mut read_set
                } else {
                    std::ptr::null_mut()
                },
                if write {
                    &mut write_set
                } else {
                    std::ptr::null_mut()
                },
                &mut except_set,
                &timeout,
            )
        };
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
            readable: read && read_set.has(socket),
            writable: write && write_set.has(socket),
            failed: except_set.has(socket),
        })
    }
}

#[cfg(windows)]
#[repr(C)]
struct FdSet {
    count: u32,
    sockets: [std::os::windows::io::RawSocket; 64],
}

#[cfg(windows)]
impl FdSet {
    fn one(socket: std::os::windows::io::RawSocket) -> Self {
        Self {
            count: 1,
            sockets: [socket; 64],
        }
    }

    fn has(&self, socket: std::os::windows::io::RawSocket) -> bool {
        let count = (self.count as usize).min(self.sockets.len());
        self.sockets[..count].contains(&socket)
    }
}

#[cfg(windows)]
#[repr(C)]
struct Timeval {
    sec: i32,
    usec: i32,
}

#[cfg(windows)]
fn windows_timeval(wait: Duration) -> Timeval {
    let millis = wait.as_millis().clamp(1, i32::MAX as u128) as i32;
    Timeval {
        sec: millis / 1000,
        usec: (millis % 1000) * 1000,
    }
}

#[cfg(all(windows, target_pointer_width = "64"))]
const _: () = assert!(std::mem::size_of::<FdSet>() == 520);
#[cfg(all(windows, target_pointer_width = "64"))]
const _: () = assert!(std::mem::offset_of!(FdSet, sockets) == 8);
#[cfg(all(windows, target_pointer_width = "32"))]
const _: () = assert!(std::mem::size_of::<FdSet>() == 260);
#[cfg(all(windows, target_pointer_width = "32"))]
const _: () = assert!(std::mem::offset_of!(FdSet, sockets) == 4);

#[cfg(windows)]
#[link(name = "ws2_32")]
extern "system" {
    fn select(
        nfds: i32,
        readfds: *mut FdSet,
        writefds: *mut FdSet,
        exceptfds: *mut FdSet,
        timeout: *const Timeval,
    ) -> i32;
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

#[cfg(not(windows))]
fn connection_pending(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
    ) {
        return true;
    }
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == libc::EINPROGRESS || code == libc::EALREADY || code == libc::EWOULDBLOCK
    )
}

#[cfg(not(windows))]
fn already_connected(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::EISCONN)
}

pub(super) const CONNECT_LIMIT: Duration = Duration::from_secs(10);

pub(super) fn connect_deadline(deadline: Instant) -> Instant {
    deadline.min(Instant::now() + CONNECT_LIMIT)
}

pub(super) fn connect_tcp(
    address: SocketAddr,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<TcpStream, String> {
    #[cfg(windows)]
    {
        return connect_tcp_windows(address, deadline, cancel);
    }
    #[cfg(not(windows))]
    {
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
                    None if ready.failed && !ready.writable => {
                        return Err("connection failed".to_string());
                    }
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
}

#[cfg(windows)]
fn connect_tcp_windows(
    address: SocketAddr,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<TcpStream, String> {
    // A refused connect on a non-blocking socket is invisible to select and
    // WSAPoll: the wait ends at the deadline and the relay is reported as
    // timed out. The standard connect reports that refusal.
    let wait = wait_budget(deadline, cancel)?;
    if cancel.is_none() {
        return TcpStream::connect_timeout(&address, wait).map_err(windows_connect_error);
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(TcpStream::connect_timeout(&address, wait));
    });
    loop {
        let slice = io_slice(deadline, cancel)?;
        match rx.recv_timeout(slice) {
            Ok(result) => return result.map_err(windows_connect_error),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("connection failed".to_string());
            }
        }
    }
}

#[cfg(windows)]
fn windows_connect_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::TimedOut {
        RELAY_TIMEOUT.to_string()
    } else {
        error.to_string()
    }
}

pub(super) fn tls_handshake(
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

pub(super) fn resolve_host(
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

pub(super) fn with_relay_tls<T>(
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
    let deadline = connect_deadline(deadline);
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

pub(super) fn connect_relay(
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
    let deadline = connect_deadline(deadline);
    with_relay_tls(relay, deadline, cancel, public_only, socks, |tls| {
        websocket_handshake(request.clone(), tls, deadline, cancel)
    })
}

pub(super) fn socks5_connect(
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

pub(super) enum Incoming {
    Text(String),
    Closed,
}

pub(super) fn read_incoming(
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

pub(super) fn open_relay_socket(
    relay: &str,
    deadline: Instant,
    public_only: bool,
    socks: RelaySocks,
) -> Result<RelaySocket, String> {
    let connect_by = connect_deadline(deadline);
    if let Some(tls) = take_held(relay) {
        // A relay can close this parked handshake while Tor still lists its circuit.
        if let Ok(request) = relay.into_client_request() {
            if let Ok(socket) = websocket_handshake(request, tls, connect_by, None) {
                return Ok(socket);
            }
        }
    }
    connect_relay(relay, connect_by, None, public_only, socks)
}
