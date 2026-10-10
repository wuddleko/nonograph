use super::{open, seal, Error, Note, SECRET_LEN};

const SECRET: [u8; SECRET_LEN] = [7u8; SECRET_LEN];
const NONCE: [u8; 32] = [9u8; 32];

fn sample() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        "A \"title\"",
        "Ada",
        "line\nwith \"quotes\"",
        "0123456789abcdef0123",
    )
}

#[test]
fn seal_opens_with_the_same_secret_and_locator() {
    let (title, author, content, locator) = sample();
    let payload = seal(title, author, content, locator, &SECRET, &NONCE).unwrap();
    let note = open(&payload, &SECRET, locator).unwrap();
    assert_eq!(
        note,
        Note {
            title: title.to_owned(),
            author: author.to_owned(),
            content: content.to_owned(),
            locator: locator.to_owned(),
        }
    );
}

#[test]
fn a_wrong_secret_fails_the_mac() {
    let (title, author, content, locator) = sample();
    let payload = seal(title, author, content, locator, &SECRET, &NONCE).unwrap();
    let mut wrong = SECRET;
    wrong[0] ^= 1;
    assert!(matches!(open(&payload, &wrong, locator), Err(Error::Mac)));
}

#[test]
fn a_moved_blob_fails_when_the_d_tag_differs() {
    let (title, author, content, locator) = sample();
    let payload = seal(title, author, content, locator, &SECRET, &NONCE).unwrap();
    assert!(matches!(
        open(&payload, &SECRET, "ffffffffffffffffffff"),
        Err(Error::Locator)
    ));
}
