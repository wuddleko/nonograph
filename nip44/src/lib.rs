use base64::Engine;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[cfg(not(target_arch = "wasm32"))]
use secp256k1::{Parity, PublicKey, SecretKey, XOnlyPublicKey};

type HmacSha256 = Hmac<Sha256>;

const MIN_PAYLOAD_CHARS: usize = 132;
const MAX_PAYLOAD_CHARS: usize = 1_048_576;
const MIN_PAYLOAD_BYTES: usize = 99;
const MAX_PAYLOAD_BYTES: usize = 786_432;
const EXTENDED_PREFIX_AT: usize = 65536;
const MESSAGE_KEY_LEN: usize = 76;

#[derive(Debug)]
pub enum Error {
    Key,
    PlaintextLength,
    Payload,
    Mac,
    Padding,
    Utf8,
    Note,
    Locator,
    Kind,
    Stamp,
    Tag,
    Id,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Key => write!(f, "invalid key"),
            Error::PlaintextLength => write!(f, "invalid plaintext length"),
            Error::Payload => write!(f, "invalid payload"),
            Error::Mac => write!(f, "invalid MAC"),
            Error::Padding => write!(f, "invalid padding"),
            Error::Utf8 => write!(f, "invalid UTF-8"),
            Error::Note => write!(f, "invalid note"),
            Error::Locator => write!(f, "locator mismatch"),
            Error::Kind => write!(f, "invalid kind"),
            Error::Stamp => write!(f, "weak stamp"),
            Error::Tag => write!(f, "invalid tag"),
            Error::Id => write!(f, "id mismatch"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(not(target_arch = "wasm32"))]
pub fn conversation_key(private_key: &[u8; 32], public_key: &[u8]) -> Result<[u8; 32], Error> {
    if public_key.len() != 32 {
        return Err(Error::Key);
    }
    let secret = SecretKey::from_slice(private_key).map_err(|_| Error::Key)?;
    let xonly = XOnlyPublicKey::from_slice(public_key).map_err(|_| Error::Key)?;
    let point = PublicKey::from_x_only_public_key(xonly, Parity::Even);
    let shared = secp256k1::ecdh::shared_secret_point(&point, &secret);
    Ok(hkdf_extract(b"nip44-v2", &shared[..32]))
}

pub fn encrypt(
    plaintext: &str,
    conversation_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<String, Error> {
    let (chacha_key, chacha_nonce, hmac_key) = message_keys(conversation_key, nonce);
    let mut ciphertext = pad(plaintext)?;
    apply_chacha(&chacha_key, &chacha_nonce, &mut ciphertext);
    let mac = hmac_aad(&hmac_key, &ciphertext, nonce)
        .finalize()
        .into_bytes();
    let mut payload = Vec::with_capacity(1 + nonce.len() + ciphertext.len() + mac.len());
    payload.push(2);
    payload.extend_from_slice(nonce);
    payload.extend_from_slice(&ciphertext);
    payload.extend_from_slice(&mac);
    Ok(base64::engine::general_purpose::STANDARD.encode(payload))
}

pub fn decrypt(payload: &str, conversation_key: &[u8; 32]) -> Result<String, Error> {
    let (nonce, ciphertext, mac) = decode_payload(payload)?;
    let (chacha_key, chacha_nonce, hmac_key) = message_keys(conversation_key, &nonce);
    hmac_aad(&hmac_key, &ciphertext, &nonce)
        .verify_slice(&mac)
        .map_err(|_| Error::Mac)?;
    let mut padded = ciphertext;
    apply_chacha(&chacha_key, &chacha_nonce, &mut padded);
    unpad(&padded)
}

fn message_keys(conversation_key: &[u8; 32], nonce: &[u8; 32]) -> ([u8; 32], [u8; 12], [u8; 32]) {
    let keys = hkdf_expand(conversation_key, nonce);
    (
        keys[0..32].try_into().expect("chacha key is 32 bytes"),
        keys[32..44].try_into().expect("chacha nonce is 12 bytes"),
        keys[44..MESSAGE_KEY_LEN]
            .try_into()
            .expect("hmac key is 32 bytes"),
    )
}

fn decode_payload(payload: &str) -> Result<([u8; 32], Vec<u8>, [u8; 32]), Error> {
    if payload.is_empty() || payload.starts_with('#') {
        return Err(Error::Payload);
    }
    if payload.len() < MIN_PAYLOAD_CHARS || payload.len() > MAX_PAYLOAD_CHARS {
        return Err(Error::Payload);
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| Error::Payload)?;
    if data.len() < MIN_PAYLOAD_BYTES || data.len() > MAX_PAYLOAD_BYTES || data[0] != 2 {
        return Err(Error::Payload);
    }
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&data[1..33]);
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&data[data.len() - 32..]);
    let ciphertext = data[33..data.len() - 32].to_vec();
    Ok((nonce, ciphertext, mac))
}

