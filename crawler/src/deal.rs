use crate::canonical_public_relay;

pub fn deal(
    candidates: &[String],
    open: usize,
    pinned: &[String],
    removed: &[String],
) -> Vec<String> {
    if open == 0 {
        return Vec::new();
    }
    let mut chosen = Vec::new();
    for candidate in candidates {
        if chosen.len() == open {
            break;
        }
        let Some(url) = canonical_public_relay(candidate) else {
            continue;
        };
        if skipped(&url, pinned, removed) {
            continue;
        }
        chosen.push(url);
    }
    chosen
}

fn skipped(url: &str, pinned: &[String], removed: &[String]) -> bool {
    pinned
        .iter()
        .chain(removed.iter())
        .any(|raw| canonical_public_relay(raw).as_deref() == Some(url))
}
