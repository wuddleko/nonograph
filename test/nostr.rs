use super::*;
use secp256k1::{Secp256k1, XOnlyPublicKey};
use serial_test::serial;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;

#[test]
fn test_npub_bech32_matches_nip19() {
    let bytes =
        decode_fixed_hex::<32>("3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d")
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
    let content = "a line\nwith \"quotes\" and \\slashes";
    let first = sign_sealed_note(content, "0123456789abcdef0123", 1_700_000_000);
    let second = sign_sealed_note(content, "0123456789abcdef0123", 1_700_000_000);
    assert_ne!(first.id, second.id);
    assert_ne!(first.pubkey, second.pubkey);
    assert_signature(&first);
    assert_signature(&second);

    let parsed: serde_json::Value = serde_json::from_str(&first.event_json).unwrap();
    assert_eq!(parsed["content"], content);
}

#[test]
fn seal_query_asks_for_the_event_id() {
    let event_id = "ab".repeat(32);
    let payload = req_payload("sub", &event_id);
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value[0], "REQ");
    assert_eq!(value[1], "sub");
    assert_eq!(value[2]["ids"][0], event_id);
    assert!(value[2].get("kinds").is_none());
    assert!(value[2].get("#d").is_none());
    assert!(value[2].get("limit").is_none());
    assert!(value[2].get("authors").is_none());
}

