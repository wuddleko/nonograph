use std::path::Path;

use rocket::response::content;

use crawler::deal;

use crate::crawl::{candidates, pool_path};
use crate::relays::render_dealt_rows;

const OPEN_SEATS: usize = 3;

pub(crate) fn relays_page_html(path: &Path) -> String {
    let urls = candidates(path);
    let dealt = deal(&urls, OPEN_SEATS, &[], &[]);
    let body = render_dealt_rows(&dealt, &urls);
    relays_document(&body)
}

fn relays_document(body: &str) -> String {
    let version = super::assets::version();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Relays</title>
    <link rel="icon" href="data:," />
    <script src="/theme.js?v={version}"></script>
    <link rel="stylesheet" href="/home.css?v={version}" />
</head>
<body class="relays-page">
    <h1>Relays</h1>
    {body}
    <script src="/relays.js?v={version}"></script>
</body>
</html>"#
    )
}

#[get("/relays")]
pub fn relays_page() -> content::RawHtml<String> {
    content::RawHtml(relays_page_html(&pool_path(Path::new("content"))))
}

#[cfg(test)]
mod tests {
    use crawler::{save, Pool, ProbeResult, Row, Visit};
    use serde_json::Value;

    use super::*;

    fn open_document() -> Value {
        serde_json::from_str(include_str!("../../test/nip11/open.json")).expect("nip11 fixture")
    }

    fn usable(url: &str, document: Value) -> Row {
        Row {
            url: url.to_owned(),
            first_seen: 1,
            next_try: 1,
            failures: 0,
            visit: Some(Visit {
                at: 1,
                socket_answered: true,
                nip11: Some(document),
                probes: vec![ProbeResult {
                    name: "30323".to_owned(),
                    accepted: true,
                    detail: "OK".to_owned(),
                }],
            }),
        }
    }

    #[test]
    fn a_fixture_pool_renders_the_first_three_relays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay-pool.json");
        let open = open_document();
        let candidates: Vec<String> = (0..5)
            .map(|index| format!("wss://relay{index}.example"))
            .collect();
        let rows = candidates
            .iter()
            .map(|url| usable(url, open.clone()))
            .collect();
        save(&path, &Pool { rows }).unwrap();

