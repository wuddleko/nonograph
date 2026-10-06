use super::*;

const PUBKEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CREATED_AT: i64 = 1_700_000_000;
const CONTENT: &str = "ciphertext";
const LOCATOR: &str = "0123456789abcdef0123";

fn event_tags(nonce: &str) -> Vec<Vec<String>> {
    vec![
        vec!["d".to_string(), LOCATOR.to_string()],
        vec!["nonce".to_string(), nonce.to_string(), POW_BITS.to_string()],
    ]
}

fn hashed(kind: u32, tags: &[Vec<String>], content: &str) -> [u8; 32] {
    event_id(PUBKEY, CREATED_AT, kind, tags, content)
}

fn mined() -> (Vec<Vec<String>>, [u8; 32]) {
    for nonce in 0..1_000_000u32 {
        let tags = event_tags(&nonce.to_string());
        let id = hashed(KIND, &tags, CONTENT);
        if check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT).is_ok() {
            return (tags, id);
        }
    }
    panic!("no stamp in range");
}

fn unmined() -> (Vec<Vec<String>>, [u8; 32]) {
    for nonce in 0..1_000_000u32 {
        let tags = event_tags(&nonce.to_string());
        let id = hashed(KIND, &tags, CONTENT);
        if matches!(
            check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
            Err(Error::Stamp)
        ) {
            return (tags, id);
        }
    }
    panic!("no weak stamp in range");
}

#[test]
fn twelve_leading_zero_bits_pass() {
    let (tags, id) = mined();
    assert!(check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT).is_ok());
}

#[test]
fn a_weaker_stamp_is_rejected() {
    let (tags, id) = unmined();
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Stamp)
    ));
}

#[test]
fn a_claimed_id_that_is_not_the_event_hash_is_rejected() {
    let (tags, _) = unmined();
    let mut fake = [0u8; 32];
    fake[1] = 0x0f;
    assert!(matches!(
        check_stamp(&fake, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Id)
    ));
}

#[test]
fn a_title_tag_is_rejected() {
    let (mut tags, _) = mined();
    tags.push(vec!["title".to_string(), "Visible".to_string()]);
    let id = hashed(KIND, &tags, CONTENT);
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Tag)
    ));
}

#[test]
fn another_kind_is_rejected() {
    let tags = event_tags("0");
    let id = hashed(30023, &tags, CONTENT);
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, 30023, &tags, CONTENT),
        Err(Error::Kind)
    ));
}

#[test]
fn missing_or_duplicate_required_tags_are_rejected() {
    let (valid, _) = unmined();
    let cases = [
        vec![],
        vec![valid[0].clone()],
        vec![valid[1].clone()],
        vec![valid[0].clone(), valid[0].clone(), valid[1].clone()],
        vec![valid[0].clone(), valid[1].clone(), valid[1].clone()],
    ];
    for tags in cases {
        let id = hashed(KIND, &tags, CONTENT);
        assert!(
            matches!(
                check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
                Err(Error::Tag)
            ),
            "{tags:?}"
        );
    }
}

#[test]
fn extra_fields_on_d_or_nonce_are_rejected() {
    let (mut tags, _) = unmined();
    tags[0].push("visible title".to_string());
    let id = hashed(KIND, &tags, CONTENT);
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Tag)
    ));

    let (mut tags, _) = unmined();
    tags[1].push("extra".to_string());
    let id = hashed(KIND, &tags, CONTENT);
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Tag)
    ));
}

#[test]
fn a_nonce_that_does_not_commit_to_twelve_bits_is_rejected() {
    let tags = vec![
        vec!["d".to_string(), LOCATOR.to_string()],
        vec!["nonce".to_string(), "1".to_string(), "11".to_string()],
    ];
    let id = hashed(KIND, &tags, CONTENT);
    assert!(matches!(
        check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
        Err(Error::Tag)
    ));
}

#[test]
fn a_locator_that_is_not_twenty_lowercase_hex_is_rejected() {
    for locator in ["0123456789abcdef012", "0123456789ABCDEF0123", "0123456789abcdef012g"] {
        let tags = vec![
            vec!["d".to_string(), locator.to_string()],
            vec!["nonce".to_string(), "1".to_string(), POW_BITS.to_string()],
        ];
        let id = hashed(KIND, &tags, CONTENT);
        assert!(
            matches!(
                check_stamp(&id, PUBKEY, CREATED_AT, KIND, &tags, CONTENT),
                Err(Error::Tag)
            ),
            "{locator}"
        );
    }
    assert_eq!(LOCATOR.len(), LOCATOR_LEN);
}
