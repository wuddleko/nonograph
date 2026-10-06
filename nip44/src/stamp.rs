use sha2::{Digest, Sha256};

use crate::Error;

pub const KIND: u32 = 30323;
pub const POW_BITS: u32 = 12;
pub const LOCATOR_LEN: usize = 20;

pub fn check_stamp(
    id: &[u8; 32],
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> Result<(), Error> {
    if kind != KIND {
        return Err(Error::Kind);
    }
    check_tags(tags)?;
    let computed = event_id(pubkey_hex, created_at, kind, tags, content);
    if computed != *id {
        return Err(Error::Id);
    }
    if leading_zero_bits(&computed) < POW_BITS {
        return Err(Error::Stamp);
    }
    Ok(())
}

fn check_tags(tags: &[Vec<String>]) -> Result<(), Error> {
    let mut seen_d = false;
    let mut seen_nonce = false;
    for tag in tags {
        match tag.as_slice() {
            [name, locator] if name == "d" => {
                if seen_d || !valid_locator(locator) {
                    return Err(Error::Tag);
                }
                seen_d = true;
            }
            [name, value, target] if name == "nonce" => {
                if seen_nonce || value.is_empty() || *target != POW_BITS.to_string() {
                    return Err(Error::Tag);
                }
                seen_nonce = true;
            }
            _ => return Err(Error::Tag),
        }
    }
    if seen_d && seen_nonce {
        Ok(())
    } else {
        Err(Error::Tag)
    }
}

fn valid_locator(locator: &str) -> bool {
    locator.len() == LOCATOR_LEN
        && locator
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn event_id(
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> [u8; 32] {
    Sha256::digest(canonical_event(pubkey_hex, created_at, kind, tags, content).as_bytes()).into()
}

fn canonical_event(
    pubkey_hex: &str,
    created_at: i64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> String {
    let mut out = String::new();
    out.push_str("[0,\"");
    out.push_str(pubkey_hex);
    out.push_str("\",");
    out.push_str(&created_at.to_string());
    out.push(',');
    out.push_str(&kind.to_string());
    out.push(',');
    push_tags(&mut out, tags);
    out.push(',');
    push_json_string(&mut out, content);
    out.push(']');
    out
}

pub fn push_tags(out: &mut String, tags: &[Vec<String>]) {
    out.push('[');
    for (tag_index, tag) in tags.iter().enumerate() {
        if tag_index > 0 {
            out.push(',');
        }
        out.push('[');
        for (item_index, item) in tag.iter().enumerate() {
            if item_index > 0 {
                out.push(',');
            }
            push_json_string(out, item);
        }
        out.push(']');
    }
    out.push(']');
}

pub fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn leading_zero_bits(id: &[u8; 32]) -> u32 {
    let mut bits = 0;
    for byte in id {
        if *byte == 0 {
            bits += 8;
            continue;
        }
        return bits + byte.leading_zeros();
    }
    bits
}

#[cfg(test)]
#[path = "../../test/stamp.rs"]
mod tests;
