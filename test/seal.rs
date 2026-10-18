use super::*;
use crate::nostr::{decode_fixed_hex, hex_encode, sealed_locator, sealed_note_from_json};

#[test]
fn generate_seal_material_matches_the_browser_lengths() {
    let first = generate_seal_material();
    let second = generate_seal_material();
    assert_eq!(first.secret.len(), nostorgraph_nip44::SECRET_LEN);
    assert_ne!(first.secret, second.secret);
    assert_ne!(first.locator, second.locator);
    for material in [&first, &second] {
        assert!(sealed_locator(&material.locator));
        assert_eq!(
            decode_fixed_hex::<{ nostorgraph_nip44::LOCATOR_LEN / 2 }>(&material.locator)
                .unwrap()
                .len(),
            nostorgraph_nip44::LOCATOR_LEN / 2
        );
    }
}

#[test]
fn seal_plaintext_opens_and_rejects_an_over_cap_ciphertext() {
    let sealed = seal_plaintext("Title", "Ada", "hello", 128, 32, 256_000).unwrap();
    let note = nostorgraph_nip44::open(&sealed.payload, &sealed.secret, &sealed.locator).unwrap();
    assert_eq!(note.title, "Title");
    assert_eq!(note.author, "Ada");
    assert_eq!(note.content, "hello");
    assert_eq!(note.locator, sealed.locator);
    assert!(sealed_locator(&sealed.locator));

    let again = seal_plaintext("Title", "Ada", "hello", 128, 32, 256_000).unwrap();
    assert_ne!(sealed.payload, again.payload);
    assert_ne!(sealed.signing_key.secret_bytes(), sealed.secret);
    assert_ne!(
        sealed.signing_key.secret_bytes(),
        again.signing_key.secret_bytes()
    );
    let pubkey_hex = hex_encode(&sealed.signing_key.x_only_public_key().0.serialize());
    assert_eq!(
        sealed.stamp.tags,
        vec![
            vec!["d".to_string(), sealed.locator.clone()],
            vec![
                "nonce".to_string(),
                sealed.stamp.tags[1][1].clone(),
                nostorgraph_nip44::POW_BITS.to_string(),
            ],
        ]
    );
    assert!(nostorgraph_nip44::check_stamp(
        &sealed.stamp.id,
        &pubkey_hex,
        sealed.stamp.created_at,
        nostorgraph_nip44::KIND,
        &sealed.stamp.tags,
        &sealed.payload,
    )
    .is_ok());

    let over = "h".repeat(sealed_payload_max(0, 0, 0));
    match seal_plaintext("Title", "Ada", &over, 0, 0, 0) {
        Err(SealPlaintextError::TooLong) => {}
        Err(SealPlaintextError::Seal(error)) => panic!("{error}"),
        Err(SealPlaintextError::Stamp) => panic!("stamp failed before the length check"),
        Ok(_) => panic!("expected the ciphertext to exceed the cap"),
    }
}

#[test]
fn base64url_matches_bytes_to_base64url() {
    assert_eq!(base64url(&[0]), "AA");
    assert_eq!(base64url(&[0xfb]), "-w");
    assert_eq!(base64url(&[0xff]), "_w");
    assert_eq!(base64url(&[0, 1]), "AAE");
    assert_eq!(base64url(&[0, 0, 0]), "AAAA");
    assert_eq!(base64url(&[0u8; 32]), "A".repeat(43));
    assert_eq!(base64url(&[0xff, 0xef, 0x00]), "_-8A");
}

#[test]
fn mine_stamp_returns_an_error_when_the_loop_never_finds_a_stamp() {
    assert!(matches!(
        mine_stamp("ab", 1, "0123456789abcdef0123", "cipher", 0),
        Err(SealPlaintextError::Stamp)
    ));
}

