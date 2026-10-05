use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

static LATCH: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

pub fn current() -> HashMap<String, String> {
    if let Some(reply) = control_reply() {
        let live = paths_by_relay(&reply);
        let mut guard = LATCH.lock().expect("tor circuit latch");
        let latch = guard.get_or_insert_with(HashMap::new);
        for (relay, path) in live {
            latch.insert(relay, path);
        }
    }
    LATCH.lock().expect("tor circuit latch").clone().unwrap_or_default()
}

pub(crate) fn paths_by_relay(reply: &str) -> HashMap<String, String> {
    let mut circuits = HashMap::new();
    for line in info_lines(reply, "circuit-status") {
        let mut parts = line.split_whitespace();
        let id = parts.next();
        let status = parts.next();
        let hops = parts.next();
        let (Some(id), Some(status), Some(hops)) = (id, status, hops) else {
            continue;
        };
        if status != "BUILT" && status != "EXTENDED" {
            continue;
        }
        let path = hop_names(hops);
        let Some(relay) = socks_username(line).as_deref().and_then(relay_key) else {
            continue;
        };
        if !path.is_empty() {
            circuits.insert(id.to_string(), (relay, path));
        }
    }

    let mut relays = HashMap::new();
    for line in info_lines(reply, "stream-status") {
        let mut parts = line.split_whitespace();
        let _stream = parts.next();
        let _status = parts.next();
        let circuit = parts.next();
        let target = parts.next();
        let (Some(circuit), Some(target)) = (circuit, target) else {
            continue;
        };
        if circuit == "0" {
            continue;
        }
        let Some(host) = stream_host(target) else {
            continue;
        };
        let Some((relay, path)) = circuits.get(circuit) else {
            continue;
        };
        if relay_host(relay) == Some(host.as_str()) {
            relays.insert(relay.clone(), path.clone());
        }
    }
    relays
}

fn socks_username(line: &str) -> Option<String> {
    let value = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("SOCKS_USERNAME="))?;
    if value.starts_with('"') {
        unquote(value)
    } else if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn unquote(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut out = Vec::new();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return String::from_utf8(out).ok(),
            b'\\' => {
                index += 1;
                let next = *bytes.get(index)?;
                if (b'0'..b'8').contains(&next) {
                    let mut octet = (next - b'0') as u32;
                    let mut digits = 1;
                    while digits < 3
                        && bytes
                            .get(index + 1)
                            .is_some_and(|byte| (b'0'..b'8').contains(byte))
                    {
                        index += 1;
                        octet = octet * 8 + (bytes[index] - b'0') as u32;
                        digits += 1;
                    }
                    if octet > 255 {
                        return None;
                    }
                    out.push(octet as u8);
                } else {
                    out.push(match next {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        other => other,
                    });
                }
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    None
}

fn relay_key(url: &str) -> Option<String> {
    let rest = url.get(6..).filter(|_| url[..6].eq_ignore_ascii_case("wss://"))?;
    let split = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest[..split].trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let mut tail = &rest[split..];
    if tail == "/" {
        tail = "";
    } else if tail.ends_with('/') {
        tail = &tail[..tail.len() - 1];
    }
    Some(format!("wss://{host}{tail}"))
}

fn relay_host(relay: &str) -> Option<&str> {
    let rest = relay.strip_prefix("wss://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

fn hop_names(hops: &str) -> String {
    hops.split(',')
        .filter_map(|hop| hop.split('~').nth(1))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(" → ")
}

fn stream_host(target: &str) -> Option<String> {
    if target.contains(".exit") {
        return None;
    }
    let host = target.rsplit_once(':').map(|(host, _)| host).unwrap_or(target);
    let host = host.trim_matches(['[', ']']);
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

fn info_lines<'a>(reply: &'a str, key: &str) -> Vec<&'a str> {
    let multi = format!("250+{key}=");
    if let Some(start) = reply.find(&multi) {
        let rest = reply[start + multi.len()..].trim_start_matches(['\r', '\n']);
        return rest
            .lines()
            .map(str::trim_end)
            .take_while(|line| *line != ".")
            .filter(|line| !line.is_empty())
            .collect();
    }
    let single = format!("250-{key}=");
    if let Some(start) = reply.find(&single) {
        let line = reply[start + single.len()..]
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        if !line.is_empty() {
            return vec![line];
        }
    }
    Vec::new()
}

fn control_reply() -> Option<String> {
    let cookie = cookie_hex()?;
    let mut stream = TcpStream::connect_timeout(
        &"127.0.0.1:9051".parse().ok()?,
        Duration::from_millis(400),
    )
    .ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(800))).ok()?;
    let command = format!(
        "AUTHENTICATE {cookie}\r\nGETINFO circuit-status\r\nGETINFO stream-status\r\nQUIT\r\n"
    );
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                reply.push_str(&String::from_utf8_lossy(&buf[..n]));
                if reply.contains("250 closing connection") || reply.len() > 512 * 1024 {
                    break;
                }
            }
        }
    }
    reply.contains("250 OK").then_some(reply)
}

fn cookie_hex() -> Option<String> {
    for path in ["/tmp/tor-control-cookie", "/var/lib/tor/control_auth_cookie"] {
        if let Ok(bytes) = std::fs::read(path) {
            if !bytes.is_empty() {
                return Some(bytes.iter().map(|byte| format!("{byte:02X}")).collect());
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "../test/tor_circuits.rs"]
mod tests;
