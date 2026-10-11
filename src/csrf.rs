use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "nonograph_csrf";
pub const TOKEN_TTL_SECS: u64 = 86_400;

pub fn generate_csrf_token_with_timestamp() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random_part = random_token();
    let data = format!("{timestamp}:{random_part}");
    format!("{data}.{}", csrf_mac(&data))
}

pub fn is_valid_csrf_token(token: &str) -> bool {
    let Some((data, provided)) = token.split_once('.') else {
        return false;
    };
    if !mac_matches(provided, &csrf_mac(data)) {
        return false;
    }

    // The token expires after 24 hours.
    let Some((timestamp, random)) = data.split_once(':') else {
        return false;
    };
    if random.len() != 64 || !random.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let Ok(timestamp) = timestamp.parse::<u64>() else {
        return false;
    };
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    current_time
        .checked_sub(timestamp)
        .is_some_and(|age| age < TOKEN_TTL_SECS)
}

/// Token to put in the form. The bool is true when the response must set the cookie.
pub fn token_for_form(existing: Option<&str>) -> (String, bool) {
    match existing {
        Some(token) if is_valid_csrf_token(token) => (token.to_string(), false),
        _ => (generate_csrf_token_with_timestamp(), true),
    }
}

/// The form field and the cookie have to be the same unexpired token.
pub fn accepts_submission(field: &str, cookie: &str) -> bool {
    mac_matches(field, cookie) && is_valid_csrf_token(field)
}

fn csrf_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    })
}

fn csrf_mac(data: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(csrf_key()).expect("sha256 accepts this key");
    mac.update(data.as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

fn mac_matches(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in provided.bytes().zip(expected.bytes()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
#[path = "../test/csrf.rs"]
mod tests;
