use std::collections::HashMap;

use chrono::{DateTime, Utc};

pub(crate) fn post_description(raw: &str) -> String {
    let Some((end, _)) = raw.char_indices().nth(160) else {
        return raw.to_string();
    };
    let mut description = raw[..end].to_string();
    description.push_str("...");
    description
}

pub(crate) fn article_context(
    title: &str,
    author: &str,
    created_at: Option<DateTime<Utc>>,
    url: &str,
    description: &str,
    nojs: bool,
    public_id: &str,
    origin: &str,
) -> HashMap<String, String> {
    let author = if author.is_empty() {
        "Anonymous".to_string()
    } else {
        author.to_string()
    };
    let mut context = HashMap::new();
    context.insert("title".to_string(), title.to_string());
    context.insert("author".to_string(), author.clone());
    context.insert("author_display".to_string(), format!("{author} · "));
    context.insert(
        "created_at".to_string(),
        created_at
            .map(|time| time.format("%B %d, %Y").to_string())
            .unwrap_or_default(),
    );
    context.insert(
        "created_at_iso".to_string(),
        created_at
            .map(|time| time.format("%Y-%m-%dT00:00:00+00:00").to_string())
            .unwrap_or_default(),
    );
    context.insert("url".to_string(), join_public_origin(origin, url));
    context.insert("description".to_string(), description.to_string());
    fill_layout(&mut context, nojs, public_id);
    context
}

pub(crate) fn fill_layout(context: &mut HashMap<String, String>, nojs: bool, public_id: &str) {
    let version = super::assets::version();
    context.insert("asset_version".to_string(), version.to_string());
    context.insert(
        "scripts".to_string(),
        if nojs {
            String::new()
        } else {
            super::assets::post_scripts(version)
        },
    );
    context.insert("mode_href".to_string(), mode_href(nojs, public_id));
    context.insert("mode_label".to_string(), mode_label(nojs).to_string());
    context.insert("fallback_css".to_string(), post_fallback_css(nojs, version));
}

pub(super) fn join_public_origin(origin: &str, path: &str) -> String {
    if origin.is_empty() {
        path.to_string()
    } else {
        format!("{origin}{path}")
    }
}

pub(crate) fn mode_href(nojs: bool, public_id: &str) -> String {
    if nojs {
        format!("/{public_id}")
    } else {
        format!("/nojs/{public_id}")
    }
}

pub(crate) fn mode_label(nojs: bool) -> &'static str {
    if nojs {
        "js"
    } else {
        "nojs"
    }
}

fn query_escape(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0xf) as usize] as char);
            }
        }
    }
    out
}

pub(super) fn js_seal_href(locator: &str, event_id: &str, k: &str, hints: &[String]) -> String {
    let mut href = format!("/s/{locator}/{event_id}");
    let mut first = true;
    for relay in hints {
        let Some(relay) = crate::nostr::canonical_public_relay(relay) else {
            continue;
        };
        href.push(if first { '?' } else { '&' });
        first = false;
        href.push_str("r=");
        href.push_str(&query_escape(&relay));
    }
    href.push('#');
    href.push_str(k);
    href
}

fn post_fallback_css(nojs: bool, version: &str) -> String {
    if nojs {
        format!("<link rel=\"stylesheet\" href=\"/post-noscript.css?v={version}\" />")
    } else {
        String::new()
    }
}

pub(super) fn centered_notice(document_title: &str, heading: &str, body: &str) -> String {
    let version = super::assets::version();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{document_title}</title>
    <link rel="icon" href="data:," />
    <link rel="stylesheet" href="/notice.css?v={version}" />
</head>
<body>
    <h1>{heading}</h1>
    {body}
</body>
</html>"#
    )
}

pub(crate) fn not_found_html() -> String {
    centered_notice(
        "404 - Page not found",
        "Page Not Found",
        "<p><a href=\"/\">Write Your Own</a></p>",
    )
}
