use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

use crate::font;
use crate::font::Face;

pub(crate) fn compose(text: &str) -> Cow<'_, str> {
    if unicode_normalization::is_nfc(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}

pub(crate) fn shown(ch: char, face: Face) -> char {
    if font::covers(face, ch) {
        ch
    } else {
        '?'
    }
}

pub(crate) fn utf16(text: &str, face: Face) -> Vec<u8> {
    let text = compose(text);
    let mut out = Vec::with_capacity(text.len().saturating_mul(2));
    for ch in text.chars() {
        out.extend_from_slice(&font::cid(face, shown(ch, face)).to_be_bytes());
    }
    out
}

pub(crate) fn literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('(');
    for &byte in bytes {
        match byte {
            b'\\' | b'(' | b')' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7E => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out.push(')');
    out
}

pub(crate) fn uri(url: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in url.chars() {
        if ch.is_ascii() {
            out.push(ch as u8);
            continue;
        }
        let mut buf = [0u8; 4];
        for byte in ch.encode_utf8(&mut buf).as_bytes() {
            out.push(b'%');
            out.push(hex(byte >> 4));
            out.push(hex(byte & 0x0F));
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn tj(text: &str) -> Vec<u8> {
    format!("{} Tj", literal(&utf16(text, Face::Helvetica))).into_bytes()
}

fn hex(nibble: u8) -> u8 {
    b"0123456789ABCDEF"[nibble as usize]
}
