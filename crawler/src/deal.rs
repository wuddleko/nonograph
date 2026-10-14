use crate::canonical_public_relay;
use crate::relay_url::publish_relay;

pub fn deal(
    candidates: &[String],
    open: usize,
    pinned: &[String],
    removed: &[String],
) -> Vec<String> {
    if open == 0 {
        return Vec::new();
    }
    let mut hosts = hosts_of(pinned);
    hosts.extend(hosts_of(removed));
    let mut chosen = Vec::new();
    for candidate in candidates {
        if chosen.len() == open {
            break;
        }
        let Some(url) = publish_relay(candidate) else {
            continue;
        };
        let Some(host) = canonical_public_relay(&url) else {
            continue;
        };
        if hosts.iter().any(|have| have == &host) {
            continue;
        }
        hosts.push(host);
        chosen.push(url);
    }
    chosen
}

fn hosts_of(urls: &[String]) -> Vec<String> {
    urls.iter()
        .filter_map(|url| canonical_public_relay(url))
        .collect()
}
