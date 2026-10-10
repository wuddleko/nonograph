use super::*;

#[test]
fn sidebar_lists_relay_hosts() {
    let html = render_sidebar_field(
        &[
            "wss://relay.primal.net".to_string(),
            "wss://offchain.pub/".to_string(),
        ],
        false,
        "",
        "",
        false,
        "",
    );
    assert!(html.contains("relay.primal.net"));
    assert!(html.contains("offchain.pub"));
    assert!(html.contains("sidebar-relays"));
    assert!(html.contains("relay-url-input"));
    assert!(html.contains(">Relays</p>"));
    assert!(!html.contains("relay-add-label"));
    assert!(!html.contains(">Yours</p>"));
    assert!(!html.contains("Nostr relays"));
}

#[test]
fn private_relays_are_omitted() {
    let html = render_sidebar_field(&["wss://127.0.0.1".to_string()], false, "", "", false, "");
    assert!(html.contains("No public relays"));
}

#[test]
fn nojs_sidebar_names_the_relay_field() {
    let relays = ["wss://relay.primal.net".to_string()];
    let nojs_html = render_sidebar_field(&relays, true, "", "", false, "");
    let nojs = relay_input(&nojs_html);
    assert!(nojs.contains("name=\"relay_box\""));
    assert!(nojs.contains("form=\"publishForm\""));
    assert!(!nojs
        .replace("name=\"relay_box\"", "")
        .contains("name=\"relay\""));
    assert!(nojs.contains("placeholder=\"wss://…\""));
    assert!(nojs_html.contains(
        "<button type=\"submit\" class=\"relay-add-btn\" form=\"publishForm\" formaction=\"/nojs/relay\" formnovalidate>Add</button>"
    ));
    assert!(!nojs_html.contains("type=\"button\" class=\"relay-add-btn\""));

    let listed = render_sidebar_field(&relays, true, "wss://relay.damus.io", "", true, "");
    let listed_input = relay_input(&listed);
    assert!(!listed_input.contains("value="));
    assert!(listed.contains("<li data-relay=\"wss://relay.damus.io\">"));
    assert!(listed.contains("name=\"relay_remove\""));
    assert!(listed.contains("formaction=\"/nojs/relay\""));
    assert!(listed.contains("formnovalidate"));
    assert!(listed.contains("aria-label=\"Remove relay\""));

    let js_html = render_sidebar_field(&relays, false, "", "", false, "");
    let js = relay_input(&js_html);
    assert!(!js.contains("name="));
    assert!(!js.contains("form="));

    let about_html = render_about_section(&relays, false);
    assert!(about_html.contains("<h3>Relays</h3>"));
    assert!(!about_html.contains("Nostr relays"));
    assert!(!about_html.contains("relay-add-label"));
    let about = relay_input(&about_html);
    assert!(!about.contains("name="));
    assert!(!about.contains("form="));
    assert!(about_html.contains("type=\"button\" class=\"relay-add-btn\""));

    let nojs_about = render_about_section(&relays, true);
    assert!(nojs_about.contains("relay.primal.net"));
    assert!(nojs_about.contains("This host seals the note and sends it to these relays over Tor."));
    assert!(nojs_about.contains("Add one more with the form."));
    assert!(nojs_about.contains("kept with this draft"));
    assert!(!nojs_about.contains("signs in your browser"));
    assert!(!nojs_about.contains("stored only in this browser"));
    assert!(!nojs_about.contains("relay-add-btn"));
    assert!(!nojs_about.contains("relay-url-input"));
    assert!(about_html.contains("stored only in this browser"));

    let erred = render_sidebar_field(
        &relays,
        true,
        "wss://nos.lol",
        "wss://127.0.0.1",
        true,
        "Use a public wss:// relay URL.",
    );
    assert!(erred.contains("<p class=\"relay-add-error\">Use a public wss:// relay URL.</p>"));
}

fn relay_input(html: &str) -> &str {
    let start = html.find("<input class=\"relay-url-input\"").unwrap();
    let rest = &html[start..];
    &rest[..rest.find('>').unwrap()]
}
