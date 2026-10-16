#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
struct CircuitLatch {
    pins: HashMap<String, String>,
}

#[cfg(test)]
static LATCH: Mutex<Option<CircuitLatch>> = Mutex::new(None);

#[cfg(test)]
fn remember(reply: &str) -> HashMap<String, String> {
    let mut latch = LATCH.lock().expect("tor circuit latch");
    let pins = latch
        .as_ref()
        .map(|latch| latch.pins.clone())
        .unwrap_or_default();
    let (paths, pins) = stabilize(built_circuits(reply), &pins);
    *latch = Some(CircuitLatch { pins });
    paths
}

#[cfg(test)]
fn circuit_status(
    proxied: bool,
    live: Option<HashMap<String, String>>,
) -> (bool, HashMap<String, String>) {
    match live {
        Some(relays) if proxied => (true, relays),
        _ => (false, HashMap::new()),
    }
}

#[cfg(test)]
struct BuiltCircuit {
    id: String,
    relay: String,
    path: String,
}

#[cfg(test)]
fn built_circuits(reply: &str) -> Vec<BuiltCircuit> {
    let mut circuits = Vec::new();
    for line in info_lines(reply, "circuit-status") {
        let mut parts = line.split_whitespace();
        let Some(id) = parts.next() else {
            continue;
        };
        let Some(status) = parts.next() else {
            continue;
        };
        let Some(hops) = parts.next() else {
            continue;
        };
        if status != "BUILT" {
            continue;
        }
        let path = hop_names(hops);
        let Some(relay) = socks_username(line).as_deref().and_then(relay_key) else {
            continue;
        };
        if !path.is_empty() {
            circuits.push(BuiltCircuit {
                id: id.to_string(),
                relay,
                path,
            });
        }
    }
    circuits
}

#[cfg(test)]
pub(crate) fn paths_by_relay(reply: &str) -> HashMap<String, String> {
    stabilize(built_circuits(reply), &HashMap::new()).0
}

#[cfg(test)]
fn stabilize(
    circuits: Vec<BuiltCircuit>,
    pins: &HashMap<String, String>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut by_relay: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for circuit in circuits {
        by_relay
            .entry(circuit.relay)
            .or_default()
            .push((circuit.id, circuit.path));
    }
    let mut paths = HashMap::new();
    let mut next_pins = HashMap::new();
    for (relay, list) in by_relay {
        let chosen = pins
            .get(&relay)
            .and_then(|id| list.iter().find(|(circ, _)| circ == id))
            .unwrap_or(&list[0]);
        next_pins.insert(relay.clone(), chosen.0.clone());
        paths.insert(relay, chosen.1.clone());
    }
    (paths, next_pins)
}

#[cfg(test)]
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

#[cfg(test)]
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

pub(crate) fn relay_key(url: &str) -> Option<String> {
    let rest = url
        .get(6..)
        .filter(|_| url[..6].eq_ignore_ascii_case("wss://"))?;
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

#[cfg(test)]
fn hop_names(hops: &str) -> String {
    hops.split(',')
        .skip(1)
        .filter_map(|hop| hop.split('~').nth(1))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(" → ")
}

#[cfg(test)]
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

#[cfg(test)]
const CONTROL_CACHE: Duration = Duration::from_secs(1);

#[cfg(test)]
struct CachedReply {
    at: Instant,
    reply: Option<String>,
}

#[cfg(test)]
fn reused_control_reply(cached: &CachedReply, now: Instant) -> Option<Option<String>> {
    (now.saturating_duration_since(cached.at) < CONTROL_CACHE).then(|| cached.reply.clone())
}

#[cfg(test)]
#[path = "../test/tor_circuits.rs"]
mod tests;
