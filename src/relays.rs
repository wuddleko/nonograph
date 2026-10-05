use crate::nostr::public_relay_url;
use nonograph_parser::html_escape;

pub fn render_sidebar(relays: &[String]) -> String {
    let mut html = String::from(
        "<div class=\"sidebar-relays\" id=\"relays\">\n\
        <p class=\"sidebar-relays-title\">Nostr relays</p>\n",
    );
    html.push_str(&render_list(relays));
    html.push_str(render_add_relay_controls());
    html.push_str("</div>\n");
    html
}

pub fn render_about_section(relays: &[String]) -> String {
    let mut html = String::from(
        "<div class=\"help-section\" id=\"relays\">\n\
        <h3>Nostr relays</h3>\n\
        <p><strong>On Nostr</strong> signs in your browser; this host sends the note to these relays over Tor.</p>\n",
    );
    html.push_str(&render_list(relays));
    html.push_str(render_add_relay_controls());
    html.push_str(
        "<p>Operators can also set defaults in <code>Config.toml</code>. Yours are stored only in this browser.</p>\n\
        </div>\n",
    );
    html
}

fn render_list(relays: &[String]) -> String {
    let mut items = String::new();
    for relay in relays {
        if !public_relay_url(relay) {
            continue;
        }
        let label = relay_label(relay);
        items.push_str("<li data-relay=\"");
        items.push_str(&html_escape(relay));
        items.push_str("\">");
        items.push_str(&html_escape(&label));
        items.push_str("</li>\n");
    }
    if items.is_empty() {
        return "<p class=\"relay-empty\">No public relays configured.</p>\n".to_string();
    }
    format!("<ul class=\"relay-list\">\n{items}</ul>\n")
}

fn render_add_relay_controls() -> &'static str {
    "<div class=\"relay-add\" data-relay-add>\n\
        <p class=\"relay-add-label\">Yours</p>\n\
        <ul class=\"relay-list relay-list-user\" hidden></ul>\n\
        <div class=\"relay-add-row\">\n\
        <input class=\"relay-url-input\" type=\"text\" inputmode=\"url\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"wss://…\" maxlength=\"255\" />\n\
        <button type=\"button\" class=\"relay-add-btn\">Add</button>\n\
        </div>\n\
        <p class=\"relay-add-error\"></p>\n\
        </div>\n"
}

fn relay_label(url: &str) -> String {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .unwrap_or(url)
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
#[path = "../test/relays.rs"]
mod tests;