fn hmac_aad(key: &[u8; 32], message: &[u8], aad: &[u8; 32]) -> HmacSha256 {
    HmacSha256::new_from_slice(key)
        .expect("sha256 accepts this key")
        .chain_update(aad)
        .chain_update(message)
}

fn apply_chacha(key: &[u8; 32], nonce: &[u8; 12], data: &mut [u8]) {
    chacha20::ChaCha20::new_from_slices(key, nonce)
        .expect("32-byte key, 12-byte nonce")
        .apply_keystream(data);
}

fn pad(plaintext: &str) -> Result<Vec<u8>, Error> {
    let unpadded = plaintext.as_bytes();
    let unpadded_len = unpadded.len();
    if unpadded_len < 1 || unpadded_len > u32::MAX as usize {
        return Err(Error::PlaintextLength);
    }
    let mut out = if unpadded_len >= EXTENDED_PREFIX_AT {
        let mut prefix = vec![0, 0];
        prefix.extend_from_slice(&(unpadded_len as u32).to_be_bytes());
        prefix
    } else {
        (unpadded_len as u16).to_be_bytes().to_vec()
    };
    out.extend_from_slice(unpadded);
    out.resize(out.len() + calc_padded_len(unpadded_len) - unpadded_len, 0);
    Ok(out)
}

fn unpad(padded: &[u8]) -> Result<String, Error> {
    if padded.len() < 2 {
        return Err(Error::Padding);
    }
    let first_two = u16::from_be_bytes([padded[0], padded[1]]) as usize;
    let (unpadded_len, prefix_len): (usize, usize) = if first_two == 0 {
        if padded.len() < 6 {
            return Err(Error::Padding);
        }
        let len = u32::from_be_bytes([padded[2], padded[3], padded[4], padded[5]]) as usize;
        if len < EXTENDED_PREFIX_AT {
            return Err(Error::Padding);
        }
        (len, 6)
    } else {
        (first_two, 2)
    };
    let end = prefix_len.checked_add(unpadded_len).ok_or(Error::Padding)?;
    if unpadded_len == 0 || padded.len() < end {
        return Err(Error::Padding);
    }
    if padded.len() != prefix_len + calc_padded_len(unpadded_len) {
        return Err(Error::Padding);
    }
    str::from_utf8(&padded[prefix_len..end])
        .map(str::to_owned)
        .map_err(|_| Error::Utf8)
}

fn calc_padded_len(unpadded_len: usize) -> usize {
    if unpadded_len <= 32 {
        return 32;
    }
    let unpadded_len = unpadded_len as u64;
    let next_power = 1u64 << ((unpadded_len - 1).ilog2() + 1);
    let chunk = if next_power <= 256 {
        32
    } else {
        next_power / 8
    };
    (chunk * ((unpadded_len - 1) / chunk + 1)) as usize
}

pub(crate) fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let bytes = HmacSha256::new_from_slice(salt)
        .expect("sha256 accepts this salt")
        .chain_update(ikm)
        .finalize()
        .into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

fn hkdf_expand(prk: &[u8; 32], info: &[u8; 32]) -> [u8; MESSAGE_KEY_LEN] {
    let mut okm = [0u8; MESSAGE_KEY_LEN];
    let mut previous = [0u8; 32];
    let mut offset = 0;
    for counter in 1u8..=3 {
        let mut mac = HmacSha256::new_from_slice(prk).expect("sha256 accepts this key");
        if counter > 1 {
            mac.update(&previous);
        }
        mac.update(info);
        mac.update(&[counter]);
        previous.copy_from_slice(&mac.finalize().into_bytes());
        let end = (offset + 32).min(MESSAGE_KEY_LEN);
        okm[offset..end].copy_from_slice(&previous[..end - offset]);
        offset = end;
    }
    okm
}

mod seal;
mod stamp;

pub use seal::{open, seal, Note, SECRET_LEN};
pub use stamp::{check_stamp, event_id, push_json_string, push_tags, KIND, LOCATOR_LEN, POW_BITS};

#[cfg(test)]
#[path = "../../test/nip44.rs"]
mod tests;
