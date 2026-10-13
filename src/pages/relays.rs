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
        assert!(!html.contains("notice.css"));
        assert!(html.contains("<p class=\"relay-empty\">No relays.</p>"));
        assert!(html.contains("<div class=\"relay-add\" data-relay-add>"));
        assert!(!html.contains("<li data-relay="));
        assert!(
            html.contains("<script type=\"application/json\" id=\"relay-candidates\">[]</script>")
        );
    }
}