        let html = relays_page_html(&path);
        assert!(html.contains("<body class=\"relays-page\">"));
        assert!(html.contains("/home.css?v="));
        assert!(html.contains("/theme.js?v="));
        assert!(html.contains("/relays.js?v="));
        assert!(!html.contains("notice.css"));
        assert!(html.contains("<button type=\"button\" class=\"relay-add-btn\">Add</button>"));
        assert!(html.contains("<li data-relay=\"wss://relay0.example\">"));
        assert!(html.contains("<li data-relay=\"wss://relay1.example\">"));
        assert!(html.contains("<li data-relay=\"wss://relay2.example\">"));
        assert!(!html.contains("<li data-relay=\"wss://relay3.example\">"));
        assert!(!html.contains("<li data-relay=\"wss://relay4.example\">"));
        let json = serde_json::to_string(&candidates).unwrap();
        assert!(html.contains(&format!(
            "<script type=\"application/json\" id=\"relay-candidates\">{json}</script>"
        )));
    }

    #[test]
    fn an_empty_file_renders_the_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay-pool.json");
        std::fs::write(&path, "").unwrap();

        let html = relays_page_html(&path);
        assert!(html.contains("<body class=\"relays-page\">"));
        assert!(html.contains("/home.css?v="));
        assert!(html.contains("/relays.js?v="));
        assert!(!html.contains("notice.css"));
        assert!(html.contains("<p class=\"relay-empty\">No relays.</p>"));
        assert!(html.contains("<div class=\"relay-add\" data-relay-add>"));
        assert!(!html.contains("<li data-relay="));
        assert!(
            html.contains("<script type=\"application/json\" id=\"relay-candidates\">[]</script>")
        );
    }

    #[test]
    fn the_relay_list_round_trips_and_a_missing_store_is_empty() {
        let program = format!(
            "{source}\n{checks}\n",
            source = include_str!("../../templates/relays.js"),
            checks = r#"
const memory = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
const missing = readRelayList(memory);
if (missing.dealt.length !== 0 || missing.pinned.length !== 0) {
    throw new Error("missing store was not empty");
}
writeRelayList(memory, {
    dealt: ["wss://a.example", "wss://b.example", "wss://c.example"],
    pinned: ["wss://pin.example"],
});
const saved = readRelayList(memory);
const expected = {
    dealt: ["wss://a.example", "wss://b.example", "wss://c.example"],
    pinned: ["wss://pin.example"],
};
if (JSON.stringify(saved) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(saved));
}
"#,
        );
        run_javascript("nonograph-relay-list", &program);
    }

    #[test]
    fn the_script_deal_returns_the_first_two_that_pass() {
        let program = format!(
            "{source}\n{checks}\n",
            source = include_str!("../../templates/relays.js"),
            checks = r#"
const candidates = [
    "wss://a.example",
    "wss://pinned.example",
    "wss://removed.example",
    "wss://b.example",
    "wss://c.example",
];
const dealt = dealRelays(
    candidates,
    2,
    ["wss://pinned.example"],
    ["wss://removed.example"],
);
const expected = ["wss://a.example", "wss://b.example"];
if (JSON.stringify(dealt) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(dealt));
}
if (dealt.indexOf("wss://pinned.example") !== -1) {
    throw new Error("pinned url was dealt");
}
"#,
        );
        run_javascript("nonograph-relay-deal", &program);
    }

    #[test]
    fn an_empty_store_deals_three_and_a_second_load_returns_them() {
        let program = format!(
            "{source}\n{checks}\n",
            source = include_str!("../../templates/relays.js"),
            checks = r#"
const memory = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
const candidates = [
    "wss://a.example",
    "wss://b.example",
    "wss://c.example",
    "wss://d.example",
    "wss://e.example",
];
const first = loadRelayList(memory, candidates);
const expected = ["wss://a.example", "wss://b.example", "wss://c.example"];
if (JSON.stringify(first.dealt) !== JSON.stringify(expected) || first.pinned.length !== 0) {
    throw new Error(JSON.stringify(first));
}
const second = loadRelayList(memory, ["wss://other.example"]);
if (JSON.stringify(second.dealt) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(second));
}
const kept = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
writeRelayList(kept, {
    dealt: ["wss://kept.example"],
    pinned: ["wss://pin.example"],
});
const loaded = loadRelayList(kept, candidates);
if (JSON.stringify(loaded) !== JSON.stringify({
    dealt: ["wss://kept.example"],
    pinned: ["wss://pin.example"],
})) {
    throw new Error(JSON.stringify(loaded));
}
"#,
        );
        run_javascript("nonograph-relay-visit", &program);
    }

    #[test]
    fn the_relays_page_actions_call_the_saved_list() {
        let script = include_str!("../../templates/relays.js");
        for name in [
            "reshuffleRelays",
            "setRelayCount",
            "addRelay",
            "removeRelay",
            "deleteRelays",
        ] {
            let marker = format!("function {name}(");
            let start = script.find(&marker).unwrap_or_else(|| panic!("missing {name}"));
            let rest = &script[start + marker.len()..];
            let end = rest.find("\nfunction ").unwrap_or(rest.len());
            assert!(
                rest[..end].contains("writeRelayList"),
                "{name} does not write the saved list"
            );
            assert!(
                script.contains(&format!("{name}(storage")),
                "{name} is not called with the store"
            );
        }
    }

    #[test]
    fn a_hand_added_url_survives_a_reshuffle() {
        let program = format!(
            "{source}\n{checks}\n",
            source = include_str!("../../templates/relays.js"),
            checks = r#"
const memory = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
const candidates = [
    "wss://a.example",
    "wss://b.example",
    "wss://c.example",
    "wss://d.example",
    "wss://e.example",
];
loadRelayList(memory, candidates);
const added = addRelay(memory, "wss://hand.example");
if (added.pinned.indexOf("wss://hand.example") === -1) {
    throw new Error("hand added url was not pinned");
}
const refused = addRelay(memory, "wss://127.0.0.1");
if (refused.pinned.indexOf("wss://127.0.0.1") !== -1) {
    throw new Error("private url was pinned");
}
const again = reshuffleRelays(memory, candidates, 4);
if (again.pinned.indexOf("wss://hand.example") === -1) {
    throw new Error("hand added url did not survive reshuffle");
}
const low = setRelayCount(memory, candidates, 1);
if (low.pinned.indexOf("wss://hand.example") === -1 || low.dealt.length !== 0) {
    throw new Error(JSON.stringify(low));
}
const high = setRelayCount(memory, candidates, 9);
if (high.pinned.indexOf("wss://hand.example") === -1 || high.dealt.length + high.pinned.length > 6) {
    throw new Error(JSON.stringify(high));
}
const dropped = removeRelay(memory, "wss://a.example");
if (dropped.dealt.indexOf("wss://a.example") !== -1) {
    throw new Error("removed url stayed in the list");
}
const cleared = deleteRelays(memory);
if (cleared.dealt.length !== 0 || cleared.pinned.length !== 0) {
    throw new Error(JSON.stringify(cleared));
}
"#,
        );
        run_javascript("nonograph-relay-actions", &program);
    }

    fn run_javascript(prefix: &str, program: &str) {
        let path = std::env::temp_dir().join(format!("{prefix}-{}.js", std::process::id()));
        std::fs::write(&path, program).expect("write relay list fixture");
        let output = match std::process::Command::new("node").arg(&path).output() {
            Ok(output) if output.status.success() => output,
            _ => std::process::Command::new("osascript")
                .args(["-l", "JavaScript"])
                .arg(&path)
                .output()
                .expect("node or osascript must run the relay list"),
        };
        let _ = std::fs::remove_file(&path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
