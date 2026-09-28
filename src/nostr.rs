use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bech32::primitives::decode::CheckedHrpstring;
use bech32::{Bech32, Hrp};
use rand::{thread_rng, Rng};
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, Secp256k1, XOnlyPublicKey};
use sha2::{Digest, Sha256};
use tungstenite::client::IntoClientRequest;

const KIND_LONG_FORM: u32 = 30023;

pub struct SignedNote {
    pub id: [u8; 32],
    pub pubkey: [u8; 32],
    pub event_json: String,
}

pub struct Nevent {
    pub event_id_hex: String,
    pub relays: Vec<String>,
}

pub struct FetchedNote {
    pub id_hex: String,
    pub title: String,
    pub author: String,
    pub content: String,
    pub created_at: i64,
}

impl SignedNote {
    pub fn id_hex(&self) -> String {
        hex_encode(&self.id)
    }
}

pub fn sign_note(title: &str, author: &str, content: &str, created_at: i64) -> SignedNote {
    let secp = Secp256k1::new();
    let keypair = Keypair::new(&secp, &mut thread_rng());
    let (xonly, _parity) = keypair.x_only_public_key();
    let pubkey = xonly.serialize();
    let pubkey_hex = hex_encode(&pubkey);

    let mut tags = vec![
        vec!["d".to_string(), random_hex(16)],
        vec!["title".to_string(), title.to_string()],
        vec!["published_at".to_string(), created_at.to_string()],
    ];
    if !author.is_empty() {
        tags.push(vec!["author".to_string(), author.to_string()]);
    }

    let preimage = canonical_event(&pubkey_hex, created_at, KIND_LONG_FORM, &tags, content);
    let id: [u8; 32] = Sha256::digest(preimage.as_bytes()).into();
    let sig = secp.sign_schnorr(&Message::from_digest(id), &keypair);
    let sig_hex = hex_encode(&sig.serialize());
    let id_hex = hex_encode(&id);

    let event_json = wire_event(&id_hex, &pubkey_hex, created_at, &tags, content, &sig_hex);
    SignedNote {
        id,
        pubkey,
        event_json,
    }
}

pub fn encode_nevent(id: &[u8; 32], relays: &[String], pubkey: &[u8; 32]) -> String {
    let mut data = Vec::new();
    push_tlv(&mut data, 0, id);
    for relay in relays {
        if valid_relay_url(relay) {
            push_tlv(&mut data, 1, relay.as_bytes());
        }
    }
    push_tlv(&mut data, 2, pubkey);
    let kind = KIND_LONG_FORM.to_be_bytes();
    push_tlv(&mut data, 3, &kind);
    let hrp = Hrp::parse("nevent").expect("nevent hrp");
    bech32::encode::<Bech32>(hrp, &data).expect("nevent fits in a bech32 string")
}

