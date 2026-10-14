use super::*;

#[test]
fn about_lists_relay_hosts_and_omits_private_ones() {
    let html = render_about_section(
        &[
            "wss://relay.primal.net".to_string(),
            "wss://offchain.pub/".to_string(),
        ],
        false,
    );
    assert!(html.contains("relay.primal.net"));
    assert!(html.contains("offchain.pub"));
    assert!(html.contains("<h3>Relays</h3>"));
    assert!(!html.contains("relay-add-label"));
    assert!(!html.contains(">Yours</p>"));
    assert!(!html.contains("Nostr relays"));
    assert!(!html.contains("relay-add-btn"));
    assert!(!html.contains("relay-url-input"));
    assert!(!html.contains("relay-reshuffle"));
    assert!(!html.contains("Config.toml"));
    assert!(!html.contains("stored only in this browser"));

    let private = render_about_section(&["wss://127.0.0.1".to_string()], false);
    assert!(private.contains("No relays."));
}

#[test]
fn nojs_relay_form_posts_to_the_cookie_list() {
    let html = render_nojs_relay_form(
        &["wss://dealt.example".to_string()],
        &["wss://pin.example".to_string()],
        "token",
        "Use a public wss:// relay URL.",
        "wss://127.0.0.1",
    );
    let input = relay_input(&html);
    assert!(input.contains("name=\"relay\""));
    assert!(input.contains("value=\"wss://127.0.0.1\""));
    assert!(!input.contains("name=\"relay_box\""));
    assert!(!input.contains("form=\"publishForm\""));
    assert!(html.contains("action=\"/nojs/relays\""));
    assert!(html.contains("<li data-relay=\"wss://pin.example\">"));
    assert!(html.contains("<li data-relay=\"wss://dealt.example\">"));
    assert!(html.contains("name=\"remove\""));
    assert!(html.contains("aria-label=\"Remove relay\""));
    assert!(html.contains("<p class=\"relay-add-error\">Use a public wss:// relay URL.</p>"));
    assert!(!html.contains("/nojs/relay\""));
    assert!(!html.contains("name=\"relay_remove\""));
    assert!(!html.contains("formaction="));

    let relays = ["wss://relay.primal.net".to_string()];
    let about_html = render_about_section(&relays, false);
    assert!(about_html.contains("signs in your browser"));
    let nojs_about = render_about_section(&relays, true);
    assert!(nojs_about.contains("relay.primal.net"));
    assert!(nojs_about.contains("This host seals the note and sends it to these relays over Tor."));
    assert!(nojs_about.contains("Add or remove relays with the form."));
    assert!(nojs_about.contains("stays in this browser"));
    assert!(nojs_about.contains("sent with the note"));
    assert!(!nojs_about.contains("Config.toml"));
    assert!(!nojs_about.contains("signs in your browser"));
    assert!(!nojs_about.contains("stored only in this browser"));
    assert!(!nojs_about.contains("kept with this draft"));
    assert!(!nojs_about.contains("relay-add-btn"));
    assert!(!nojs_about.contains("relay-url-input"));
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
    assert!(html.contains("<span class=\"relay-count\">3/6</span>"));
    assert!(html.contains("class=\"relay-reshuffle\" aria-label=\"Reshuffle\""));
    assert!(html.contains("class=\"relay-count-up\" aria-label=\"More relays\""));
    assert!(html.contains("class=\"relay-count-down\" aria-label=\"Fewer relays\""));
    assert!(!html.contains("type=\"number\""));
    assert!(html.contains("<div class=\"relay-add\" data-relay-add>"));
    assert!(html.contains("class=\"relay-url-input\""));
    assert!(html.contains("<button type=\"button\" class=\"relay-add-btn\">Add</button>"));
    assert!(html.contains("<p class=\"relay-add-error\"></p>"));
    assert!(!html.contains("relay-delete-all"));

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
