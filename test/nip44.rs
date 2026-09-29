
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
    serde_json::from_str(include_str!("nip44.vectors.json")).unwrap()
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

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
