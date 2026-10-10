use crate::nostr::public_relay_url;
use nonograph_parser::{html_attr_escape, html_escape};

pub(crate) fn render_sidebar_field(
    relays: &[String],
    nojs: bool,
    saved: &str,
    typed: &str,
    list_relay: bool,
    relay_error: &str,
) -> String {
    let mut html = String::from(
        "<div class=\"sidebar-relays\" id=\"sidebar-relays\">\n\
        <p class=\"sidebar-relays-title\">Relays</p>\n",
    );
    html.push_str(&render_list(relays));
    html.push_str(&render_add_relay_controls(
        nojs,
        saved,
        typed,
        list_relay,
        relay_error,
    ));
    html.push_str("</div>\n");
    html
}

pub(crate) fn render_saved_relay_input(saved: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"relay\" value=\"{}\" />",
        html_attr_escape(saved)
    )
}

pub fn render_about_section(relays: &[String], nojs: bool) -> String {
    let mut html = String::from(
        "<div class=\"help-section\" id=\"relays\">\n\
        <h3>Relays</h3>\n",
    );
    if nojs {
        html.push_str("<p>This host seals the note and sends it to these relays over Tor.</p>\n");
    } else {
        html.push_str(
            "<p><strong>On Nostr</strong> signs in your browser; this host sends the note to these relays over Tor.</p>\n",
        );
    }
    html.push_str(&render_list(relays));
    if nojs {
        html.push_str(
            "<p>Add one more with the form. It is sent with the note and kept with this draft.</p>\n\
            <p>Operators can also set defaults in <code>Config.toml</code>.</p>\n",
        );
    } else {
        html.push_str(&render_add_relay_controls(false, "", "", false, ""));
        html.push_str(
            "<p>Operators can also set defaults in <code>Config.toml</code>. Yours are stored only in this browser.</p>\n",
        );
    }
    html.push_str("</div>\n");
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

fn render_add_relay_controls(
    nojs: bool,
    saved: &str,
    typed: &str,
    list_relay: bool,
    relay_error: &str,
) -> String {
    let relay_field = if nojs {
        let value = if typed.is_empty() {
            String::new()
        } else {
            format!(" value=\"{}\"", html_attr_escape(typed))
        };
        format!(" name=\"relay_box\" form=\"publishForm\"{value}")
    } else {
        String::new()
    };
    let list = if list_relay {
        let remove = if nojs {
            "<button type=\"submit\" class=\"relay-remove\" name=\"relay_remove\" value=\"on\" form=\"publishForm\" formaction=\"/nojs/relay\" formnovalidate aria-label=\"Remove relay\">×</button>"
        } else {
            ""
        };
        format!(
            "<ul class=\"relay-list relay-list-user\">\n<li data-relay=\"{}\"><span class=\"relay-name\">{}</span>{remove}</li>\n</ul>\n",
            html_attr_escape(saved),
            html_escape(&relay_label(saved)),
        )
    } else {
        "<ul class=\"relay-list relay-list-user\" hidden></ul>\n".to_string()
    };
    let button = if nojs {
        "<button type=\"submit\" class=\"relay-add-btn\" form=\"publishForm\" formaction=\"/nojs/relay\" formnovalidate>Add</button>"
    } else {
        "<button type=\"button\" class=\"relay-add-btn\">Add</button>"
    };
    let relay_error = html_escape(relay_error);
    format!(
        "<div class=\"relay-add\" data-relay-add>\n\
        {list}\
        <div class=\"relay-add-row\">\n\
        <input class=\"relay-url-input\" type=\"text\"{relay_field} inputmode=\"url\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"wss://…\" maxlength=\"255\" />\n\
        {button}\n\
        </div>\n\
        <p class=\"relay-add-error\">{relay_error}</p>\n\
        </div>\n"
    )
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