#[test]
fn a_sealed_relay_event_is_returned_unchanged() {
    let locator = "0123456789abcdef0123";
    let payload_max = crate::seal::sealed_payload_max(0, 0, 4);
    let content = v2_payload(locator, 2);
    assert!(content.len() <= payload_max);
    let note = sign_sealed_note(&content, locator, 1_700_000_000);
    let spaced = note.event_json.replace("\":\"", "\" : \"");
    let message = format!(r#"["EVENT","sub",{spaced}]"#);
    let (created_at, event_json) =
        sealed_from_relay_message(&message, &note.id_hex(), locator, payload_max).unwrap();
    assert_eq!(created_at, 1_700_000_000);
    assert_eq!(event_json, spaced);
    let parsed: serde_json::Value = serde_json::from_str(&event_json).unwrap();
    assert_eq!(parsed["content"], content);
    assert_eq!(parsed["id"], note.id_hex());
    assert!(parsed["content"].as_str().unwrap().len() <= payload_max);
    let huge = sign_sealed_note(&"h".repeat(payload_max + 1), locator, 1_700_000_000);
    let huge_message = format!(r#"["EVENT","sub",{}]"#, huge.event_json);
    assert!(
        sealed_from_relay_message(&huge_message, &huge.id_hex(), locator, payload_max).is_none()
    );
    assert!(sealed_from_relay_message(
        &message,
        &note.id_hex(),
        "ffffffffffffffff0000",
        payload_max
    )
    .is_none());
    assert!(sealed_from_relay_message(&message, &"ab".repeat(32), locator, payload_max).is_none());
    let padded = format!(
        r#"{},"pad":"{}"}}"#,
        &spaced[..spaced.len() - 1],
        "x".repeat(sealed_event_max(payload_max))
    );
    let padded_message = format!(r#"["EVENT","sub",{padded}]"#);
    assert!(padded.len() > sealed_event_max(payload_max));
    assert!(
        sealed_from_relay_message(&padded_message, &note.id_hex(), locator, payload_max).is_none()
    );
    let mut bad_sig = parsed.clone();
    let sig = bad_sig["sig"].as_str().unwrap();
    let flipped = if sig.ends_with('0') {
        format!("{}1", &sig[..sig.len() - 1])
    } else {
        format!("{}0", &sig[..sig.len() - 1])
    };
    bad_sig["sig"] = serde_json::Value::String(flipped);
    let bad_message = format!(r#"["EVENT","sub",{}]"#, bad_sig);
    assert!(
        sealed_from_relay_message(&bad_message, &note.id_hex(), locator, payload_max).is_none()
    );
    let clear = signed_event(
        &Keypair::new(SECP256K1, &mut thread_rng()),
        1_700_000_000,
        30023,
        &[
            vec!["d".to_string(), "fedcba9876543210abcd".to_string()],
            vec!["title".to_string(), "Hello".to_string()],
            vec!["author".to_string(), "Ada".to_string()],
        ],
        "plaintext",
    );
    let clear_message = format!(r#"["EVENT","sub",{}]"#, clear.event_json);
    assert!(
        sealed_from_relay_message(&clear_message, &clear.id_hex(), locator, payload_max).is_none()
    );
}

#[test]
fn a_sealed_fetch_sizes_the_event_not_the_frame() {
    let locator = "0123456789abcdef0123";
    let payload_max = crate::seal::sealed_payload_max(0, 0, 4);
    let note = sign_sealed_note(&v2_payload(locator, 2), locator, 1_700_000_000);
    let pretty = serde_json::to_string_pretty(
        &serde_json::from_str::<serde_json::Value>(&note.event_json).unwrap(),
    )
    .unwrap();
    assert!(pretty.len() > note.event_json.len());
    let pretty_message = format!(r#"["EVENT","sub",{pretty}]"#);
    let (_, event_json) =
        sealed_from_relay_message(&pretty_message, &note.id_hex(), locator, payload_max).unwrap();
    assert_eq!(event_json, pretty);
    let pad = " ".repeat(
        sealed_event_max(4)
            .saturating_sub(note.event_json.len())
            .saturating_add(32),
    );
    let wrapped = format!(r#"["EVENT","sub",{}{pad}]"#, note.event_json);
    assert!(wrapped.len() > sealed_event_max(4));
    assert!(wrapped.len() <= sealed_frame_max(4));
    let (_, kept) =
        sealed_from_relay_message(&wrapped, &note.id_hex(), locator, payload_max).unwrap();
    assert_eq!(kept, note.event_json);
    let wide = " ".repeat(
        sealed_frame_max(4)
            .saturating_sub(note.event_json.len())
            .saturating_add(64),
    );
    let wide_message = format!(r#"["EVENT","sub",{}{wide}]"#, note.event_json);
    assert!(wide_message.len() > sealed_frame_max(4));
    assert!(wide_message.len() <= sealed_frame_max(payload_max));
    let (_, kept) =
        sealed_from_relay_message(&wide_message, &note.id_hex(), locator, payload_max).unwrap();
    assert_eq!(kept, note.event_json);
    assert_eq!(sealed_event_max(4), 4 + SEALED_EVENT_OVERHEAD);
    assert_eq!(
        sealed_frame_max(4),
        4 + SEALED_EVENT_OVERHEAD + SEALED_FRAME_OVERHEAD
    );
    assert!(SEALED_EVENT_OVERHEAD > SEALED_FRAME_OVERHEAD);
}

#[test]
fn a_sealed_fetch_keeps_the_requested_event() {
    let locator = "0123456789abcdef0123";
    let older_body = v2_payload(locator, 2);
    let newer_body = v2_payload(locator, 3);
    assert_ne!(older_body, newer_body);
    let older = sign_sealed_note(&older_body, locator, 1_700_000_000);
    let newer = sign_sealed_note(&newer_body, locator, 1_800_000_000);
    let event_id = older.id_hex();
    let payload_max = crate::seal::sealed_payload_max(128, 32, 256);
    let old_message = format!(r#"["EVENT","sub",{}]"#, older.event_json);
    let new_message = format!(r#"["EVENT","sub",{}]"#, newer.event_json);
    let old_json = note_for_query(&old_message, &event_id, locator, payload_max).unwrap();
    assert!(note_for_query(&new_message, &event_id, locator, payload_max).is_none());
    assert!(old_json.contains(&older_body));
    assert!(!old_json.contains(&newer_body));
    let (tx, rx) = mpsc::channel();
    tx.send(Some(old_json)).unwrap();
    drop(tx);
    let started = Instant::now();
    let kept = take_first_note(rx, Instant::now() + Duration::from_secs(2)).unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(kept.contains(&older_body));
    assert!(!kept.contains(&newer_body));
}

#[test]
fn a_title_tag_is_dropped_on_the_sealed_fetch_path() {
    let locator = "0123456789abcdef0123";
    let titled = signed_event(
        &Keypair::new(SECP256K1, &mut thread_rng()),
        1_700_000_000,
        nonograph_nip44::KIND,
        &[
            vec!["d".to_string(), locator.to_string()],
            vec![
                "nonce".to_string(),
                "1".to_string(),
                nonograph_nip44::POW_BITS.to_string(),
            ],
            vec!["title".to_string(), "Visible".to_string()],
        ],
        &v2_payload(locator, 2),
    );
    let message = format!(r#"["EVENT","sub",{}]"#, titled.event_json);
    assert!(titled.event_json.contains("Visible"));
    assert!(sealed_from_relay_message(
        &message,
        &titled.id_hex(),
        locator,
        crate::seal::sealed_payload_max(128, 32, 256),
    )
    .is_none());
}

#[test]
#[serial(publish_script)]
fn a_publish_records_the_refused_relay_and_leaves_out_a_relay_that_did_not_answer() {
    let note = sign_sealed_note("kept", "0123456789abcdef0123", 1_700_000_000);
    let script = PublishScript::begin();
    script.refuse("wss://refused.example");
    script.limit("wss://slow.example");
    let started = Instant::now();
    let published = publish_to_relays(
        &[
            "wss://refused.example".to_string(),
            "wss://slow.example".to_string(),
            "wss://silent.example".to_string(),
        ],
        &note,
        Duration::from_secs(2),
    );
    assert_eq!(published.refused, vec!["wss://refused.example".to_string()]);
    assert!(published.accepted.is_empty());
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(script);
}

#[test]
fn a_policy_reason_is_a_durable_refusal() {
    assert!(durable_refusal("blocked: kind"));
    assert!(durable_refusal("restricted: we require auth"));
    for reason in [
        "rate-limited: slow down",
        "error: missing",
        "duplicate: already have this event",
        "pow: difficulty 20",
        "invalid: id",
        "",
        "relay rejected the note",
    ] {
        assert!(!durable_refusal(reason), "{reason}");
    }
}

#[test]
fn a_fetch_of_that_relay_returns_the_note_and_a_dead_relay_returns_nothing() {
    let locator = "0123456789abcdef0123";
    let note = sign_sealed_note("kept", locator, 1_700_000_000);
    let answers = RelayAnswer::begin();
    answers.answer("wss://live.example", &note.event_json);
    let event_id = note.id_hex();
    let started = Instant::now();
    assert_eq!(
        fetch_sealed(
            &["wss://live.example".to_string()],
            &event_id,
            locator,
            128,
            32,
            4,
            Duration::from_secs(2),
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert!(fetch_sealed(
        &["wss://dead.example".to_string()],
        &event_id,
        locator,
        128,
        32,
        4,
        Duration::from_secs(2),
    )
    .is_none());
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn fetch_sealed_skips_when_it_cannot_ask() {
    let event_id = "ab".repeat(32);
    assert!(fetch_sealed(
        &[],
        &event_id,
        "0123456789abcdef0123",
        128,
        32,
        4,
        Duration::from_millis(20)
    )
    .is_none());
    assert!(fetch_sealed(
        &["wss://relay.damus.io".to_string()],
        &event_id,
        "not-a-locator",
        128,
        32,
        4,
        Duration::from_millis(20),
    )
    .is_none());
    assert!(fetch_sealed(
        &["wss://relay.damus.io".to_string()],
        "not-an-id",
        "0123456789abcdef0123",
        128,
        32,
        4,
        Duration::from_millis(20),
    )
    .is_none());
}

#[test]
fn a_sealed_fetch_caps_the_websocket_frame() {
    let mut socket = tungstenite::WebSocket::from_raw_socket(
        std::io::Cursor::new(Vec::<u8>::new()),
        tungstenite::protocol::Role::Client,
        None,
    );
    assert!(socket.get_config().max_message_size.unwrap() > sealed_frame_max(256_000));
    assert!(socket.get_config().max_frame_size.unwrap() > sealed_frame_max(256_000));
    limit_sealed_socket(&mut socket, 4);
    let max = sealed_frame_max(4);
    assert_eq!(max, 4 + SEALED_EVENT_OVERHEAD + SEALED_FRAME_OVERHEAD);
    assert_eq!(socket.get_config().max_message_size, Some(max));
    assert_eq!(socket.get_config().max_frame_size, Some(max));
    limit_socket(&mut socket, PUBLISH_FRAME_MAX);
    assert_eq!(
        socket.get_config().max_message_size,
        Some(PUBLISH_FRAME_MAX)
    );
    assert_eq!(socket.get_config().max_frame_size, Some(PUBLISH_FRAME_MAX));
    limit_socket(&mut socket, LIST_FRAME_MAX);
    assert_eq!(socket.get_config().max_message_size, Some(LIST_FRAME_MAX));
    assert_eq!(socket.get_config().max_frame_size, Some(LIST_FRAME_MAX));
    assert!(LIST_FRAME_MAX < 64 * 1024 * 1024);
    assert!(PUBLISH_FRAME_MAX < LIST_FRAME_MAX);
    assert_rejects_claimed_frame(PUBLISH_FRAME_MAX);
    assert_rejects_claimed_frame(LIST_FRAME_MAX);
}

fn assert_rejects_claimed_frame(max: usize) {
    let claimed = 16u64 << 20;
    assert!(claimed as usize > max);
    let mut frame = vec![0x81, 0x7F];
    frame.extend_from_slice(&claimed.to_be_bytes());
    let mut socket = tungstenite::WebSocket::from_raw_socket(
        std::io::Cursor::new(frame),
        tungstenite::protocol::Role::Client,
        None,
    );
    limit_socket(&mut socket, max);
    for _ in 0..2 {
        match socket.read() {
            Err(tungstenite::Error::Capacity(
                tungstenite::error::CapacityError::MessageTooLong { size, max_size },
            )) => {
                assert_eq!(size, claimed as usize);
                assert_eq!(max_size, max);
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_sealed_fetch_stops_on_an_oversize_frame() {
    assert_rejects_claimed_frame(sealed_frame_max(4));
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
fn test_first_note_returns_when_the_first_relay_answers() {
    let (tx, rx) = mpsc::channel();
    tx.send(None).unwrap();
    let late = tx.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        let _ = late.send(Some("body".to_string()));
    });
    let started = Instant::now();
    let note = take_first_note(rx, Instant::now() + Duration::from_secs(2)).unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(note, "body");
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

#[test]
fn a_relay_does_not_connect_after_its_deadline() {
    let started = Instant::now();
    let error = connect_relay(
        "wss://127.0.0.1:9",
        Instant::now(),
        None,
        false,
        RelaySocks::Direct,
    )
    .unwrap_err();
    assert_eq!(error, RELAY_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn connect_relay_sends_socks_connect_for_the_host() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let (host_tx, host_rx) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut hello = [0u8; 3];
            if stream.read_exact(&mut hello).is_err() || hello != [0x05, 0x01, 0x02] {
                return;
            }
            if stream.write_all(&[0x05, 0x02]).is_err() {
                return;
            }
            let mut ulen = [0u8; 2];
            if stream.read_exact(&mut ulen).is_err() || ulen[0] != 0x01 {
                return;
            }
            let mut user = vec![0u8; ulen[1] as usize];
            if stream.read_exact(&mut user).is_err() {
                return;
            }
            let mut plen = [0u8; 1];
            if stream.read_exact(&mut plen).is_err() || plen[0] != 0 {
                return;
            }
            if stream.write_all(&[0x01, 0x00]).is_err() {
                return;
            }
            let mut head = [0u8; 4];
            if stream.read_exact(&mut head).is_err() {
                return;
            }
            if head[3] != 0x03 {
                return;
            }
            let mut len = [0u8; 1];
            if stream.read_exact(&mut len).is_err() {
                return;
            }
            let mut host = vec![0u8; len[0] as usize];
            if stream.read_exact(&mut host).is_err() {
                return;
            }
            let mut port = [0u8; 2];
            if stream.read_exact(&mut port).is_err() {
                return;
            }
            let _ = host_tx.send((
                String::from_utf8_lossy(&user).into_owned(),
                String::from_utf8_lossy(&host).into_owned(),
                u16::from_be_bytes(port),
            ));
            let _ = stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
        }
    });
    for relay in [
        "wss://socks-target.example",
        "wss://socks-target.example/path",
    ] {
        let _ = connect_relay(
            relay,
            Instant::now() + Duration::from_secs(2),
            None,
            true,
            RelaySocks::Proxy(proxy),
        );
        let (user, host, port) = host_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("socks handshake");
        assert_eq!(user, relay);
        assert_eq!(host, "socks-target.example");
        assert_eq!(port, 443);
    }
}

#[test]
fn relay_assignment_follows_the_circuit_and_its_age() {
    assert!(relay_needs_assignment(None, Some(false)));
    assert!(!relay_needs_assignment(None, Some(true)));
    assert!(relay_needs_assignment(None, None));
    assert!(!relay_needs_assignment(
        Some(Duration::from_secs(10)),
        Some(false)
    ));
    assert!(relay_needs_assignment(
        Some(Duration::from_secs(30)),
        Some(false)
    ));
    assert!(!relay_needs_assignment(Some(Duration::from_secs(30)), None));
    assert!(!relay_needs_assignment(
        Some(Duration::from_secs(7 * 60)),
        Some(true)
    ));
    assert!(relay_needs_assignment(
        Some(Duration::from_secs(8 * 60)),
        Some(true)
    ));
}

#[test]
fn a_missing_circuit_waits_longer_after_each_miss() {
    assert!(!assignment_due(
        Some(Duration::from_secs(30)),
        Some(false),
        2
    ));
    assert!(assignment_due(
        Some(Duration::from_secs(60)),
        Some(false),
        2
    ));
    assert!(!assignment_due(
        Some(Duration::from_secs(3 * 60)),
        Some(false),
        4
    ));
    assert!(assignment_due(
        Some(Duration::from_secs(4 * 60)),
        Some(false),
        4
    ));
    assert!(!assignment_due(
        Some(Duration::from_secs(7 * 60)),
        Some(false),
        5
    ));
    assert!(assignment_due(
        Some(Duration::from_secs(8 * 60)),
        Some(false),
        9
    ));
    assert_eq!(next_misses(None, Some(false)), 1);
    assert_eq!(next_misses(Some(2), Some(false)), 3);
    assert_eq!(next_misses(Some(4), Some(true)), 1);
    assert_eq!(next_misses(Some(4), None), 4);
}

#[test]
fn a_second_miss_is_not_retried_after_thirty_seconds() {
    let pass = Arc::new(RelayPass::new());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&opened);
    let open: OpenRelay = Arc::new(move |relay: &str, _: Duration| {
        log.lock().expect("opens").push(relay.to_string());
    });
    let relay = "wss://dead.example";
    pass.mark_requested(
        relay,
        Instant::now().checked_sub(Duration::from_secs(30)).unwrap(),
        2,
    );
    pass.submit(
        vec![relay.to_string()],
        Vec::new(),
        Duration::from_secs(30),
        Arc::clone(&open),
        Arc::new(|| Some(HashMap::new())),
    );
    wait_until(|| pass.settled());
    assert!(opened.lock().expect("opens").is_empty());
    assert_eq!(pass.misses(relay), Some(2));

    pass.mark_requested(
        relay,
        Instant::now().checked_sub(Duration::from_secs(60)).unwrap(),
        2,
    );
    pass.submit(
        vec![relay.to_string()],
        Vec::new(),
        Duration::from_secs(30),
        open,
        Arc::new(|| Some(HashMap::new())),
    );
    wait_until(|| pass.settled());
    assert_eq!(
        opened.lock().expect("opens").clone(),
        vec![relay.to_string()]
    );
    assert_eq!(pass.misses(relay), Some(3));
}

#[test]
fn relays_needing_assignment_keep_a_young_circuit_and_retry_a_missing_one() {
    let relays = vec![
        "wss://Damus.io".to_string(),
        "wss://damus.io/path".to_string(),
        "wss://fresh.example".to_string(),
        "wss://old.example".to_string(),
    ];
    let mut paths = HashMap::new();
    paths.insert(
        "wss://damus.io".to_string(),
        "guard → middle → exit".to_string(),
    );
    paths.insert("wss://old.example".to_string(), "a → b → c".to_string());
    let mut requested = HashMap::new();
    requested.insert("wss://Damus.io".to_string(), (Duration::from_secs(60), 1));
    requested.insert(
        "wss://damus.io/path".to_string(),
        (Duration::from_secs(60), 1),
    );
    requested.insert(
        "wss://fresh.example".to_string(),
        (Duration::from_secs(10), 1),
    );
    requested.insert(
        "wss://old.example".to_string(),
        (Duration::from_secs(8 * 60), 1),
    );
    assert_eq!(
        relays_needing_assignment(&relays, Some(&paths), &requested),
        vec![
            "wss://damus.io/path".to_string(),
            "wss://old.example".to_string(),
        ]
    );
    assert_eq!(
        relays_needing_assignment(&relays, None, &requested),
        vec!["wss://old.example".to_string()]
    );
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct Unblock(Option<mpsc::Sender<()>>);

impl Unblock {
    fn fire(mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

impl Drop for Unblock {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[test]
fn assignment_pass_opens_relays_the_decision_still_needs() {
    let pass = Arc::new(RelayPass::new());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&opened);
    let open: OpenRelay = Arc::new(move |relay: &str, timeout: Duration| {
        assert_eq!(timeout, Duration::from_secs(30));
        log.lock().expect("opens").push(relay.to_string());
    });
    let paths = HashMap::from([
        ("wss://damus.io".to_string(), "g → m → e".to_string()),
        ("wss://old.example".to_string(), "a → b → c".to_string()),
    ]);
    let circuits: ReadCircuits = Arc::new(move || Some(paths.clone()));
    pass.mark_requested(
        "wss://fresh.example",
        Instant::now().checked_sub(Duration::from_secs(10)).unwrap(),
        1,
    );
    pass.mark_requested(
        "wss://damus.io",
        Instant::now().checked_sub(Duration::from_secs(60)).unwrap(),
        1,
    );
    pass.mark_requested(
        "wss://old.example",
        Instant::now()
            .checked_sub(Duration::from_secs(8 * 60))
            .unwrap(),
        1,
    );
    let page = vec![
        "wss://Damus.io".to_string(),
        "wss://damus.io/path".to_string(),
        "wss://fresh.example".to_string(),
        "wss://old.example".to_string(),
        "wss://127.0.0.1".to_string(),
    ];
    pass.submit(
        page.clone(),
        Vec::new(),
        Duration::from_secs(30),
        Arc::clone(&open),
        circuits,
    );
    wait_until(|| pass.settled());
    assert_eq!(
        opened.lock().expect("opens").clone(),
        vec![
            "wss://damus.io/path".to_string(),
            "wss://old.example".to_string(),
        ]
    );

    let mut again = page;
    again.push("wss://new.example".to_string());
    pass.submit(
        again,
        Vec::new(),
        Duration::from_secs(30),
        open,
        Arc::new(|| None),
    );
    wait_until(|| pass.settled());
    assert_eq!(
        opened.lock().expect("opens").clone(),
        vec![
            "wss://damus.io/path".to_string(),
            "wss://old.example".to_string(),
            "wss://new.example".to_string(),
        ]
    );
}

#[test]
fn a_running_pass_keeps_only_the_newest_relay_list() {
    let pass = Arc::new(RelayPass::new());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&opened);
    let calls = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let unblock = Unblock(Some(release_tx));
    let open: OpenRelay = Arc::new(move |relay: &str, _: Duration| {
        log.lock().expect("opens").push(relay.to_string());
        if calls.fetch_add(1, Ordering::AcqRel) == 0 {
            let _ = release_rx.lock().expect("release").recv();
        }
    });
    let circuits: ReadCircuits = Arc::new(|| Some(HashMap::new()));
    pass.submit(
        vec!["wss://first.example".to_string()],
        Vec::new(),
        Duration::from_secs(30),
        Arc::clone(&open),
        Arc::clone(&circuits),
    );
    wait_until(|| opened.lock().expect("opens").len() == 1);
    pass.submit(
        vec!["wss://second.example".to_string()],
        Vec::new(),
        Duration::from_secs(30),
        Arc::clone(&open),
        Arc::clone(&circuits),
    );
    pass.submit(
        vec!["wss://third.example".to_string()],
        Vec::new(),
        Duration::from_secs(30),
        open,
        circuits,
    );
    unblock.fire();
    wait_until(|| pass.settled() && opened.lock().expect("opens").len() == 2);
    assert_eq!(
        opened.lock().expect("opens").clone(),
        vec![
            "wss://first.example".to_string(),
            "wss://third.example".to_string(),
        ]
    );
}

#[test]
fn held_handshakes_expire_and_keep_only_the_newest() {
    let now = Instant::now();
    let ttl = Duration::from_secs(45);
    let fresh = |secs| now.checked_sub(Duration::from_secs(secs)).unwrap();
    let entries = vec![
        ("stale".to_string(), fresh(46)),
        ("older".to_string(), fresh(5)),
        ("newer".to_string(), fresh(1)),
    ];
    let mut dropped = held_keys_to_drop(&entries, now, ttl, 16);
    dropped.sort();
    assert_eq!(dropped, vec!["stale".to_string()]);

    let mut crowded = Vec::new();
    for index in 0..HELD_RELAY_LIMIT + 2 {
        crowded.push((format!("wss://held{index}.example"), fresh(index as u64)));
    }
    let dropped = held_keys_to_drop(&crowded, now, ttl, HELD_RELAY_LIMIT);
    assert_eq!(
        dropped,
        vec![
            format!("wss://held{}.example", HELD_RELAY_LIMIT + 1),
            format!("wss://held{}.example", HELD_RELAY_LIMIT),
        ]
    );
}

#[test]
fn assignment_pass_forgets_relays_left_off_the_list() {
    let pass = Arc::new(RelayPass::new());
    pass.mark_requested("wss://gone.example", Instant::now(), 1);
    let open: OpenRelay = Arc::new(|_: &str, _: Duration| {});
    pass.submit(
        vec!["wss://kept.example".to_string()],
        Vec::new(),
        Duration::from_secs(30),
        open,
        Arc::new(|| None),
    );
    wait_until(|| pass.settled());
    assert!(!pass.remembers("wss://gone.example"));
    assert!(pass.remembers("wss://kept.example"));
}

#[test]
fn assignment_pass_stops_dialing_once_the_budget_is_spent() {
    let pass = Arc::new(RelayPass::new());
    let now = Instant::now();
    for _ in 0..ASSIGN_LIMIT {
        assert!(pass.allow_assignment(now));
    }
    assert!(!pass.allow_assignment(now));

    let opened = Arc::new(Mutex::new(0usize));
    let count = Arc::clone(&opened);
    let open: OpenRelay = Arc::new(move |_: &str, _: Duration| {
        *count.lock().expect("opens") += 1;
    });
    pass.submit(
        vec!["wss://budget.example".to_string()],
        Vec::new(),
        Duration::from_secs(30),
        open,
        Arc::new(|| None),
    );
    wait_until(|| pass.settled());
    assert_eq!(*opened.lock().expect("opens"), 0);
    assert!(pass.allow_assignment(now + ASSIGN_WINDOW));
}

#[test]
fn direct_relays_are_not_assigned_circuits() {
    for setting in [RelaySocks::Direct, RelaySocks::Disabled] {
        let started = Instant::now();
        schedule_relay_assignments_via(
            vec!["wss://direct.example".to_string()],
            Vec::new(),
            Duration::from_secs(30),
            setting,
        );
        assert!(started.elapsed() < Duration::from_millis(50));
    }
    assert!(shared_relay_pass().settled());
}

#[test]
fn assign_relay_does_not_dial_without_a_proxy() {
    let started = Instant::now();
    let error = assign_relay(
        "wss://warm.example",
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Direct,
    )
    .unwrap_err();
    assert_eq!(error, SOCKS_MISCONFIGURED);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn assign_relay_does_not_dial_a_private_relay() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = listener.local_addr().unwrap();
    let error = assign_relay(
        "wss://127.0.0.1",
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    )
    .unwrap_err();
    assert_eq!(error, "relay address is not public");
    assert!(listener.accept().is_err());
}

#[test]
fn the_production_connector_rejects_the_test_certificate() {
    let identity = native_tls::Identity::from_pkcs12(include_bytes!("warm.p12"), "test").unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let _ = acceptor.accept(stream);
        }
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let connector = build_tls_connector().unwrap();
    let tcp = std::net::TcpStream::connect(address).unwrap();
    assert!(tls_handshake(&connector, "warm.example", tcp, deadline, None).is_err());

    let loose = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .unwrap();
    let tcp = std::net::TcpStream::connect(address).unwrap();
    assert!(tls_handshake(&loose, "warm.example", tcp, deadline, None).is_ok());
}

#[test]
fn assign_relay_opens_tls_through_socks_for_the_relay_url() {
    use_invalid_test_certs();
    let identity = native_tls::Identity::from_pkcs12(include_bytes!("warm.p12"), "test").unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let Some((user, host, port)) = read_socks_connect(&mut stream) else {
            return;
        };
        let tls_ok = acceptor.accept(stream).is_ok();
        let _ = seen_tx.send((user, host, port, tls_ok));
    });
    let relay = "wss://warm.example/path";
    let assigned = assign_relay(
        relay,
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    );
    let (user, host, port, tls_ok) = seen_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("socks handshake");
    assert_eq!(user, relay);
    assert_eq!(host, "warm.example");
    assert_eq!(port, 443);
    assert!(tls_ok);
    assert!(assigned.is_ok());
    assert!(take_held(relay).is_some());
    assert!(take_held(relay).is_none());
}

#[test]
fn fetch_nip11_reads_the_document_over_socks_and_classifies_it() {
    let open = include_str!("nip11/open.json");
    let (body, request) =
        fetch_saved_nip11("wss://warm.example/path", &nip11_response("200 OK", open));
    assert!(request.starts_with("GET /path HTTP/1.1\r\n"));
    assert!(request.contains("Host: warm.example\r\n"));
    assert!(request.contains("Accept: application/nostr+json\r\n"));
    assert!(!request.contains("Upgrade"));
    assert!(!request.contains("websocket"));
    assert_eq!(body.as_deref(), Some(open));
    let document: serde_json::Value = serde_json::from_str(open).unwrap();
    assert!(crate::nip11::document_is_public(&document));

    let auth = include_str!("nip11/auth.json");
    let (body, _) = fetch_saved_nip11("wss://warm.example/path", &nip11_response("200 OK", auth));
    assert_eq!(body.as_deref(), Some(auth));
    let document: serde_json::Value = serde_json::from_str(auth).unwrap();
    assert!(!crate::nip11::document_is_public(&document));

    let (body, _) = fetch_saved_nip11(
        "wss://warm.example",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n",
    );
    assert_eq!(body.as_deref(), Some("{}"));
    let document: serde_json::Value = serde_json::from_str("{}").unwrap();
    assert!(crate::nip11::document_is_public(&document));
}

#[test]
fn fetch_nip11_misses_when_the_relay_sends_no_document() {
    let (body, request) = fetch_saved_nip11(
        "wss://warm.example",
        &nip11_response("404 Not Found", "missing"),
    );
    assert!(request.starts_with("GET / HTTP/1.1\r\n"));
    assert!(body.is_none());
    assert!(fetch_nip11(
        "wss://warm.example",
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Disabled,
    )
    .is_none());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = listener.local_addr().unwrap();
    assert!(fetch_nip11(
        "wss://127.0.0.1",
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    )
    .is_none());
    assert!(listener.accept().is_err());

    let huge = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        NIP11_BODY_MAX + 1
    );
    assert!(decode_http_message(huge.as_bytes(), false).is_err());
    assert!(
        decode_http_message(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{", false)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        decode_http_message(b"HTTP/1.1 200 OK\r\n\r\n{}", true)
            .unwrap()
            .as_deref(),
        Some("{}")
    );
}

#[test]
fn the_list_request_asks_for_kind_10002() {
    let payload = list_req("list");
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value[0], "REQ");
    assert_eq!(value[1], "list");
    assert_eq!(value[2]["kinds"][0], 10002);
    assert_eq!(value[2]["limit"], 50);
    assert!(value[2].get("ids").is_none());
    assert!(value[2].get("authors").is_none());
}

#[test]
fn saved_event_and_eose_frames_fill_the_relay_list() {
    let mut read = ListRead::answered();
    assert!(!read.push(
        "list",
        r#"["EVENT","other",{"kind":10002,"tags":[["r","wss://other.example"]],"content":""}]"#
    ));
    assert!(read.urls.is_empty());
    assert!(!read.push("list", include_str!("list/10002-event.json")));
    assert_eq!(
        read.urls,
        vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
            "wss://relay.primal.net".to_string(),
        ]
    );
    assert!(!read.push("list", include_str!("list/10002-event.json")));
    assert_eq!(read.urls.len(), 3);
    assert!(read.push("list", include_str!("list/10002-eose.json")));
    let list = read.finish();
    assert!(list.socket_answered);
    assert_eq!(list.urls.len(), 3);
}

#[test]
fn an_auth_frame_leaves_the_answered_socket_empty() {
    let mut read = ListRead::answered();
    assert!(!read.push("list", include_str!("list/auth.json")));
    assert!(read.socket_answered);
    assert!(read.urls.is_empty());
    assert!(!read.finished);

    let mut closed = ListRead::answered();
    assert!(closed.push("list", r#"["CLOSED","list","auth-required: we need auth"]"#));
    let list = closed.finish();
    assert!(list.socket_answered);
    assert!(list.urls.is_empty());
}

#[test]
fn an_ok_frame_accepts_the_30323_probe() {
    let note = minimal_probe_note(1_700_000_000);
    let parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    assert_eq!(parsed["kind"], 30323);
    assert_eq!(parsed["content"], "");
    assert_eq!(parsed["tags"], serde_json::json!([]));
    let frame = format!(r#"["OK","{}",true,""]"#, note.id_hex());
    let probe = probe_from_ok(&frame, &note.id_hex()).unwrap();
    assert_eq!(probe.name, "30323");
    assert!(probe.accepted);
    assert_eq!(probe.detail, "OK");
}

#[test]
fn a_blocked_frame_refuses_the_30323_probe() {
    let note = minimal_probe_note(1_700_000_000);
    let frame = format!(
        r#"["OK","{}",false,"blocked: kind 30323 is not accepted"]"#,
        note.id_hex()
    );
    let probe = probe_from_ok(&frame, &note.id_hex()).unwrap();
    assert_eq!(probe.name, "30323");
    assert!(!probe.accepted);
    assert_eq!(probe.detail, "blocked: kind 30323 is not accepted");
}

#[test]
fn a_missing_or_closed_document_skips_the_probe() {
    let deadline = Instant::now();
    assert!(probe_kind("wss://warm.example", None, deadline).is_empty());
    let auth: serde_json::Value = serde_json::from_str(include_str!("nip11/auth.json")).unwrap();
    assert!(probe_kind("wss://warm.example", Some(&auth), deadline).is_empty());
    let kinds: serde_json::Value =
        serde_json::from_str(include_str!("nip11/kinds-without.json")).unwrap();
    assert!(probe_kind("wss://warm.example", Some(&kinds), deadline).is_empty());
}

#[test]
fn a_list_read_stays_unanswered_when_the_socket_never_opens() {
    let list = read_relay_list(
        "wss://warm.example",
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Disabled,
    );
    assert!(!list.socket_answered);
    assert!(list.urls.is_empty());
}

fn nip11_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/nostr+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn fetch_saved_nip11(relay: &str, response: &str) -> (Option<String>, String) {
    use_invalid_test_certs();
    let identity = native_tls::Identity::from_pkcs12(include_bytes!("warm.p12"), "test").unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let response = response.to_string();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        if read_socks_connect(&mut stream).is_none() {
            return;
        }
        let Ok(mut tls) = acceptor.accept(stream) else {
            return;
        };
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while request.len() < 8192 {
            if tls.read(&mut byte).ok().unwrap_or(0) == 0 {
                break;
            }
            request.push(byte[0]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let _ = seen_tx.send(String::from_utf8_lossy(&request).into_owned());
        let _ = tls.write_all(response.as_bytes());
        let _ = tls.flush();
    });
    let body = fetch_nip11(
        relay,
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    );
    let request = seen_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    (body, request)
}

fn read_socks_connect(stream: &mut std::net::TcpStream) -> Option<(String, String, u16)> {
    use std::io::{Read, Write};
    let mut hello = [0u8; 3];
    if stream.read_exact(&mut hello).is_err() || hello != [0x05, 0x01, 0x02] {
        return None;
    }
    if stream.write_all(&[0x05, 0x02]).is_err() {
        return None;
    }
    let mut ulen = [0u8; 2];
    if stream.read_exact(&mut ulen).is_err() || ulen[0] != 0x01 {
        return None;
    }
    let mut user = vec![0u8; ulen[1] as usize];
    if stream.read_exact(&mut user).is_err() {
        return None;
    }
    let mut plen = [0u8; 1];
    if stream.read_exact(&mut plen).is_err() || plen[0] != 0 {
        return None;
    }
    if stream.write_all(&[0x01, 0x00]).is_err() {
        return None;
    }
    let mut head = [0u8; 4];
    if stream.read_exact(&mut head).is_err() || head[3] != 0x03 {
        return None;
    }
    let mut len = [0u8; 1];
    if stream.read_exact(&mut len).is_err() {
        return None;
    }
    let mut host = vec![0u8; len[0] as usize];
    if stream.read_exact(&mut host).is_err() {
        return None;
    }
    let mut port = [0u8; 2];
    if stream.read_exact(&mut port).is_err()
        || stream
            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .is_err()
    {
        return None;
    }
    Some((
        String::from_utf8_lossy(&user).into_owned(),
        String::from_utf8_lossy(&host).into_owned(),
        u16::from_be_bytes(port),
    ))
}

fn park_held_relay(relay: &str) {
    use_invalid_test_certs();
    let identity = native_tls::Identity::from_pkcs12(include_bytes!("warm.p12"), "test").unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        if read_socks_connect(&mut stream).is_none() {
            return;
        }
        let _ = done_tx.send(acceptor.accept(stream).is_ok());
    });
    assign_relay(
        relay,
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    )
    .unwrap();
    assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
}

#[test]
fn a_parked_relay_upgrade_stops_at_the_connect_limit() {
    use_invalid_test_certs();
    let relay = "wss://stall-upgrade.example";
    let identity = native_tls::Identity::from_pkcs12(include_bytes!("warm.p12"), "test").unwrap();
    let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        if read_socks_connect(&mut stream).is_none() {
            return;
        }
        let Ok(tls) = acceptor.accept(stream) else {
            return;
        };
        let _ = ready_tx.send(());
        let _ = release_rx.recv_timeout(Duration::from_secs(20));
        drop(tls);
    });
    assign_relay(
        relay,
        Instant::now() + Duration::from_secs(2),
        RelaySocks::Proxy(proxy),
    )
    .unwrap();
    ready_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("parked tls");

    let started = Instant::now();
    let error = open_relay_socket(
        relay,
        Instant::now() + Duration::from_secs(30),
        true,
        RelaySocks::Disabled,
    )
    .unwrap_err();
    let elapsed = started.elapsed();
    let _ = release_tx.send(());
    assert_eq!(error, SOCKS_MISCONFIGURED);
    assert!(
        elapsed + Duration::from_secs(1) >= CONNECT_LIMIT,
        "upgrade returned in {elapsed:?}"
    );
    assert!(
        elapsed < CONNECT_LIMIT + Duration::from_secs(4),
        "upgrade waited {elapsed:?}"
    );
}

#[test]
fn a_closed_held_relay_falls_back_to_a_fresh_connect() {
    let relay = "wss://closed.example";
    park_held_relay(relay);
    let held = take_held(relay).expect("parked handshake");
    let _ = held.get_ref().shutdown(std::net::Shutdown::Both);
    store_held(relay, held);

    let fallback = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = fallback.local_addr().unwrap();
    let (saw_tx, saw_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if fallback.accept().is_ok() {
            let _ = saw_tx.send(());
        }
    });

    let error = open_relay_socket(
        relay,
        Instant::now() + Duration::from_secs(2),
        true,
        RelaySocks::Proxy(proxy),
    )
    .unwrap_err();
    assert_ne!(error, RELAY_TIMEOUT);
    saw_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("fresh connect");
    assert!(take_held(relay).is_none());
}

#[test]
fn publish_does_not_dial_a_private_relay() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let note = sign_sealed_note("body", "0123456789abcdef0123", 1_700_000_000);
    let started = Instant::now();
    let accepted = publish_to_relays(
        &[format!("wss://127.0.0.1:{port}")],
        &note,
        Duration::from_secs(2),
    );
    assert!(accepted.accepted.is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "private relay was dialed: {:?}",
        started.elapsed()
    );
    assert!(listener.accept().is_err());
}

#[test]
fn a_misconfigured_socks_proxy_does_not_dial() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let error = connect_relay(
        &format!("wss://127.0.0.1:{port}"),
        Instant::now() + Duration::from_secs(2),
        None,
        false,
        RelaySocks::Disabled,
    )
    .unwrap_err();
    assert_eq!(error, SOCKS_MISCONFIGURED);
    assert!(listener.accept().is_err());
}

#[test]
fn sealed_note_from_json_checks_signature_stamp_tags_locator_and_size() {
    let locator = "0123456789abcdef0123";
    let created_at = 1_700_000_000;
    let content = v2_payload(locator, 2);
    let note = sign_sealed_note(&content, locator, created_at);
    let value: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let (parsed, parsed_locator) = sealed_note_from_json(&value, 128, 32, 256_000).unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(parsed_locator, locator);
    let payload_max = crate::seal::sealed_payload_max(0, 0, 1);
    let over = sign_sealed_note(&"x".repeat(payload_max + 1), locator, created_at);
    let over_value: serde_json::Value = serde_json::from_str(&over.event_json).unwrap();
    assert!(sealed_note_from_json(&over_value, 0, 0, 1).is_none());
    assert!(sealed_note_from_json(&value, 0, 0, 1).is_some());
    let clear = sign_sealed_note("plaintext", locator, created_at);
    let clear_value: serde_json::Value = serde_json::from_str(&clear.event_json).unwrap();
    assert!(sealed_note_from_json(&clear_value, 128, 32, 256_000).is_none());

    let mut bad_sig = value.clone();
    let sig = bad_sig["sig"].as_str().unwrap().to_string();
    let flipped = if sig.starts_with('a') {
        format!("b{}", &sig[1..])
    } else {
        format!("a{}", &sig[1..])
    };
    bad_sig["sig"] = serde_json::Value::String(flipped);
    assert!(sealed_note_from_json(&bad_sig, 128, 32, 256_000).is_none());

    let keypair = Keypair::new(SECP256K1, &mut thread_rng());
    let kind = nonograph_nip44::KIND;
    let weak = signed_without_enough_zero_bits(&keypair, created_at, kind, locator, &content);
    let weak_value: serde_json::Value = serde_json::from_str(&weak.event_json).unwrap();
    assert!(sealed_note_from_json(&weak_value, 128, 32, 256_000).is_none());

    let titled = signed_event(
        &keypair,
        created_at,
        kind,
        &[
            vec!["d".to_string(), locator.to_string()],
            vec![
                "nonce".to_string(),
                "1".to_string(),
                nonograph_nip44::POW_BITS.to_string(),
            ],
            vec!["title".to_string(), "Visible".to_string()],
        ],
        &content,
    );
    let titled_value: serde_json::Value = serde_json::from_str(&titled.event_json).unwrap();
    assert!(sealed_note_from_json(&titled_value, 128, 32, 256_000).is_none());

    let uppercase = signed_event(
        &keypair,
        created_at,
        kind,
        &crate::seal::sealed_tags("0123456789ABCDEF0123", "1"),
        &content,
    );
    let uppercase_value: serde_json::Value = serde_json::from_str(&uppercase.event_json).unwrap();
    assert!(sealed_note_from_json(&uppercase_value, 128, 32, 256_000).is_none());
}

fn signed_without_enough_zero_bits(
    keypair: &Keypair,
    created_at: i64,
    kind: u32,
    locator: &str,
    content: &str,
) -> SignedNote {
    let pubkey_hex = hex_encode(&keypair.x_only_public_key().0.serialize());
    for nonce in 0..1_000_000u32 {
        let tags = crate::seal::sealed_tags(locator, &nonce.to_string());
        let id = nonograph_nip44::event_id(&pubkey_hex, created_at, kind, &tags, content);
        if matches!(
            nonograph_nip44::check_stamp(&id, &pubkey_hex, created_at, kind, &tags, content),
            Err(nonograph_nip44::Error::Stamp)
        ) {
            return signed_event(keypair, created_at, kind, &tags, content);
        }
    }
    panic!("no weak stamp in range");
}

#[test]
fn publish_waits_for_every_relay_it_will_name() {
    let slow = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let slow_port = slow.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = slow.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut buf = [0u8; 64];
        loop {
            match std::io::Read::read(&mut stream, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    let refused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let refused_port = refused.local_addr().unwrap().port();
    drop(refused);

    let note = sign_sealed_note("body", "0123456789abcdef0123", 1_700_000_000);
    let started = Instant::now();
    let accepted = publish_to_relays_inner(
        &[
            format!("wss://127.0.0.1:{refused_port}"),
            format!("wss://127.0.0.1:{slow_port}"),
        ],
        &note,
        Duration::from_millis(200),
        false,
    );
    let elapsed = started.elapsed();
    assert!(accepted.accepted.is_empty());
    assert!(accepted.refused.is_empty());
    assert!(
        elapsed >= Duration::from_millis(150),
        "returned when the first relay failed: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "waited longer than one deadline: {elapsed:?}"
    );
}

#[test]
fn cancel_stops_a_relay_before_the_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let _ = accepted_tx.send(());
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut buf = [0u8; 64];
        loop {
            match std::io::Read::read(&mut stream, &mut buf) {
                Ok(0) | Err(_) => {
                    let _ = closed_tx.send(());
                    break;
                }
                Ok(_) => {}
            }
        }
    });

    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let handle = std::thread::spawn(move || {
        connect_relay(
            &format!("wss://127.0.0.1:{port}"),
            Instant::now() + Duration::from_secs(5),
            Some(&flag),
            false,
            RelaySocks::Direct,
        )
    });
    accepted_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("relay did not accept");
    cancel.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let error = handle.join().unwrap().unwrap_err();
    let elapsed = started.elapsed();
    assert_eq!(error, RELAY_TIMEOUT);
    assert!(
        elapsed < Duration::from_millis(500),
        "cancel left the relay running: {elapsed:?}"
    );
    assert!(
        closed_rx.recv_timeout(Duration::from_millis(400)).is_ok(),
        "fetch thread still held the relay socket"
    );
}

#[test]
fn a_name_is_not_resolved_after_the_deadline() {
    let started = Instant::now();
    let error = resolve_host("relay.example", 443, Instant::now(), None).unwrap_err();
    assert_eq!(error, RELAY_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn a_cancelled_name_is_not_resolved() {
    let cancel = Arc::new(AtomicBool::new(true));
    let started = Instant::now();
    let error = resolve_host(
        "relay.example",
        443,
        Instant::now() + Duration::from_secs(5),
        Some(&cancel),
    )
    .unwrap_err();
    assert_eq!(error, RELAY_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn the_system_resolver_answers_localhost() {
    let addresses = resolve_host(
        "localhost",
        9,
        Instant::now() + Duration::from_secs(2),
        None,
    )
    .unwrap();
    assert!(addresses.iter().any(|address| address.ip().is_loopback()));
    assert!(addresses.iter().all(|address| address.port() == 9));
}

#[test]
fn public_relay_urls_reject_loopback_and_private_hosts() {
    assert!(public_relay_url("wss://relay.damus.io"));
    assert!(public_relay_url("wss://nos.lol/"));
    assert!(public_relay_url("wss://relay.damus.io:443"));
    assert!(!public_relay_url("ws://relay.damus.io"));
    assert!(!public_relay_url("https://relay.damus.io"));
    assert!(!public_relay_url("wss://localhost"));
    assert!(!public_relay_url("wss://127.0.0.1"));
    assert!(!public_relay_url("wss://10.0.0.1"));
    assert!(!public_relay_url("wss://192.168.1.1"));
    assert!(!public_relay_url("wss://169.254.169.254"));
    assert!(!public_relay_url("wss://100.64.0.1"));
    assert!(!public_relay_url("wss://224.0.0.1"));
    assert!(!public_relay_url("wss://[::1]"));
    assert!(!public_relay_url("wss://[::ffff:127.0.0.1]"));
    assert!(!public_relay_url("wss://[64:ff9b::a00:1]"));
    assert!(!public_relay_url("wss://[ff02::1]"));
    assert!(!public_relay_url("wss://user:pass@relay.damus.io"));
    assert!(!public_relay_url("wss://relay.damus.io:4444"));
    assert_eq!(
        canonical_public_relay("wss://Relay.Damus.io/"),
        Some("wss://relay.damus.io".to_string())
    );
    assert_eq!(
        canonical_public_relay("wss://relay.damus.io:443"),
        Some("wss://relay.damus.io".to_string())
    );
    assert_eq!(
        canonical_public_relay("wss://relay.damus.io./path/"),
        Some("wss://relay.damus.io/path".to_string())
    );
    assert_eq!(
        canonical_public_relay("  wss://nos.lol/  "),
        Some("wss://nos.lol".to_string())
    );
    assert!(canonical_public_relay("wss://127.0.0.1").is_none());
    assert!(same_public_relay(
        "wss://Relay.Primal.net/",
        "wss://relay.primal.net"
    ));
    assert!(!same_public_relay("wss://relay.damus.io", "wss://nos.lol"));
}

#[test]
fn loopback_is_not_a_public_relay_address() {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 443));
    assert_eq!(
        public_relay_addresses(vec![loopback]).unwrap_err(),
        "relay address is not public"
    );
    let public = SocketAddr::from(([1, 1, 1, 1], 443));
    assert_eq!(
        public_relay_addresses(vec![public, loopback]).unwrap(),
        vec![public]
    );
    let cgnat = SocketAddr::from(([100, 64, 0, 1], 443));
    let multicast = SocketAddr::from(([224, 0, 0, 1], 443));
    let wrong_port = SocketAddr::from(([1, 1, 1, 1], 80));
    let nat64 = SocketAddr::from((
        std::net::Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0x0a00, 1),
        443,
    ));
    assert_eq!(
        public_relay_addresses(vec![cgnat, multicast, wrong_port, nat64]).unwrap_err(),
        "relay address is not public"
    );
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
    let recomputed = nonograph_nip44::event_id(
        parsed["pubkey"].as_str().unwrap(),
        parsed["created_at"].as_i64().unwrap(),
        parsed["kind"].as_u64().unwrap() as u32,
        &tags,
        parsed["content"].as_str().unwrap(),
    );
    assert_eq!(recomputed, note.id);
    secp.verify_schnorr(&signature, &Message::from_digest(note.id), &pubkey)
        .unwrap();
}

#[test]
fn bip340_vector0_matches_libsecp() {
    let mut secret = [0u8; 32];
    secret[31] = 3;
    let keypair = Keypair::from_seckey_slice(SECP256K1, &secret).unwrap();
    let sig = SECP256K1.sign_schnorr_with_aux_rand(
        &Message::from_digest([0u8; 32]),
        &keypair,
        &[0u8; 32],
    );
    assert_eq!(
        hex_encode(&sig.serialize()),
        "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
    );
}

#[test]
fn tab_long_form_signature_verifies() {
    let mut secret = [0u8; 32];
    secret[31] = 3;
    let keypair = Keypair::from_seckey_slice(SECP256K1, &secret).unwrap();
    let created_at = 1_700_000_000;
    let content = "hello from the tab\nwith \"quotes\"";
    let tags = vec![
        vec!["d".to_string(), "4a-tab-sign".to_string()],
        vec!["title".to_string(), "Hello".to_string()],
        vec!["published_at".to_string(), created_at.to_string()],
        vec!["author".to_string(), "Ada".to_string()],
    ];
    let pubkey = keypair.x_only_public_key().0.serialize();
    let pubkey_hex = hex_encode(&pubkey);
    let id = nonograph_nip44::event_id(&pubkey_hex, created_at, 30023, &tags, content);
    let sig = SECP256K1.sign_schnorr_with_aux_rand(&Message::from_digest(id), &keypair, &[0u8; 32]);
    let json = event_wire(
        &hex_encode(&id),
        &pubkey_hex,
        created_at,
        30023,
        &tags,
        content,
        Some(&hex_encode(&sig.serialize())),
    );
    let parsed = parse_event(&serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(parsed.kind, 30023);
    assert!(event_id_matches(&parsed));
    assert!(verify_sig(&parsed));
    assert_eq!(
        pubkey_hex,
        "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"
    );
}

#[test]
fn homepage_js_signed_note_verifies() {
    let json = r#"{"id":"6d8323bd8e3e7824bf16f7983288645b2a476bcd4d6db2958edd73455b2a787b","pubkey":"f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9","created_at":1700000000,"kind":30023,"tags":[["d","4a-tab-sign"],["title","Hello"],["published_at","1700000000"],["author","Ada"]],"content":"hello from the tab\nwith \"quotes\"","sig":"148d67d68dd5079295caa75e797894f9a8654499a3316b9849c672f69fb283b75d01374033b5c6c7dd4822fd576a8f44a9e7fa0e6ede5ed160960054ba5f5364"}"#;
    let parsed = parse_event(&serde_json::from_str(json).unwrap()).unwrap();
    assert_eq!(parsed.kind, 30023);
    assert!(event_id_matches(&parsed));
    assert!(verify_sig(&parsed));
}

fn event_id_matches(parsed: &ParsedEvent) -> bool {
    nonograph_nip44::event_id(
        &parsed.pubkey_hex,
        parsed.created_at,
        parsed.kind,
        &parsed.tags,
        &parsed.content,
    ) == parsed.id
}
