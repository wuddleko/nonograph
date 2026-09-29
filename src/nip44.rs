use base64::Engine;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use secp256k1::{Parity, PublicKey, SecretKey, XOnlyPublicKey};
use sha2::Sha256;

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
        }
    }
}

impl std::error::Error for Error {}

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

fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};
    use sha2::{Digest, Sha256};

    fn decode_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn vectors() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/nip44.vectors.json")).unwrap()
    }

    #[test]
    fn test_nip44_conversation_keys() {
        for case in vectors()["v2"]["valid"]["get_conversation_key"]
            .as_array()
            .unwrap()
        {
            let sec1 = decode_hex(case["sec1"].as_str().unwrap());
            let pub2 = decode_hex(case["pub2"].as_str().unwrap());
            let key = conversation_key(sec1.as_slice().try_into().unwrap(), &pub2).unwrap();
            assert_eq!(hex::encode(key), case["conversation_key"].as_str().unwrap());
        }
    }

    #[test]
    fn test_nip44_message_keys() {
        let section = &vectors()["v2"]["valid"]["get_message_keys"];
        let conversation_key: [u8; 32] = decode_hex(section["conversation_key"].as_str().unwrap())
            .try_into()
            .unwrap();
        for case in section["keys"].as_array().unwrap() {
            let nonce: [u8; 32] = decode_hex(case["nonce"].as_str().unwrap())
                .try_into()
                .unwrap();
            let (chacha_key, chacha_nonce, hmac_key) = message_keys(&conversation_key, &nonce);
            assert_eq!(
                hex::encode(chacha_key),
                case["chacha_key"].as_str().unwrap()
            );
            assert_eq!(
                hex::encode(chacha_nonce),
                case["chacha_nonce"].as_str().unwrap()
            );
            assert_eq!(hex::encode(hmac_key), case["hmac_key"].as_str().unwrap());
        }
    }

    #[test]
    fn test_nip44_padded_lengths() {
        for pair in vectors()["v2"]["valid"]["calc_padded_len"]
            .as_array()
            .unwrap()
        {
            let input = pair[0].as_u64().unwrap() as usize;
            let expected = pair[1].as_u64().unwrap() as usize;
            assert_eq!(calc_padded_len(input), expected);
        }
    }

    #[test]
    fn test_nip44_encrypt_decrypt_vectors() {
        let secp = Secp256k1::new();
        for case in vectors()["v2"]["valid"]["encrypt_decrypt"]
            .as_array()
            .unwrap()
        {
            let sec1 = SecretKey::from_slice(&decode_hex(case["sec1"].as_str().unwrap())).unwrap();
            let sec2 = SecretKey::from_slice(&decode_hex(case["sec2"].as_str().unwrap())).unwrap();
            let pub2 = sec2.x_only_public_key(&secp).0.serialize();
            let pub1 = sec1.x_only_public_key(&secp).0.serialize();
            let sec1_bytes = sec1.secret_bytes();
            let sec2_bytes = sec2.secret_bytes();
            let from_first = conversation_key(&sec1_bytes, &pub2).unwrap();
            let from_second = conversation_key(&sec2_bytes, &pub1).unwrap();
            assert_eq!(from_first, from_second);
            assert_eq!(
                hex::encode(from_first),
                case["conversation_key"].as_str().unwrap()
            );
            let nonce: [u8; 32] = decode_hex(case["nonce"].as_str().unwrap())
                .try_into()
                .unwrap();
            let plaintext = case["plaintext"].as_str().unwrap();
            let payload = encrypt(plaintext, &from_first, &nonce).unwrap();
            assert_eq!(payload, case["payload"].as_str().unwrap());
            assert_eq!(decrypt(&payload, &from_second).unwrap(), plaintext);
        }
    }

    #[test]
    fn test_nip44_long_messages() {
        for case in vectors()["v2"]["valid"]["encrypt_decrypt_long_msg"]
            .as_array()
            .unwrap()
        {
            let conversation_key: [u8; 32] = decode_hex(case["conversation_key"].as_str().unwrap())
                .try_into()
                .unwrap();
            let nonce: [u8; 32] = decode_hex(case["nonce"].as_str().unwrap())
                .try_into()
                .unwrap();
            let plaintext = case["pattern"]
                .as_str()
                .unwrap()
                .repeat(case["repeat"].as_u64().unwrap() as usize);
            assert_eq!(
                sha256_hex(plaintext.as_bytes()),
                case["plaintext_sha256"].as_str().unwrap()
            );
            let payload = encrypt(&plaintext, &conversation_key, &nonce).unwrap();
            assert_eq!(
                sha256_hex(payload.as_bytes()),
                case["payload_sha256"].as_str().unwrap()
            );
            assert_eq!(decrypt(&payload, &conversation_key).unwrap(), plaintext);
        }
    }

    #[test]
    fn test_nip44_extended_prefix_lengths() {
        let conversation_key =
            decode_hex("c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d");
        let conversation_key: [u8; 32] = conversation_key.try_into().unwrap();
        let nonce: [u8; 32] =
            decode_hex("0000000000000000000000000000000000000000000000000000000000000001")
                .try_into()
                .unwrap();
        let cases = [
            (
                65535usize,
                "6e1bebca6a8229364a162a72ef064826c4cd7457bf54f190ef782bd9deff3e42",
                "6d8c2810d1e870fbaa1f0a0937126cca837a15f9260e27060c331d70a3c0bc84",
            ),
            (
                65536,
                "bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a",
                "b7b4edb36ba92e267d322d56d9aebc22e7fa96ff52e3c12adc07f07a43cbc616",
            ),
            (
                65537,
                "008ffc88d3c96a9f307524eb361e47c5222a887fc45fa0c1fb8d429c5c23b430",
                "eeb7c7c5373894ea2c1547cfd3ccb15d5a0b2d619da852e5c79df792dcc9e435",
            ),
        ];
        for (length, plaintext_sha, payload_sha) in cases {
            let plaintext = "a".repeat(length);
            assert_eq!(sha256_hex(plaintext.as_bytes()), plaintext_sha);
            let payload = encrypt(&plaintext, &conversation_key, &nonce).unwrap();
            assert_eq!(sha256_hex(payload.as_bytes()), payload_sha);
            assert_eq!(decrypt(&payload, &conversation_key).unwrap(), plaintext);
        }
    }

    #[test]
    fn test_nip44_invalid_conversation_keys() {
        for case in vectors()["v2"]["invalid"]["get_conversation_key"]
            .as_array()
            .unwrap()
        {
            let sec1 = decode_hex(case["sec1"].as_str().unwrap());
            let pub2 = decode_hex(case["pub2"].as_str().unwrap());
            let sec1: [u8; 32] = sec1.try_into().unwrap();
            assert!(conversation_key(&sec1, &pub2).is_err());
        }
    }

    #[test]
    fn test_nip44_invalid_payloads() {
        for case in vectors()["v2"]["invalid"]["decrypt"].as_array().unwrap() {
            let conversation_key: [u8; 32] = decode_hex(case["conversation_key"].as_str().unwrap())
                .try_into()
                .unwrap();
            assert!(decrypt(case["payload"].as_str().unwrap(), &conversation_key).is_err());
        }
        assert!(encrypt("", &[0u8; 32], &[1u8; 32]).is_err());
        let oversized = "A".repeat(MAX_PAYLOAD_CHARS + 1);
        assert!(matches!(decode_payload(&oversized), Err(Error::Payload)));
        assert!(decrypt(&oversized, &[0u8; 32]).is_err());
    }
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
