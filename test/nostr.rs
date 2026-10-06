use super::*;
use secp256k1::{Secp256k1, XOnlyPublicKey};
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
    assert_eq!(decoded.kind, Some(KIND_LONG_FORM));
    assert!(decode_nevent("about").is_none());
    assert!(decode_nevent("nevent1qqqq").is_none());

    let wrapped = wrap_note("Title", "", "body", 1_700_000_000).unwrap();
    let wrap_nevent = encode_nevent(&wrapped.id, &relays, &wrapped.pubkey, KIND_GIFT_WRAP);
    let decoded_wrap = decode_nevent(&wrap_nevent).unwrap();
    assert_eq!(decoded_wrap.event_id_hex, wrapped.id_hex());
    assert_eq!(decoded_wrap.kind, Some(KIND_GIFT_WRAP));
    let nsec = encode_nsec(&wrapped.recipient_secret);
    assert_eq!(decode_nsec(&nsec), Some(wrapped.recipient_secret));
    assert!(decode_nsec("nsec1qqqq").is_none());
}

#[test]
fn tab_encode_nevent_matches_the_server() {
    let id = [0u8; 32];
    let pubkey =
        decode_fixed_hex::<32>("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9")
            .unwrap();
    let relays = vec!["wss://relay.damus.io".to_string()];
    let nevent = encode_nevent(&id, &relays, &pubkey, KIND_LONG_FORM);
    assert_eq!(
        nevent,
        "nevent1qqsqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqpz3mhxue69uhhyetvv9ujuerpd46hxtnfdupzp7fs3gqeykxrzpyngnu9lzw4y2d4x8yytqm0nxcgvq03zw7wqdheqvzqqqr4gutzxp9l"
    );
    let decoded = decode_nevent(&nevent).unwrap();
    assert_eq!(decoded.event_id_hex, hex_encode(&id));
    assert_eq!(decoded.relays, relays);
    assert_eq!(decoded.kind, Some(KIND_LONG_FORM));
}

fn d_tag(note: &SignedNote) -> String {
    let parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    parsed["tags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tag| tag[0] == "d")
        .unwrap()[1]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn test_naddr_round_trip_carries_identifier_and_relays() {
    let note = sign_note("Title", "Ada", "body", 1_700_000_000);
    let d = d_tag(&note);
    let relays = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ];
    let encoded = encode_naddr(&d, &relays, &note.pubkey, KIND_LONG_FORM);
    let decoded = decode_naddr(&encoded).unwrap();
    assert_eq!(decoded.identifier, d);
    assert_eq!(decoded.pubkey, note.pubkey);
    assert_eq!(decoded.kind, KIND_LONG_FORM);
    assert_eq!(decoded.relays, relays);
    assert!(decode_naddr("about").is_none());
    assert!(decode_naddr("naddr1qqqq").is_none());
    assert!(decode_naddr(&encode_nevent(
        &note.id,
        &relays,
        &note.pubkey,
        KIND_LONG_FORM
    ))
    .is_none());
    assert!(decode_nevent(&encoded).is_none());
}

#[test]
fn test_naddr_note_must_match_author_and_d() {
    let note = sign_note("Hello", "Ada", "body", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let naddr = Naddr {
        identifier: d_tag(&note),
        pubkey: note.pubkey,
        kind: KIND_LONG_FORM,
        relays: Vec::new(),
    };
    let fetched = fetched_public_addr(&event, &naddr).unwrap();
    assert_eq!(fetched.1.title, "Hello");
    assert_eq!(fetched.1.content, "body");
    let mut wrong_d = naddr.clone();
    wrong_d.identifier = "other".to_string();
    assert!(fetched_public_addr(&event, &wrong_d).is_none());
    let mut wrong_author = naddr.clone();
    wrong_author.pubkey = [0u8; 32];
    assert!(fetched_public_addr(&event, &wrong_author).is_none());
    let mut wrap_kind = naddr.clone();
    wrap_kind.kind = KIND_GIFT_WRAP;
    assert!(fetched_public_addr(&event, &wrap_kind).is_none());
}

#[test]
fn test_naddr_keeps_the_newer_event() {
    let keypair = Keypair::new(SECP256K1, &mut thread_rng());
    let d = "same-article";
    let older_tags = vec![
        vec!["d".to_string(), d.to_string()],
        vec!["title".to_string(), "Hello".to_string()],
    ];
    let newer_tags = older_tags.clone();
    let older = signed_event(&keypair, 1_700_000_000, KIND_LONG_FORM, &older_tags, "old");
    let newer = signed_event(&keypair, 1_800_000_000, KIND_LONG_FORM, &newer_tags, "new");
    let naddr = Naddr {
        identifier: d.to_string(),
        pubkey: older.pubkey,
        kind: KIND_LONG_FORM,
        relays: Vec::new(),
    };
    let old_note =
        fetched_public_addr(&serde_json::from_str(&older.event_json).unwrap(), &naddr).unwrap();
    let new_note =
        fetched_public_addr(&serde_json::from_str(&newer.event_json).unwrap(), &naddr).unwrap();
    assert!(new_note.0 > old_note.0);
    assert_eq!(new_note.1.content, "new");
    assert_eq!(old_note.1.content, "old");
}

#[test]
fn fetch_public_addr_skips_when_it_cannot_ask() {
    let naddr = Naddr {
        identifier: "note".to_string(),
        pubkey: [1u8; 32],
        kind: KIND_LONG_FORM,
        relays: Vec::new(),
    };
    assert!(fetch_public_addr(&[], &naddr, Duration::from_millis(20)).is_none());
    let mut kind1 = naddr.clone();
    kind1.kind = 1;
    assert!(fetch_public_addr(
        &["wss://relay.damus.io".to_string()],
        &kind1,
        Duration::from_millis(20)
    )
    .is_none());
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
    assert!(fetch_public_note(&[], &id, Duration::from_millis(20)).is_none());
    assert!(fetch_public_note(
        &["wss://127.0.0.1".to_string(), "wss://localhost".to_string()],
        &id,
        Duration::from_millis(20)
    )
    .is_none());
    assert!(fetch_public_note(
        &["wss://relay.example:22".to_string()],
        &id,
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
            ..FetchedNote::default()
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
    let note = sign_note("Title", "Ada", "body", 1_700_000_000);
    let started = Instant::now();
    let accepted = publish_to_relays(
        &[format!("wss://127.0.0.1:{port}")],
        &note,
        Duration::from_secs(2),
    );
    assert!(accepted.is_empty());
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
fn public_note_from_json_accepts_a_signed_long_form() {
    let note = sign_note("Hello", "Ada", "body", 1_700_000_000);
    let value: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let parsed = public_note_from_json(&value, 128, 32, 256_000).unwrap();
    assert_eq!(parsed.id, note.id);
    assert!(public_note_from_json(&value, 4, 32, 256_000).is_none());
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
    let id = nonograph_nip44::event_id(&pubkey_hex, created_at, KIND_LONG_FORM, &tags, content);
    let sig = SECP256K1.sign_schnorr_with_aux_rand(&Message::from_digest(id), &keypair, &[0u8; 32]);
    let json = event_wire(
        &hex_encode(&id),
        &pubkey_hex,
        created_at,
        KIND_LONG_FORM,
        &tags,
        content,
        Some(&hex_encode(&sig.serialize())),
    );
    let parsed = parse_event(&serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(parsed.kind, KIND_LONG_FORM);
    assert!(check_id(&parsed));
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
    assert_eq!(parsed.kind, KIND_LONG_FORM);
    assert!(check_id(&parsed));
    assert!(verify_sig(&parsed));
}