pub fn decode_nevent(value: &str) -> Option<Nevent> {
    if !value.starts_with("nevent1") {
        return None;
    }
    let parsed = CheckedHrpstring::new::<Bech32>(value).ok()?;
    if parsed.hrp().as_str() != "nevent" {
        return None;
    }
    let data = parsed.byte_iter().collect::<Vec<u8>>();
    let mut event_id = None;
    let mut relays = Vec::new();
    let mut index = 0;
    while index + 2 <= data.len() {
        let tag = data[index];
        let len = data[index + 1] as usize;
        index += 2;
        if index + len > data.len() {
            return None;
        }
        let bytes = &data[index..index + len];
        index += len;
        match tag {
            0 if bytes.len() == 32 => event_id = Some(hex_encode(bytes)),
            1 => relays.push(String::from_utf8(bytes.to_vec()).ok()?),
            _ => {}
        }
    }
    if index != data.len() {
        return None;
    }
    Some(Nevent {
        event_id_hex: event_id?,
        relays,
    })
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

    let event_id = note.id_hex();
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(relays.len());
        for relay in &relays {
            let relay = relay.clone();
            let event_json = note.event_json.clone();
            let event_id = event_id.clone();
            handles.push(scope.spawn(move || {
                match send_event(&relay, &event_json, &event_id, timeout) {
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

fn connect_relay(relay: &str, timeout: Duration) -> Result<RelaySocket, String> {
    let request = relay
        .into_client_request()
        .map_err(|error| error.to_string())?;
    let uri = request.uri().clone();
    let host = uri.host().ok_or("relay url has no host")?.to_string();
    let port = uri.port_u16().unwrap_or(443);
    let mut last_error = "relay address did not resolve".to_string();
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?;
    for address in addresses {
        let tcp = match TcpStream::connect_timeout(&address, timeout) {
            Ok(tcp) => tcp,
            Err(error) => {
                last_error = error.to_string();
                continue;
            }
        };
        tcp.set_read_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        tcp.set_write_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        let connector = native_tls::TlsConnector::new().map_err(|error| error.to_string())?;
        let tls = connector
            .connect(&host, tcp)
            .map_err(|error| error.to_string())?;
        let (socket, _) =
            tungstenite::client::client(request, tls).map_err(|error| error.to_string())?;
        return Ok(socket);
    }
    Err(last_error)
}

enum Incoming {
    Text(String),
    Closed,
}

fn read_incoming(socket: &mut RelaySocket, deadline: Instant) -> Result<Incoming, String> {
    loop {
        if Instant::now() >= deadline {
            return Err("timed out waiting for the relay".to_string());
        }
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
                    || error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                return Err("timed out waiting for the relay".to_string());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn send_event(
    relay: &str,
    event_json: &str,
    event_id_hex: &str,
    timeout: Duration,
) -> Result<(), String> {
    let mut socket = connect_relay(relay, timeout)?;
    let payload = format!("[\"EVENT\",{event_json}]");
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| error.to_string())?;

    let deadline = Instant::now() + timeout;
    loop {
        match read_incoming(&mut socket, deadline)? {
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

pub fn fetch_note(relays: &[String], event_id_hex: &str, timeout: Duration) -> Option<FetchedNote> {
    if decode_fixed_hex::<32>(event_id_hex).is_none() {
        return None;
    }
    let relays: Vec<String> = relays
        .iter()
        .filter(|relay| valid_relay_url(relay))
        .cloned()
        .collect();
    if relays.is_empty() {
        return None;
    }

    let (tx, rx) = mpsc::channel();
    for relay in relays {
        let tx = tx.clone();
        let event_id_hex = event_id_hex.to_string();
        std::thread::spawn(move || {
            let found = match fetch_from_relay(&relay, &event_id_hex, timeout) {
                Ok(note) => note,
                Err(error) => {
                    eprintln!("Nonograph: relay {relay} did not return the note: {error}");
                    None
                }
            };
            let _ = tx.send(found);
        });
    }
    drop(tx);
    take_first_note(rx, Instant::now() + timeout)
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

fn fetch_from_relay(
    relay: &str,
    event_id_hex: &str,
    timeout: Duration,
) -> Result<Option<FetchedNote>, String> {
    let mut socket = connect_relay(relay, timeout)?;
    let sub_id = random_hex(8);
    let payload = format!(r#"["REQ","{sub_id}",{{"ids":["{event_id_hex}"]}}]"#);
    socket
        .send(tungstenite::Message::Text(payload.into()))
        .map_err(|error| error.to_string())?;

    let deadline = Instant::now() + timeout;
    loop {
        match read_incoming(&mut socket, deadline)? {
            Incoming::Closed => return Err("relay closed the connection".to_string()),
            Incoming::Text(text) => {
                if let Some(note) = note_from_relay_message(&text, event_id_hex) {
                    return Ok(Some(note));
                }
                if relay_has_no_event(&text, &sub_id) {
                    return Ok(None);
                }
            }
        }
    }
}

fn note_from_relay_message(message: &str, event_id_hex: &str) -> Option<FetchedNote> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    let items = value.as_array()?;
    if items.first()?.as_str()? != "EVENT" {
        return None;
    }
    note_from_value(items.get(2)?, event_id_hex)
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
    let id_hex = value.get("id")?.as_str()?;
    let pubkey_hex = value.get("pubkey")?.as_str()?;
    let created_at = value.get("created_at")?.as_i64()?;
    let kind = value.get("kind")?.as_u64()?;
    if kind != u64::from(KIND_LONG_FORM) {
        return None;
    }
    let content = value.get("content")?.as_str()?.to_string();
    let sig_hex = value.get("sig")?.as_str()?;
    let tags = value
        .get("tags")?
        .as_array()?
        .iter()
        .map(|tag| {
            tag.as_array()?
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
        })
        .collect::<Option<Vec<Vec<String>>>>()?;

    let preimage = canonical_event(pubkey_hex, created_at, kind as u32, &tags, &content);
    let recomputed: [u8; 32] = Sha256::digest(preimage.as_bytes()).into();
    if decode_fixed_hex::<32>(id_hex)? != recomputed {
        return None;
    }
    if decode_fixed_hex::<32>(expected_id_hex)? != recomputed {
        return None;
    }
    let pubkey = XOnlyPublicKey::from_slice(&decode_fixed_hex::<32>(pubkey_hex)?).ok()?;
    let signature = Signature::from_slice(&decode_fixed_hex::<64>(sig_hex)?).ok()?;
    Secp256k1::new()
        .verify_schnorr(&signature, &Message::from_digest(recomputed), &pubkey)
        .ok()?;

    let title = tag_value(&tags, "title").unwrap_or_else(|| "Untitled".to_string());
    let author = tag_value(&tags, "author").unwrap_or_default();
    let published_at = tag_value(&tags, "published_at")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(created_at);
    Some(FetchedNote {
        id_hex: hex_encode(&recomputed),
        title,
        author,
        content,
        created_at: published_at,
    })
}

fn tag_value(tags: &[Vec<String>], name: &str) -> Option<String> {
    tags.iter()
        .find(|tag| tag.first().map(String::as_str) == Some(name))
        .and_then(|tag| tag.get(1).cloned())
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

fn push_tlv(out: &mut Vec<u8>, tag: u8, value: &[u8]) {
    out.push(tag);
    out.push(value.len() as u8);
    out.extend_from_slice(value);
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

fn wire_event(
    id_hex: &str,
    pubkey_hex: &str,
    created_at: i64,
    tags: &[Vec<String>],
    content: &str,
    sig_hex: &str,
) -> String {
    let mut out = String::from("{\"id\":");
    push_json_string(&mut out, id_hex);
    out.push_str(",\"pubkey\":");
    push_json_string(&mut out, pubkey_hex);
    out.push_str(",\"created_at\":");
    out.push_str(&created_at.to_string());
    out.push_str(",\"kind\":");
    out.push_str(&KIND_LONG_FORM.to_string());
    out.push_str(",\"tags\":");
    push_tags(&mut out, tags);
    out.push_str(",\"content\":");
    push_json_string(&mut out, content);
    out.push_str(",\"sig\":");
    push_json_string(&mut out, sig_hex);
    out.push('}');
    out
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
mod tests {
    use super::*;
    use secp256k1::XOnlyPublicKey;

    #[test]
    fn test_npub_bech32_matches_nip19() {
        let bytes = decode_fixed_hex::<32>(
            "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d",
        )
        .unwrap();
        let hrp = Hrp::parse("npub").unwrap();
        let encoded = bech32::encode::<Bech32>(hrp, &bytes).unwrap();
        assert_eq!(
            encoded,
            "npub180cvv07tjdrrgpa0j7j7tmnyl2yr6yr7l8j4s3evf6u64th6gkwsyjh6w6"
        );
    }

    #[test]
    fn test_signed_note_verifies_and_ids_differ() {
        let first = sign_note(
            "Hello",
            "Ada",
            "a line\nwith \"quotes\" and \\slashes",
            1_700_000_000,
        );
        let second = sign_note(
            "Hello",
            "Ada",
            "a line\nwith \"quotes\" and \\slashes",
            1_700_000_000,
        );
        assert_ne!(first.id, second.id);
        assert_ne!(first.pubkey, second.pubkey);
        assert_signature(&first);
        assert_signature(&second);

        let parsed: serde_json::Value = serde_json::from_str(&first.event_json).unwrap();
        assert_eq!(parsed["kind"], 30023);
        assert_eq!(parsed["content"], "a line\nwith \"quotes\" and \\slashes");
        assert_eq!(parsed["tags"][1][0], "title");
        assert_eq!(parsed["tags"][1][1], "Hello");
        assert_eq!(parsed["tags"][3][0], "author");
        assert_eq!(parsed["tags"][3][1], "Ada");
    }

    #[test]
    fn test_nevent_round_trip_carries_id_and_relays() {
        let note = sign_note("Title", "", "body", 1_700_000_000);
        let relays = vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
        ];
        let nevent = encode_nevent(&note.id, &relays, &note.pubkey);
        let decoded = decode_nevent(&nevent).unwrap();
        assert_eq!(decoded.event_id_hex, hex_encode(&note.id));
        assert_eq!(decoded.relays, relays);
        assert!(decode_nevent("about").is_none());
        assert!(decode_nevent("nevent1qqqq").is_none());
    }

    #[test]
    fn test_parse_ok_matches_the_event_id() {
        let id = "ab".repeat(32);
        assert!(parse_ok(&format!(r#"["OK","{id}",true,""]"#), &id)
            .unwrap()
            .is_ok());
        let rejected = parse_ok(&format!(r#"["OK","{id}",false,"blocked"]"#), &id).unwrap();
        assert!(rejected.is_err());
        assert!(parse_ok(r#"["NOTICE","hi"]"#, &id).is_none());
        assert!(parse_ok(&format!(r#"["OK","{}",true,""]"#, "cd".repeat(32)), &id).is_none());
    }

    #[test]
    fn test_fetched_note_keeps_title_author_and_markdown() {
        let note = sign_note(
            "Hello",
            "Ada",
            "a line\nwith \"quotes\" and \\slashes",
            1_700_000_000,
        );
        let message = format!(r#"["EVENT","sub",{}]"#, note.event_json);
        let fetched = note_from_relay_message(&message, &note.id_hex()).unwrap();
        assert_eq!(fetched.id_hex, note.id_hex());
        assert_eq!(fetched.title, "Hello");
        assert_eq!(fetched.author, "Ada");
        assert_eq!(fetched.content, "a line\nwith \"quotes\" and \\slashes");
        assert_eq!(fetched.created_at, 1_700_000_000);
        assert!(relay_has_no_event(r#"["EOSE","sub"]"#, "sub"));
        assert!(!relay_has_no_event(r#"["EOSE","other"]"#, "sub"));
        assert!(note_from_relay_message(r#"["EOSE","sub"]"#, &note.id_hex()).is_none());
    }

    #[test]
    fn test_fetched_note_rejects_a_tampered_event() {
        let note = sign_note("Hello", "", "body", 1_700_000_000);
        let mut parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
        parsed["content"] = serde_json::Value::String("edited".to_string());
        assert!(note_from_value(&parsed, &note.id_hex()).is_none());

        let mut wrong_kind: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
        wrong_kind["kind"] = serde_json::Value::from(1);
        assert!(note_from_value(&wrong_kind, &note.id_hex()).is_none());

        let mut bad_sig: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
        let mut sig = bad_sig["sig"].as_str().unwrap().to_string();
        let flipped = if sig.starts_with('a') { 'b' } else { 'a' };
        sig.replace_range(0..1, &flipped.to_string());
        bad_sig["sig"] = serde_json::Value::String(sig);
        assert!(note_from_value(&bad_sig, &note.id_hex()).is_none());
        assert!(note_from_value(
            &serde_json::from_str(&note.event_json).unwrap(),
            &"ab".repeat(32)
        )
        .is_none());
    }

    #[test]
    fn test_fetch_note_skips_relays_it_cannot_ask() {
        let id = "ab".repeat(32);
        assert!(fetch_note(&[], &id, Duration::from_millis(20)).is_none());
        assert!(fetch_note(
            &["https://relay.example".to_string()],
            &id,
            Duration::from_millis(20)
        )
        .is_none());
        assert!(fetch_note(
            &["wss://relay.example".to_string()],
            "abcd",
            Duration::from_millis(20)
        )
        .is_none());
    }

    #[test]
    fn test_first_note_returns_when_the_first_relay_answers() {
        let (tx, rx) = mpsc::channel();
        tx.send(None).unwrap();
        let late = tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            let _ = late.send(Some(FetchedNote {
                id_hex: "ab".repeat(32),
                title: "Hello".to_string(),
                author: String::new(),
                content: "body".to_string(),
                created_at: 1_700_000_000,
            }));
        });
        let started = Instant::now();
        let note = take_first_note(rx, Instant::now() + Duration::from_secs(2)).unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(note.title, "Hello");
        assert_eq!(note.content, "body");
        drop(tx);
    }

    #[test]
    fn test_first_note_times_out_when_every_relay_is_silent() {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        assert!(take_first_note(rx, Instant::now() + Duration::from_millis(40)).is_none());
        assert!(started.elapsed() < Duration::from_millis(400));
        drop(tx);
    }

    fn assert_signature(note: &SignedNote) {
        let secp = Secp256k1::new();
        let pubkey = XOnlyPublicKey::from_slice(&note.pubkey).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
        let sig_hex = parsed["sig"].as_str().unwrap();
        let sig_bytes = decode_fixed_hex::<64>(sig_hex).unwrap();
        let signature = secp256k1::schnorr::Signature::from_slice(&sig_bytes).unwrap();
        let tags = parsed["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| {
                tag.as_array()
                    .unwrap()
                    .iter()
                    .map(|item| item.as_str().unwrap().to_string())
                    .collect()
            })
            .collect::<Vec<Vec<String>>>();
        let preimage = canonical_event(
            parsed["pubkey"].as_str().unwrap(),
            parsed["created_at"].as_i64().unwrap(),
            parsed["kind"].as_u64().unwrap() as u32,
            &tags,
            parsed["content"].as_str().unwrap(),
        );
        let recomputed: [u8; 32] = Sha256::digest(preimage.as_bytes()).into();
        assert_eq!(recomputed, note.id);
        secp.verify_schnorr(&signature, &Message::from_digest(note.id), &pubkey)
            .unwrap();
    }
}
