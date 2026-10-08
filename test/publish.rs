use super::*;
use crate::cache::PostCache;
use serial_test::serial;
use tempfile::tempdir;

#[test]
#[serial]
fn publish_saves_a_file_and_returns_an_id_without_talking_to_relays() {
    let temp = tempdir().unwrap();
    let base = temp.path().to_str().unwrap();
    let storage = PostCache::shared(1);

    let id = publish_note_in_dir(&storage, "Hello World", "Ada", "<p>hi</p>", "hi", base).unwrap();

    assert!(!id.contains("nevent"));
    assert!(!id.contains("nsec"));
    assert!(id.contains("hello-world"));
    assert!(crate::save::post_file_exists_in_dir(&id, base));
    assert!(storage.read().unwrap().contains_key(&id));
}

#[test]
#[serial]
fn publish_does_not_need_relays() {
    let temp = tempdir().unwrap();
    let base = temp.path().to_str().unwrap();
    let storage = PostCache::shared(1);

    let id = publish_note_in_dir(&storage, "Title", "", "<p>hi</p>", "hi", base).unwrap();
    assert!(id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    assert!(!id.is_empty());
}

#[test]
fn published_href_is_the_id_with_no_secret() {
    let id = "hello-a1b2c3d4";
    let href = published_href(false, id);
    let nojs = published_href(true, id);

    assert_eq!(href, format!("/{id}"));
    assert_eq!(nojs, format!("/nojs/{id}"));
    assert!(!href.contains('?'));
    assert!(!nojs.contains('?'));
    assert!(!href.contains("nsec"));
    assert!(!nojs.contains("nsec"));
}

#[test]
fn published_href_drops_a_query_including_nsec() {
    let href = published_href(false, "hello-a1b2c3d4?nsec=nsec1secret");
    assert_eq!(href, "/hello-a1b2c3d4");
    assert!(!href.contains("nsec"));
    assert!(!href.contains('?'));
}
