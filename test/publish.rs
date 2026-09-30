use super::*;
use crate::cache::PostCache;
use crate::config::Config;

#[test]
fn publish_fails_when_no_relay_can_take_the_note() {
    let storage = PostCache::shared(1);
    let mut config = Config::default();
    config.nostr.relays.clear();
    config.nostr.timeout_secs = 1;

    let error = publish_note(&storage, &config, "Title", "Ada", "<p>hi</p>", "hi").unwrap_err();
    assert!(matches!(error, PublishFailure::Relays));
    assert!(!storage.read().unwrap().contains_key("not-saved"));

    config.nostr.relays = vec!["https://relay.example".to_string()];
    let error = publish_note(&storage, &config, "Title", "Ada", "<p>hi</p>", "hi").unwrap_err();
    assert!(matches!(error, PublishFailure::Relays));
}
