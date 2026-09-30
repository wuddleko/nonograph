use super::*;
use secp256k1::{Secp256k1, XOnlyPublicKey};

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
fn test_gift_wrap_hides_title_alias_and_text() {
    let title = "relay-hidden-title";
    let alias = "relay-hidden-alias";
    let text = "relay-hidden-text";
    let created_at = 1_700_000_000;
    let before = now_secs();
    let wrapped = wrap_note(title, alias, text, created_at).unwrap();
    let after = now_secs();
    let payload = format!("[\"EVENT\",{}]", wrapped.event_json);
    assert!(!payload.contains(title));
    assert!(!payload.contains(alias));
    assert!(!payload.contains(text));
    assert!(!payload.contains(&hex_encode(&wrapped.recipient_secret)));

    let parsed: serde_json::Value = serde_json::from_str(&wrapped.event_json).unwrap();
    assert_eq!(parsed["kind"], KIND_GIFT_WRAP);
    assert_eq!(parsed["tags"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["tags"][0][0], "p");
    assert_ne!(parsed["pubkey"], parsed["tags"][0][1]);
    let wrap_time = parsed["created_at"].as_i64().unwrap();
    assert!(wrap_time <= after);
    assert!(wrap_time >= before.saturating_sub(TWO_DAYS_SECS));
    assert_signature(&SignedNote {
        id: wrapped.id,
        pubkey: decode_fixed_hex(parsed["pubkey"].as_str().unwrap()).unwrap(),
        event_json: wrapped.event_json.clone(),
    });

    let opened = open_wrapped_note(&wrapped.event_json, &wrapped.recipient_secret).unwrap();
    assert_eq!(opened.title, title);
    assert_eq!(opened.author, alias);
    assert_eq!(opened.content, text);
    assert_eq!(opened.created_at, created_at);

    let clear = format!(
        "[\"EVENT\",{}]",
        sign_note(title, alias, text, created_at).event_json
    );
    assert!(clear.contains(title));
    assert!(clear.contains(alias));
    assert!(clear.contains(text));
}

#[test]
fn test_opened_note_matches_fetch_title_and_published_at() {
    let rumor_created_at = 1_700_000_000;
    let published_at = 1_800_000_000;
    let tags = vec![
        vec!["d".to_string(), "note".to_string()],
        vec!["published_at".to_string(), published_at.to_string()],
    ];
    let wrapped = wrap_custom_rumor(&tags, "body", rumor_created_at);
    let opened = open_wrapped_note(&wrapped.event_json, &wrapped.recipient_secret).unwrap();

    let signed = signed_event(
        &Keypair::new(&Secp256k1::new(), &mut thread_rng()),
        rumor_created_at,
        KIND_LONG_FORM,
        &tags,
        "body",
    );
    let fetched = note_from_value(
        &serde_json::from_str(&signed.event_json).unwrap(),
        &signed.id_hex(),
    )
    .unwrap();
    assert_eq!(opened.title, "Untitled");
    assert_eq!(opened.author, "");
    assert_eq!(opened.content, "body");
    assert_eq!(opened.created_at, published_at);
    assert_eq!(opened.title, fetched.title);
    assert_eq!(opened.author, fetched.author);
    assert_eq!(opened.content, fetched.content);
    assert_eq!(opened.created_at, fetched.created_at);
}

#[test]
fn test_nevent_round_trip_carries_id_and_relays() {
    let note = sign_note("Title", "", "body", 1_700_000_000);
    let relays = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ];
    let nevent = encode_nevent(&note.id, &relays, &note.pubkey, KIND_LONG_FORM);
    let decoded = decode_nevent(&nevent).unwrap();
    assert_eq!(decoded.event_id_hex, hex_encode(&note.id));
    assert_eq!(decoded.relays, relays);
    assert!(decode_nevent("about").is_none());
    assert!(decode_nevent("nevent1qqqq").is_none());

    let wrapped = wrap_note("Title", "", "body", 1_700_000_000).unwrap();
    let wrap_nevent = encode_nevent(&wrapped.id, &relays, &wrapped.pubkey, KIND_GIFT_WRAP);
    let decoded_wrap = decode_nevent(&wrap_nevent).unwrap();
    assert_eq!(decoded_wrap.event_id_hex, wrapped.id_hex());
    let nsec = encode_nsec(&wrapped.recipient_secret);
    assert_eq!(decode_nsec(&nsec), Some(wrapped.recipient_secret));
    assert!(decode_nsec("nsec1qqqq").is_none());
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
    let fetched = note_from_relay_message(&message, &note.id_hex(), None).unwrap();
    assert_eq!(fetched.id_hex, note.id_hex());
    assert_eq!(fetched.title, "Hello");
    assert_eq!(fetched.author, "Ada");
    assert_eq!(fetched.content, "a line\nwith \"quotes\" and \\slashes");
    assert_eq!(fetched.created_at, 1_700_000_000);
    assert!(relay_has_no_event(r#"["EOSE","sub"]"#, "sub"));
    assert!(!relay_has_no_event(r#"["EOSE","other"]"#, "sub"));
    assert!(note_from_relay_message(r#"["EOSE","sub"]"#, &note.id_hex(), None).is_none());
}

#[test]
fn test_fetched_wrap_opens_with_nsec_and_ignores_without() {
    let wrapped = wrap_note("Hello", "Ada", "secret body", 1_700_000_000).unwrap();
    let message = format!(r#"["EVENT","sub",{}]"#, wrapped.event_json);
    assert!(note_from_relay_message(&message, &wrapped.id_hex(), None).is_none());
    let fetched =
        note_from_relay_message(&message, &wrapped.id_hex(), Some(&wrapped.recipient_secret))
            .unwrap();
    assert_eq!(fetched.id_hex, wrapped.id_hex());
    assert_eq!(fetched.title, "Hello");
    assert_eq!(fetched.author, "Ada");
    assert_eq!(fetched.content, "secret body");
    assert_eq!(fetched.created_at, 1_700_000_000);
    assert!(
        note_from_relay_message(&message, &"ab".repeat(32), Some(&wrapped.recipient_secret))
            .is_none()
    );
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
    assert!(fetch_note(&[], &id, Duration::from_millis(20), None).is_none());
    assert!(fetch_note(
        &["https://relay.example".to_string()],
        &id,
        Duration::from_millis(20),
        None
    )
    .is_none());
    assert!(fetch_note(
        &["wss://relay.example".to_string()],
        "abcd",
        Duration::from_millis(20),
        None
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

fn wrap_custom_rumor(tags: &[Vec<String>], content: &str, created_at: i64) -> WrappedNote {
    let secp = Secp256k1::new();
    let author_key = Keypair::new(&secp, &mut thread_rng());
    let recipient_key = Keypair::new(&secp, &mut thread_rng());
    let wrap_key = Keypair::new(&secp, &mut thread_rng());
    let recipient_secret = recipient_key.secret_bytes();
    let recipient_pubkey = recipient_key.x_only_public_key().0.serialize();
    let rumor = rumor_json(&author_key, created_at, KIND_LONG_FORM, tags, content);
    let seal_content =
        nip44_encrypt(&rumor, &author_key.secret_bytes(), &recipient_pubkey).unwrap();
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
    )
    .unwrap();
    let wrap = signed_event(
        &wrap_key,
        random_past(now_secs()),
        KIND_GIFT_WRAP,
        &[vec![
            "p".to_string(),
            hex_encode(&recipient_pubkey).to_ascii_uppercase(),
        ]],
        &wrap_content,
    );
    WrappedNote {
        id: wrap.id,
        pubkey: wrap.pubkey,
        event_json: wrap.event_json,
        recipient_secret,
    }
}

#[test]
fn a_relay_does_not_connect_after_its_deadline() {
    let started = Instant::now();
    let error = connect_relay("wss://127.0.0.1:9", Instant::now(), None).unwrap_err();
    assert_eq!(error, RELAY_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(50));
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

    let note = sign_note("Title", "Ada", "body", 1_700_000_000);
    let started = Instant::now();
    let accepted = publish_to_relays(
        &[
            format!("wss://127.0.0.1:{refused_port}"),
            format!("wss://127.0.0.1:{slow_port}"),
        ],
        &note,
        Duration::from_millis(200),
    );
    let elapsed = started.elapsed();
    assert!(accepted.is_empty());
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
fn fetch_stops_threads_at_the_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (closed_tx, closed_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
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

    let started = Instant::now();
    let note = fetch_note(
        &[format!("wss://127.0.0.1:{port}")],
        &"cd".repeat(32),
        Duration::from_millis(200),
        None,
    );
    let elapsed = started.elapsed();
    assert!(note.is_none());
    assert!(
        elapsed < Duration::from_millis(700),
        "ran past one deadline: {elapsed:?}"
    );
    assert!(
        closed_rx.recv_timeout(Duration::from_millis(400)).is_ok(),
        "fetch thread still held the relay socket"
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
