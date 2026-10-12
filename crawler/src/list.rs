use crate::canonical_public_relay;

pub fn list_urls(event: &str) -> Vec<String> {
    list_urls_with(event, 10002, "r")
}

pub fn list_urls_with(event: &str, kind: u32, tag: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(event) else {
        return Vec::new();
    };
    if value.get("kind").and_then(|kind| kind.as_u64()) != Some(u64::from(kind)) {
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
        if parts.first().and_then(|name| name.as_str()) != Some(tag) {
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
        urls.push(url);
    }
    urls
}
