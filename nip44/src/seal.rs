use serde::{Deserialize, Serialize};

use crate::{decrypt, encrypt, hkdf_extract, Error};

pub const SECRET_LEN: usize = 32;
const DOMAIN: &[u8] = b"nonograph-seal-v1";

#[derive(Debug, PartialEq, Eq)]
pub struct Note {
    pub title: String,
    pub author: String,
    pub content: String,
    pub locator: String,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    title: String,
    author: String,
    content: String,
    locator: String,
}

pub fn seal(
    title: &str,
    author: &str,
    content: &str,
    locator: &str,
    secret: &[u8; SECRET_LEN],
    nonce: &[u8; 32],
) -> Result<String, Error> {
    let plaintext = serde_json::to_string(&Wire {
        title: title.to_owned(),
        author: author.to_owned(),
        content: content.to_owned(),
        locator: locator.to_owned(),
    })
    .map_err(|_| Error::Note)?;
    encrypt(&plaintext, &conversation_key(secret), nonce)
}

pub fn open(payload: &str, secret: &[u8; SECRET_LEN], d_tag: &str) -> Result<Note, Error> {
    let json = decrypt(payload, &conversation_key(secret))?;
    let wire: Wire = serde_json::from_str(&json).map_err(|_| Error::Note)?;
    if wire.locator != d_tag {
        return Err(Error::Locator);
    }
    Ok(Note {
        title: wire.title,
        author: wire.author,
        content: wire.content,
        locator: wire.locator,
    })
}

fn conversation_key(secret: &[u8; SECRET_LEN]) -> [u8; SECRET_LEN] {
    hkdf_extract(DOMAIN, secret)
}

#[cfg(test)]
#[path = "../../test/seal.rs"]
mod tests;
