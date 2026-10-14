use crate::relay_url::{canonical_public_relay, relay_host};

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
        let Some(url) = canonical_public_relay(candidate) else {
            continue;
        };
        let Some(host) = relay_host(&url) else {
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
    urls.iter().filter_map(|url| relay_host(url)).collect()
}