#[test]
fn seal_and_sign_round_trips_through_open() {
    let signed = seal_and_sign("Title", "Ada", "hello", 128, 32, 256_000).unwrap();
    let value: serde_json::Value = serde_json::from_str(&signed.note.event_json).unwrap();
    let content = value["content"].as_str().unwrap();
    let pubkey_hex = value["pubkey"].as_str().unwrap();
    let created_at = value["created_at"].as_i64().unwrap();
    let kind = value["kind"].as_u64().unwrap() as u32;
    let id = decode_fixed_hex::<32>(value["id"].as_str().unwrap()).unwrap();
    let tags = value["tags"]
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
        .collect::<Vec<Vec<_>>>();
    assert!(
        nostorgraph_nip44::check_stamp(&id, pubkey_hex, created_at, kind, &tags, content).is_ok()
    );
    let (parsed, locator) = sealed_note_from_json(&value, 128, 32, 256_000).unwrap();
    assert_eq!(parsed.id, signed.note.id);
    assert_eq!(locator, signed.locator);
    let opened = nostorgraph_nip44::open(content, &signed.secret, &locator).unwrap();
    assert_eq!(opened.title, "Title");
    assert_eq!(opened.author, "Ada");
    assert_eq!(opened.content, "hello");
}

#[test]
fn seal_and_sign_returns_a_signed_note_the_locator_and_the_secret() {
    let signed = seal_and_sign("Title", "Ada", "hello", 128, 32, 256_000).unwrap();
    assert_eq!(signed.secret.len(), nostorgraph_nip44::SECRET_LEN);
    let value: serde_json::Value = serde_json::from_str(&signed.note.event_json).unwrap();
    let (parsed, locator) = sealed_note_from_json(&value, 128, 32, 256_000).unwrap();
    assert_eq!(parsed.id, signed.note.id);
    assert_eq!(locator, signed.locator);
    let opened = nostorgraph_nip44::open(
        value["content"].as_str().unwrap(),
        &signed.secret,
        &signed.locator,
    )
    .unwrap();
    assert_eq!(opened.title, "Title");
    assert_eq!(opened.author, "Ada");
    assert_eq!(opened.content, "hello");
    assert_eq!(value["kind"], nostorgraph_nip44::KIND);
    assert_ne!(value["content"].as_str().unwrap(), "hello");
    assert_eq!(signed.k, base64url(&signed.secret));
    assert!(signed
        .k
        .bytes()
        .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_')));
    assert!(!signed.k.contains('='));
    assert_eq!(signed.k.len(), 43);

    let over = "h".repeat(sealed_payload_max(0, 0, 0));
    match seal_and_sign("Title", "Ada", &over, 0, 0, 0) {
        Err(SealPlaintextError::TooLong) => {}
        Err(SealPlaintextError::Seal(error)) => panic!("{error}"),
        Err(SealPlaintextError::Stamp) => panic!("stamp failed before the length check"),
        Ok(_) => panic!("expected the ciphertext to exceed the cap"),
    }
}

#[test]
fn seal_and_sign_does_not_log_the_secret_title_or_body() {
    let source = include_str!("../src/seal.rs");
    let end = source.find("\n#[cfg(test)]").unwrap();
    let body = &source[..end];
    for call in [
        "println!",
        "eprintln!",
        "dbg!",
        "log::",
        "tracing::",
        "info!",
        "warn!",
        "error!",
        "debug!",
    ] {
        assert!(!body.contains(call), "{call}");
    }

    let title = "Title not for the log zz";
    let content = "Body not for the log zz";
    let err = match seal_and_sign(title, "Ada", content, 0, 0, 0) {
        Err(err) => err,
        Ok(_) => panic!("expected the ciphertext to exceed the cap"),
    };
    let rendered = format!("{err:?}");
    assert!(!rendered.contains(title));
    assert!(!rendered.contains(content));
    assert!(!rendered.contains("secret"));
}

#[test]
fn sealed_payload_max_is_the_ciphertext_a_full_note_can_produce() {
    let empty = serde_json::json!({
        "title": "",
        "author": "",
        "content": "",
        "locator": "",
    })
    .to_string();
    assert_eq!(empty.len(), NOTE_JSON_SKELETON);

    let key = [1u8; 32];
    let nonce = [2u8; 32];
    for len in [1usize, 32, 33, 65_535, 65_536] {
        let payload = nostorgraph_nip44::encrypt(&"a".repeat(len), &key, &nonce).unwrap();
        assert_eq!(
            payload.len(),
            nostorgraph_nip44::encrypted_len(len).unwrap(),
            "{len}"
        );
    }

    let title_max = 8;
    let alias_max = 4;
    let content_max = 64;
    let title = "\u{1}".repeat(title_max);
    let author = "\u{1}".repeat(alias_max);
    let content = "\u{1}".repeat(content_max);
    let locator = "0123456789abcdef0123";
    let json = serde_json::json!({
        "title": title,
        "author": author,
        "content": content,
        "locator": locator,
    })
    .to_string();
    assert_eq!(
        json.len(),
        worst_note_json_len(title_max, alias_max, content_max).unwrap()
    );
    let secret = [3u8; 32];
    let sealed_payload =
        nostorgraph_nip44::seal(&title, &author, &content, locator, &secret, &nonce).unwrap();
    let cap = sealed_payload_max(title_max, alias_max, content_max);
    assert_eq!(sealed_payload.len(), cap);
    assert!(cap > content_max);

    let sealed =
        seal_plaintext(&title, &author, &content, title_max, alias_max, content_max).unwrap();
    assert_eq!(sealed.payload.len(), cap);
    let signed =
        seal_and_sign(&title, &author, &content, title_max, alias_max, content_max).unwrap();
    let value: serde_json::Value = serde_json::from_str(&signed.note.event_json).unwrap();
    assert!(sealed_note_from_json(&value, title_max, alias_max, content_max).is_some());
    assert_eq!(value["content"].as_str().unwrap().len(), cap);

    let ordinary = "a".repeat(content_max);
    let ordinary_seal =
        seal_plaintext("Title", "Ada", &ordinary, title_max, alias_max, content_max).unwrap();
    assert!(ordinary_seal.payload.len() > content_max);
    assert!(ordinary_seal.payload.len() <= cap);
}

#[test]
fn sealed_payload_max_stops_at_the_payload_decrypt_accepts() {
    assert_eq!(
        sealed_payload_max(128, 32, 256_000),
        nostorgraph_nip44::MAX_PAYLOAD_CHARS
    );
    assert_eq!(
        sealed_payload_max(usize::MAX, usize::MAX, usize::MAX),
        nostorgraph_nip44::MAX_PAYLOAD_CHARS
    );

    let locator = "0123456789abcdef0123";
    let secret = [3u8; 32];
    let nonce = [2u8; 32];
    let ordinary = "a".repeat(256_000);
    let ordinary_payload =
        nostorgraph_nip44::seal("T", "", &ordinary, locator, &secret, &nonce).unwrap();
    let cap = sealed_payload_max(128, 32, 256_000);
    assert!(ordinary_payload.len() > 256_000);
    assert!(ordinary_payload.len() <= cap);
    assert!(nostorgraph_nip44::open(&ordinary_payload, &secret, locator).is_ok());

    let controls = "\u{1}".repeat(109_215);
    let over = nostorgraph_nip44::seal("T", "", &controls, locator, &secret, &nonce).unwrap();
    assert!(over.len() > cap);
    assert!(nostorgraph_nip44::open(&over, &secret, locator).is_err());
    match seal_plaintext("T", "", &controls, 128, 32, 256_000) {
        Err(SealPlaintextError::TooLong) => {}
        Err(SealPlaintextError::Seal(error)) => panic!("{error}"),
        Err(SealPlaintextError::Stamp) => panic!("stamp failed before the length check"),
        Ok(_) => panic!("expected the ciphertext to exceed the decrypt limit"),
    }
}
