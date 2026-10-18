use rand::{thread_rng, Rng};
use secp256k1::{Keypair, SECP256K1};

use crate::nostr::{hex_encode, now_secs, signed_event, SignedNote};

pub(crate) struct SealMaterial {
    pub secret: [u8; nostorgraph_nip44::SECRET_LEN],
    pub locator: String,
}

pub(crate) fn generate_seal_material() -> SealMaterial {
    let mut rng = thread_rng();
    let mut raw = [0u8; nostorgraph_nip44::SECRET_LEN + nostorgraph_nip44::LOCATOR_LEN / 2];
    for byte in &mut raw {
        *byte = rng.gen();
    }
    let mut secret = [0u8; nostorgraph_nip44::SECRET_LEN];
    secret.copy_from_slice(&raw[..nostorgraph_nip44::SECRET_LEN]);
    SealMaterial {
        secret,
        locator: hex_encode(&raw[nostorgraph_nip44::SECRET_LEN..]),
    }
}

pub(crate) struct SealedPlaintext {
    pub secret: [u8; nostorgraph_nip44::SECRET_LEN],
    pub locator: String,
    pub payload: String,
    pub signing_key: Keypair,
    pub stamp: MinedStamp,
}

#[derive(Debug)]
pub(crate) enum SealPlaintextError {
    Seal(nostorgraph_nip44::Error),
    TooLong,
    Stamp,
}

pub(crate) const STAMP_ATTEMPTS: u32 = 1_000_000;

const NOTE_JSON_SKELETON: usize = 50;
const JSON_ESCAPE_EXPANSION: usize = 6;

pub(crate) fn sealed_payload_max(title_max: usize, alias_max: usize, content_max: usize) -> usize {
    worst_note_json_len(title_max, alias_max, content_max)
        .and_then(nostorgraph_nip44::encrypted_len)
        .map(|len| len.min(nostorgraph_nip44::MAX_PAYLOAD_CHARS))
        .unwrap_or(nostorgraph_nip44::MAX_PAYLOAD_CHARS)
}

fn worst_note_json_len(title_max: usize, alias_max: usize, content_max: usize) -> Option<usize> {
    let expanded = |len: usize| len.checked_mul(JSON_ESCAPE_EXPANSION);
    NOTE_JSON_SKELETON
        .checked_add(expanded(title_max)?)?
        .checked_add(expanded(alias_max)?)?
        .checked_add(expanded(content_max)?)?
        .checked_add(nostorgraph_nip44::LOCATOR_LEN)
}

pub(crate) struct MinedStamp {
    pub id: [u8; 32],
    pub created_at: i64,
    pub tags: Vec<Vec<String>>,
}

pub(crate) fn sealed_tags(locator: &str, nonce: &str) -> Vec<Vec<String>> {
    vec![
        vec!["d".to_string(), locator.to_string()],
        vec![
            "nonce".to_string(),
            nonce.to_string(),
            nostorgraph_nip44::POW_BITS.to_string(),
        ],
    ]
}

pub(crate) fn mine_stamp(
    pubkey_hex: &str,
    created_at: i64,
    locator: &str,
    content: &str,
    attempts: u32,
) -> Result<MinedStamp, SealPlaintextError> {
    let kind = nostorgraph_nip44::KIND;
    for counter in 0..attempts {
        let tags = sealed_tags(locator, &counter.to_string());
        let id = nostorgraph_nip44::event_id(pubkey_hex, created_at, kind, &tags, content);
        if nostorgraph_nip44::check_stamp(&id, pubkey_hex, created_at, kind, &tags, content).is_ok() {
            return Ok(MinedStamp {
                id,
                created_at,
                tags,
            });
        }
    }
    Err(SealPlaintextError::Stamp)
}

pub(crate) fn seal_plaintext(
    title: &str,
    author: &str,
    content: &str,
    title_max: usize,
    alias_max: usize,
    content_max: usize,
) -> Result<SealedPlaintext, SealPlaintextError> {
    let material = generate_seal_material();
    let mut nonce = [0u8; 32];
    let mut rng = thread_rng();
    for byte in &mut nonce {
        *byte = rng.gen();
    }
    let payload = nostorgraph_nip44::seal(
        title,
        author,
        content,
        &material.locator,
        &material.secret,
        &nonce,
    )
    .map_err(SealPlaintextError::Seal)?;
    if payload.len() > sealed_payload_max(title_max, alias_max, content_max) {
        return Err(SealPlaintextError::TooLong);
    }
    let signing_key = Keypair::new(SECP256K1, &mut thread_rng());
    let pubkey_hex = hex_encode(&signing_key.x_only_public_key().0.serialize());
    let created_at = now_secs();
    let stamp = mine_stamp(
        &pubkey_hex,
        created_at,
        &material.locator,
        &payload,
        STAMP_ATTEMPTS,
    )?;
    Ok(SealedPlaintext {
        secret: material.secret,
        locator: material.locator,
        payload,
        signing_key,
        stamp,
    })
}

pub(crate) struct SignedSeal {
    pub note: SignedNote,
    pub locator: String,
    pub secret: [u8; nostorgraph_nip44::SECRET_LEN],
    pub k: String,
}

pub(crate) fn seal_and_sign(
    title: &str,
    author: &str,
    content: &str,
    title_max: usize,
    alias_max: usize,
    content_max: usize,
) -> Result<SignedSeal, SealPlaintextError> {
    let sealed = seal_plaintext(title, author, content, title_max, alias_max, content_max)?;
    let note = signed_event(
        &sealed.signing_key,
        sealed.stamp.created_at,
        nostorgraph_nip44::KIND,
        &sealed.stamp.tags,
        &sealed.payload,
    );
    Ok(SignedSeal {
        note,
        locator: sealed.locator,
        k: base64url(&sealed.secret),
        secret: sealed.secret,
    })
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | (bytes[index + 2] as u32);
        out.push(ALPHABET[((chunk >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((chunk >> 12) & 63) as usize] as char);
        out.push(ALPHABET[((chunk >> 6) & 63) as usize] as char);
        out.push(ALPHABET[(chunk & 63) as usize] as char);
        index += 3;
    }
    match bytes.len() - index {
        1 => {
            let chunk = (bytes[index] as u32) << 16;
            out.push(ALPHABET[((chunk >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((chunk >> 12) & 63) as usize] as char);
        }
        2 => {
            let chunk = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            out.push(ALPHABET[((chunk >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((chunk >> 12) & 63) as usize] as char);
            out.push(ALPHABET[((chunk >> 6) & 63) as usize] as char);
        }
        _ => {}
    }
    out
}

#[cfg(test)]
#[path = "../test/seal.rs"]
mod tests;
