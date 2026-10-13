use std::time::Duration;

use crate::config::Config;
use crate::nostr::{canonical_public_relay, MAX_FETCH_RELAYS};

pub(crate) fn relay_timeout(config: &Config) -> Duration {
    Duration::from_secs(config.nostr.timeout_secs.max(1))
}

pub(crate) fn push_public_relay(relays: &mut Vec<String>, relay: &str) {
    if relays.len() == MAX_FETCH_RELAYS {
        return;
    }
    let Some(relay) = canonical_public_relay(relay) else {
        return;
    };
    if relays.iter().any(|have| have == &relay) {
        return;
    }
    relays.push(relay);
}

pub(crate) fn relays_for_public_publish(requested: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for relay in requested {
        push_public_relay(&mut relays, relay);
    }
    relays
}

pub(crate) fn relays_for_public_fetch(hints: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for relay in hints {
        push_public_relay(&mut relays, relay);
    }
    relays
}

pub(crate) fn relays_to_assign(page_relays: &[String], configured: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for relay in page_relays {
        push_public_relay(&mut relays, relay);
    }
    for relay in configured {
        push_public_relay(&mut relays, relay);
    }
    relays
}
