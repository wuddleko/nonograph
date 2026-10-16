use crate::canonical_public_relay;

const MAX_LIST_URLS: usize = 20;

pub fn list_urls(event: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(event) else {
        return Vec::new();
    };
    if value.get("kind").and_then(|kind| kind.as_u64()) != Some(10002) {
        return Vec::new();
    }
    let Some(tags) = value.get("tags").and_then(|tags| tags.as_array()) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for entry in tags {
        let Some(parts) = entry.as_array() else {
            continue;
        };
        if parts.first().and_then(|name| name.as_str()) != Some("r") {
            continue;
        }
        let Some(raw) = parts.get(1).and_then(|url| url.as_str()) else {
            continue;
        };
        let Some(url) = canonical_public_relay(raw) else {
            continue;
        };
        if urls.iter().any(|have| have == &url) {
            continue;
        }
        if urls.len() == MAX_LIST_URLS {
            break;
        }
        urls.push(url);
    }
    urls
}
