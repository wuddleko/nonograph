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

#[test]
fn dealt_rows_list_three_relays_and_an_empty_slice_is_empty() {
    let html = render_dealt_rows(
        &[
            "wss://a.example".to_string(),
            "wss://b.example".to_string(),
            "wss://c.example".to_string(),
        ],
        &[],
    );
    assert!(html.contains("<li data-relay=\"wss://a.example\">"));
    assert!(html.contains("<li data-relay=\"wss://b.example\">"));
    assert!(html.contains("<li data-relay=\"wss://c.example\">"));
    assert_eq!(html.matches("class=\"relay-remove\"").count(), 3);
    assert!(!html.contains("relay-empty"));
    assert!(html.contains(
        "<input class=\"relay-count\" type=\"number\" value=\"3\" max=\"6\" aria-label=\"How many relays\" />"
    ));
    assert!(html.contains("<button type=\"button\" class=\"relay-reshuffle\">Reshuffle</button>"));
    assert!(html.contains("<div class=\"relay-add\" data-relay-add>"));
    assert!(html.contains("class=\"relay-url-input\""));
    assert!(html.contains("<button type=\"button\" class=\"relay-add-btn\">Add</button>"));
    assert!(html.contains("<p class=\"relay-add-error\"></p>"));
    assert!(html.contains("<button type=\"button\" class=\"relay-delete-all\">Delete all</button>"));

    let empty = render_dealt_rows(&[], &[]);
    assert!(empty.contains("<p class=\"relay-empty\">No relays.</p>"));
    assert!(!empty.contains("relay-remove"));
    assert!(!empty.contains("<li"));
}

#[test]
fn dealt_rows_embed_every_candidate_and_list_the_first_three() {
    let candidates = vec![
        "wss://a.example".to_string(),
        "wss://b.example".to_string(),
        "wss://c.example".to_string(),
        "wss://d.example".to_string(),
        "wss://e.example".to_string(),
    ];
    let html = render_dealt_rows(&candidates[..3], &candidates);
    assert!(html.contains("<li data-relay=\"wss://a.example\">"));
    assert!(html.contains("<li data-relay=\"wss://b.example\">"));
    assert!(html.contains("<li data-relay=\"wss://c.example\">"));
    assert!(!html.contains("<li data-relay=\"wss://d.example\">"));
    assert!(!html.contains("<li data-relay=\"wss://e.example\">"));
    let json = serde_json::to_string(&candidates).unwrap();
    assert!(html.contains(&format!(
        "<script type=\"application/json\" id=\"relay-candidates\">{json}</script>"
    )));
}

#[test]
fn dealt_rows_escape_a_script_close_in_the_candidate_json() {
    let candidates = vec!["wss://a.example/</script>".to_string()];
    let html = render_dealt_rows(&[], &candidates);
    assert!(html.contains(
        "<script type=\"application/json\" id=\"relay-candidates\">[\"wss://a.example/\\u003c/script>\"]</script>"
    ));
    assert_eq!(html.matches("</script>").count(), 1);
    assert!(!html.contains("</script><script"));
}

fn relay_input(html: &str) -> &str {
    let start = html.find("<input class=\"relay-url-input\"").unwrap();
    let rest = &html[start..];
    &rest[..rest.find('>').unwrap()]
}
