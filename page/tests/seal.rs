use nonograph_page::{open, seal};

const LOCATOR: &str = "0123456789abcdef0123";

#[test]
fn the_wrapper_seals_and_opens() {
    let secret = [4u8; 32];
    let nonce = [5u8; 32];
    let payload = seal("Title", "Ada", "body", LOCATOR, &secret, &nonce).unwrap();
    let note = open(&payload, &secret, LOCATOR).unwrap();
    assert_eq!(note.title(), "Title");
    assert_eq!(note.author(), "Ada");
    assert_eq!(note.content(), "body");
    assert_eq!(note.locator(), LOCATOR);
}
