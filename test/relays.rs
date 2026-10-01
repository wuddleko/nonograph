use super::*;

#[test]
fn sidebar_lists_relay_hosts() {
    let html = render_sidebar(&[
        "wss://relay.primal.net".to_string(),
        "wss://offchain.pub/".to_string(),
    ]);
    assert!(html.contains("relay.primal.net"));
    assert!(html.contains("offchain.pub"));
    assert!(html.contains("sidebar-relays"));
    assert!(html.contains("relay-url-input"));
    assert!(html.contains("relay-add-label"));
}

#[test]
fn private_relays_are_omitted() {
    let html = render_sidebar(&["wss://127.0.0.1".to_string()]);
    assert!(html.contains("No public relays"));
}
