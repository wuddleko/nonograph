use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use sha2::{Digest, Sha256};

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
        .is_some_and(|age| age < 86400)
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
    let key = csrf_key();
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data.as_bytes());
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner.finalize());
    hex_encode(&outer.finalize())
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
