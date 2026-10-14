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
            "<p>Add one more with the form. It is sent with the note and kept with this draft.</p>\n",
        );
    }
    html.push_str("</div>\n");
    html
}

pub(crate) fn render_dealt_rows(relays: &[String], candidates: &[String]) -> String {
    let mut items = String::new();
    for relay in relays {
        if !public_relay_url(relay) {
            continue;
        }
        let label = relay_label(relay);
        items.push_str("<li data-relay=\"");
        items.push_str(&html_attr_escape(relay));
        items.push_str("\"><span class=\"relay-name\">");
        items.push_str(&html_escape(&label));
        items.push_str(
            "</span><button type=\"button\" class=\"relay-remove\" aria-label=\"Remove relay\">×</button></li>\n",
        );
    }
    let list = if items.is_empty() {
        "<p class=\"relay-empty\">No relays.</p>\n".to_string()
    } else {
        format!("<ul class=\"relay-list relay-list-user\">\n{items}</ul>\n")
    };
    format!(
        "{list}\
        <div class=\"relay-deal-controls\">\n\
        {count_row}\
        <div class=\"relay-add\" data-relay-add>\n\
        <div class=\"relay-add-row\">\n\
        <input class=\"relay-url-input\" type=\"text\" inputmode=\"url\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"wss://…\" maxlength=\"255\" />\n\
        <button type=\"button\" class=\"relay-add-btn\">Add</button>\n\
        </div>\n\
        <p class=\"relay-add-error\"></p>\n\
        </div>\n\
        </div>\n\
        {candidates}",
        count_row = relay_count_row(3),
        candidates = candidate_script(candidates),
    )
}

pub(crate) fn render_nojs_relay_form(
    dealt: &[String],
    pinned: &[String],
    csrf_token: &str,
    add_error: &str,
    typed: &str,
) -> String {
    let mut items = String::new();
    for relay in pinned.iter().chain(dealt.iter()) {
        if !public_relay_url(relay) {
            continue;
        }
        let label = relay_label(relay);
        items.push_str("<li data-relay=\"");
        items.push_str(&html_attr_escape(relay));
        items.push_str("\"><span class=\"relay-name\">");
        items.push_str(&html_escape(&label));
        items.push_str(
            "</span><form class=\"relay-remove-form\" method=\"post\" action=\"/nojs/relays\">",
        );
        items.push_str(&hidden_csrf(csrf_token));
        items.push_str("<button type=\"submit\" class=\"relay-remove\" name=\"remove\" value=\"");
        items.push_str(&html_attr_escape(relay));
        items.push_str("\" aria-label=\"Remove relay\">×</button></form></li>\n");
    }
    let list = if items.is_empty() {
        "<p class=\"relay-empty\">No relays.</p>\n".to_string()
    } else {
        format!("<ul class=\"relay-list relay-list-user\">\n{items}</ul>\n")
    };
    let total = pinned.len() + dealt.len();
    let csrf = hidden_csrf(csrf_token);
    let add_error = html_escape(add_error);
    let typed_attr = if typed.is_empty() {
        String::new()
    } else {
        format!(" value=\"{}\"", html_attr_escape(typed))
    };
    format!(
        "{list}\
        <div class=\"relay-deal-controls\">\n\
        {count_row}\
        <form class=\"relay-add\" data-relay-add method=\"post\" action=\"/nojs/relays\">\n\
        {csrf}\
        <div class=\"relay-add-row\">\n\
        <input class=\"relay-url-input\" type=\"text\" name=\"relay\"{typed_attr} inputmode=\"url\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"wss://…\" maxlength=\"255\" />\n\
        <button type=\"submit\" class=\"relay-add-btn\" name=\"action\" value=\"add\">Add</button>\n\
        </div>\n\
        <p class=\"relay-add-error\">{add_error}</p>\n\
        </form>\n\
        </div>\n",
        count_row = nojs_count_row(total, pinned.len(), &csrf),
    )
}

fn reshuffle_icon() -> &'static str {
    r#"<svg class="relay-wheel" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M20 12a8 8 0 1 1-2.2-5.6"/><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M20 4v5h-5"/></svg>"#
}

fn count_mark(up: bool) -> &'static str {
    if up { "+" } else { "−" }
}

fn relay_count_row(total: usize) -> String {
    format!(
        "<div class=\"relay-count-row\">\n\
        <button type=\"button\" class=\"relay-reshuffle\" aria-label=\"Reshuffle\">{icon}</button>\n\
        <span class=\"relay-count-step\">\n\
        <button type=\"button\" class=\"relay-count-down\" aria-label=\"Fewer relays\">{down}</button>\n\
        <span class=\"relay-count\">{total}/6</span>\n\
        <button type=\"button\" class=\"relay-count-up\" aria-label=\"More relays\">{up}</button>\n\
        </span>\n\
        </div>\n",
        icon = reshuffle_icon(),
        up = count_mark(true),
        down = count_mark(false),
    )
}

fn nojs_count_row(total: usize, floor: usize, csrf: &str) -> String {
    let up_disabled = if total >= 6 { " disabled" } else { "" };
    let down_disabled = if total <= floor { " disabled" } else { "" };
    let quiet = if total == 0 {
        " relay-reshuffle-quiet"
    } else {
        ""
    };
    format!(
        "<form class=\"relay-count-row\" method=\"post\" action=\"/nojs/relays\">\n\
        {csrf}\
        <input type=\"hidden\" name=\"count\" value=\"{total}\" />\n\
        <button type=\"submit\" class=\"relay-reshuffle{quiet}\" name=\"action\" value=\"reshuffle\" aria-label=\"Reshuffle\">{icon}</button>\n\
        <span class=\"relay-count-step\">\n\
        <button type=\"submit\" class=\"relay-count-down\" name=\"action\" value=\"down\" aria-label=\"Fewer relays\"{down_disabled}>{down}</button>\n\
        <span class=\"relay-count\">{total}/6</span>\n\
        <button type=\"submit\" class=\"relay-count-up\" name=\"action\" value=\"up\" aria-label=\"More relays\"{up_disabled}>{up}</button>\n\
        </span>\n\
        </form>\n",
        icon = reshuffle_icon(),
        up = count_mark(true),
        down = count_mark(false),
    )
}

fn hidden_csrf(token: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"csrf_token\" value=\"{}\" />\n",
        html_attr_escape(token)
    )
}

fn candidate_script(candidates: &[String]) -> String {
    let json = serde_json::to_string(candidates)
        .unwrap_or_else(|_| "[]".to_string())
        .replace('<', "\\u003c");
    format!("<script type=\"application/json\" id=\"relay-candidates\">{json}</script>\n")
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
        return "<p class=\"relay-empty\">No relays.</p>\n".to_string();
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
