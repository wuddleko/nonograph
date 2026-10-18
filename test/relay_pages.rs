use crawler::{save, Pool, ProbeResult, Row, Visit};
use rocket::http::{ContentType, Status};
use serde_json::Value;

use super::*;

#[test]
fn a_long_relay_cookie_is_cut_to_six_and_the_count_does_not_panic() {
    let pinned: Vec<_> = (0..8)
        .map(|index| format!("wss://pin{index}.example"))
        .collect();
    let dealt: Vec<_> = (0..4)
        .map(|index| format!("wss://dealt{index}.example"))
        .collect();
    let raw = serde_json::json!({ "pinned": pinned, "dealt": dealt }).to_string();
    let cookie = read_relay_cookie(Some(&raw));
    assert_eq!(cookie.pinned.len(), RELAY_CAP);
    assert!(cookie.dealt.is_empty());
    assert_eq!(bounded_count(8, "1"), RELAY_CAP);
    assert_eq!(bounded_count(2, "9"), RELAY_CAP);
    assert_eq!(bounded_count(2, "nope"), 2);
}

#[test]
fn an_https_forward_marks_the_relay_and_csrf_cookies_secure() {
    use super::super::nojs_index;
    let config = crate::config::Config::default();
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![nojs_index]),
    )
    .unwrap();
    let plain = client.get("/nojs").dispatch();
    assert_eq!(plain.status(), Status::Ok);
    let relay = plain
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("relay cookie");
    assert_eq!(relay.secure(), None);
    let csrf = plain.cookies().get(crate::csrf::COOKIE_NAME);
    if let Some(csrf) = csrf {
        assert_eq!(csrf.secure(), None);
    }

    let https_client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![nojs_index]),
    )
    .unwrap();
    let https = https_client
        .get("/nojs")
        .header(rocket::http::Header::new("X-Forwarded-Proto", "https"))
        .dispatch();
    assert_eq!(https.status(), Status::Ok);
    let relay = https
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("relay cookie");
    assert_eq!(relay.secure(), Some(true));
    let csrf = https
        .cookies()
        .get(crate::csrf::COOKIE_NAME)
        .expect("csrf cookie");
    assert_eq!(csrf.secure(), Some(true));
}

#[test]
fn the_relay_cookie_round_trips_and_a_missing_cookie_is_empty() {
    let list = RelayCookie {
        dealt: vec![
            "wss://a.example".to_string(),
            "wss://b.example".to_string(),
            "wss://c.example".to_string(),
        ],
        pinned: vec!["wss://pin.example".to_string()],
    };
    let saved = write_relay_cookie(&list);
    assert_eq!(read_relay_cookie(Some(&saved)), list);
    assert_eq!(read_relay_cookie(None), RelayCookie::empty());
    assert_eq!(read_relay_cookie(Some("")), RelayCookie::empty());
    assert_eq!(read_relay_cookie(Some("not json")), RelayCookie::empty());
}

fn open_document() -> Value {
    serde_json::from_str(include_str!("nip11/open.json")).expect("nip11 fixture")
}

fn usable(url: &str, document: Value) -> Row {
    Row {
        url: url.to_owned(),
        first_seen: 1,
        next_try: 1,
        failures: 0,
        publish_refused: false,
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

    let html = relays_page_html(&path, &[]);
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

    let html = relays_page_html(&path, &[]);
    assert!(html.contains("<body class=\"relays-page\">"));
    assert!(html.contains("/home.css?v="));
    assert!(html.contains("/relays.js?v="));
    assert!(!html.contains("notice.css"));
    assert!(html.contains("<p class=\"relay-empty\">No relays.</p>"));
    assert!(html.contains("<div class=\"relay-add\" data-relay-add>"));
    assert!(!html.contains("<li data-relay="));
    assert!(html.contains("<script type=\"application/json\" id=\"relay-candidates\">[]</script>"));
}

#[test]
fn an_empty_cookie_deals_three_rows_and_a_second_read_returns_them() {
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

    let first = nojs_relays_view(&path, None, "token", &[]);
    assert!(first.write_cookie);
    assert!(first.html.contains("<body class=\"relays-page\">"));
    assert!(first.html.contains("action=\"/nojs/relays\""));
    assert!(first
        .html
        .contains("type=\"submit\" class=\"relay-reshuffle\""));
    assert!(first
        .html
        .contains("type=\"submit\" class=\"relay-add-btn\""));
    assert!(first.html.contains("class=\"relay-count\">3/6</span>"));
    assert!(!first.html.contains("relay-delete-all"));
    assert!(first
        .html
        .contains("type=\"submit\" class=\"relay-remove\""));
    assert!(first.html.contains("name=\"count\""));
    assert!(!first.html.contains("type=\"button\""));
    assert!(!first.html.contains("/relays.js"));
    assert!(first
        .html
        .contains("<li data-relay=\"wss://relay0.example\">"));
    assert!(first
        .html
        .contains("<li data-relay=\"wss://relay1.example\">"));
    assert!(first
        .html
        .contains("<li data-relay=\"wss://relay2.example\">"));
    assert!(!first
        .html
        .contains("<li data-relay=\"wss://relay3.example\">"));
    assert_eq!(
        first.cookie.dealt,
        vec![
            "wss://relay0.example".to_string(),
            "wss://relay1.example".to_string(),
            "wss://relay2.example".to_string(),
        ]
    );
    assert!(first.cookie.pinned.is_empty());

    let saved = write_relay_cookie(&first.cookie);
    assert_eq!(read_relay_cookie(Some(&saved)), first.cookie);
    let second = nojs_relays_view(&path, Some(&saved), "token", &[]);
    assert!(!second.write_cookie);
    assert_eq!(second.cookie, first.cookie);
    assert!(second
        .html
        .contains("<li data-relay=\"wss://relay0.example\">"));
    assert!(second
        .html
        .contains("<li data-relay=\"wss://relay1.example\">"));
    assert!(second
        .html
        .contains("<li data-relay=\"wss://relay2.example\">"));
    assert!(!second
        .html
        .contains("<li data-relay=\"wss://relay3.example\">"));

    let kept = RelayCookie {
        dealt: vec!["wss://relay4.example".to_string()],
        pinned: vec!["wss://pin.example".to_string()],
    };
    let kept_page = nojs_relays_view(&path, Some(&write_relay_cookie(&kept)), "token", &[]);
    assert!(!kept_page.write_cookie);
    assert_eq!(kept_page.cookie, kept);
    assert!(kept_page
        .html
        .contains("<li data-relay=\"wss://pin.example\">"));
    assert!(kept_page
        .html
        .contains("<li data-relay=\"wss://relay4.example\">"));
    assert!(!kept_page
        .html
        .contains("<li data-relay=\"wss://relay0.example\">"));

    let cleared = nojs_relays_view(&path, Some(r#"{"dealt":[],"pinned":[]}"#), "token", &[]);
    assert!(!cleared.write_cookie);
    assert_eq!(cleared.cookie, RelayCookie::empty());
    assert!(cleared
        .html
        .contains("<p class=\"relay-empty\">No relays.</p>"));
    assert!(!cleared.html.contains("<li data-relay="));

    let bad = nojs_relays_view(&path, Some("not json"), "token", &[]);
    assert!(bad.write_cookie);
    assert_eq!(bad.cookie.dealt, first.cookie.dealt);
    assert!(bad.cookie.pinned.is_empty());
}

fn posted_home(response: rocket::local::blocking::LocalResponse<'_>) -> (String, Option<String>) {
    assert_eq!(response.status(), Status::SeeOther);
    let location = response
        .headers()
        .get_one("Location")
        .unwrap_or("")
        .to_string();
    assert!(
        location == "/nojs" || location.starts_with("/nojs?"),
        "{location}"
    );
    let cookie = response
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .map(|cookie| cookie.value().to_string());
    (location, cookie)
}

#[test]
fn a_form_adds_one_public_url() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let opened = first
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("get writes the cookie")
        .value()
        .to_string();
    let dealt = read_relay_cookie(Some(&opened)).dealt;
    let token = csrf_field(&first.into_string().unwrap());

    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2FHand.Example%2F&csrf_token={token}"
        ))
        .dispatch();
    let (location, saved) = posted_home(added);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("add writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert_eq!(list.dealt, dealt);
    assert_eq!(list.pinned, vec!["wss://hand.example".to_string()]);
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(body.contains("name=\"relay\" inputmode=\"url\""));
    assert!(!body.contains("/relays.js"));

    let refused = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2F127.0.0.1&csrf_token={token}"
        ))
        .dispatch();
    let (location, _) = posted_home(refused);
    assert!(location.contains("error=relay_invalid"));
    assert!(location.contains("typed=wss%3A%2F%2F127.0.0.1"));

    let kept = client.get("/nojs/relays").dispatch();
    assert_eq!(kept.status(), Status::Ok);
    let kept_body = kept.into_string().unwrap();
    assert!(kept_body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(!kept_body.contains("127.0.0.1"));
}

#[test]
fn a_form_removes_the_url_it_added() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let opened = first
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("get writes the cookie")
        .value()
        .to_string();
    let dealt = read_relay_cookie(Some(&opened)).dealt;
    let token = csrf_field(&first.into_string().unwrap());

    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2FHand.Example%2F&csrf_token={token}"
        ))
        .dispatch();
    let (location, _) = posted_home(added);
    assert_eq!(location, "/nojs");
    let added_body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(added_body.contains("<li data-relay=\"wss://hand.example\">"));

    let removed = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "remove=wss%3A%2F%2Fhand.example&csrf_token={token}"
        ))
        .dispatch();
    let (location, saved) = posted_home(removed);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("remove writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert_eq!(list.dealt, dealt);
    assert!(list.pinned.is_empty());
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(!body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(!body.contains("/relays.js"));

    let token = crate::csrf::generate_csrf_token_with_timestamp();
    let seeded = RelayCookie {
        dealt: vec!["wss://dealt.example".to_string()],
        pinned: vec!["wss://hand.example".to_string()],
    };
    let bare = rocket::local::blocking::Client::untracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();
    let dropped = bare
        .post("/nojs/relays")
        .cookie((csrf::COOKIE_NAME, token.clone()))
        .cookie((RELAY_LIST_COOKIE, write_relay_cookie(&seeded)))
        .header(ContentType::Form)
        .body(format!(
            "remove=wss%3A%2F%2Fdealt.example&csrf_token={token}"
        ))
        .dispatch();
    let (location, saved) = posted_home(dropped);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("remove writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert!(list.dealt.is_empty());
    assert_eq!(list.pinned, vec!["wss://hand.example".to_string()]);
    let body = bare
        .get("/nojs/relays")
        .cookie((RELAY_LIST_COOKIE, saved))
        .dispatch()
        .into_string()
        .unwrap();
    assert!(!body.contains("<li data-relay=\"wss://dealt.example\">"));
    assert!(body.contains("<li data-relay=\"wss://hand.example\">"));
}

#[test]
fn a_form_deletes() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let token = csrf_field(&first.into_string().unwrap());
    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2FHand.Example%2F&csrf_token={token}"
        ))
        .dispatch();
    let (location, _) = posted_home(added);
    assert_eq!(location, "/nojs");

    let deleted = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!("action=delete&csrf_token={token}"))
        .dispatch();
    let (location, saved) = posted_home(deleted);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("delete writes the cookie");
    assert_eq!(read_relay_cookie(Some(&saved)), RelayCookie::empty());
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(body.contains("<p class=\"relay-empty\">No relays.</p>"));
    assert!(!body.contains("<li data-relay="));
    assert!(!body.contains("/relays.js"));

    let token = crate::csrf::generate_csrf_token_with_timestamp();
    let seeded = RelayCookie {
        dealt: vec!["wss://dealt.example".to_string()],
        pinned: vec!["wss://hand.example".to_string()],
    };
    let bare = rocket::local::blocking::Client::untracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();
    let cleared = bare
        .post("/nojs/relays")
        .cookie((csrf::COOKIE_NAME, token.clone()))
        .cookie((RELAY_LIST_COOKIE, write_relay_cookie(&seeded)))
        .header(ContentType::Form)
        .body(format!("action=delete&csrf_token={token}"))
        .dispatch();
    let (location, saved) = posted_home(cleared);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("delete writes the cookie");
    assert_eq!(read_relay_cookie(Some(&saved)), RelayCookie::empty());
    let body = bare
        .get("/nojs/relays")
        .cookie((RELAY_LIST_COOKIE, saved.clone()))
        .dispatch()
        .into_string()
        .unwrap();
    assert!(!body.contains("<li data-relay=\"wss://dealt.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(body.contains("<p class=\"relay-empty\">No relays.</p>"));

    let again = bare
        .get("/nojs/relays")
        .cookie((RELAY_LIST_COOKIE, saved))
        .dispatch();
    assert_eq!(again.status(), Status::Ok);
    if let Some(cookie) = again.cookies().get(RELAY_LIST_COOKIE) {
        assert_eq!(
            read_relay_cookie(Some(cookie.value())),
            RelayCookie::empty()
        );
    }
    let body = again.into_string().unwrap();
    assert!(!body.contains("<li data-relay=\"wss://dealt.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(!body.contains("<li data-relay="));
    assert!(body.contains("<p class=\"relay-empty\">No relays.</p>"));
}

#[test]
fn a_form_assigns_the_relays_that_remain_and_drops_the_one_that_left() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let seeded = RelayCookie {
        pinned: vec!["wss://kept.example".to_string()],
        dealt: vec![
            "wss://stay.example".to_string(),
            "wss://left.example".to_string(),
        ],
    };
    let watch = crate::nostr::RelayAssignmentWatch::begin(&[
        "wss://kept.example",
        "wss://stay.example",
        "wss://left.example",
    ]);
    let client = rocket::local::blocking::Client::untracked(rocket::build().manage(config).mount(
        "/",
        routes![super::nojs_relays_page, super::nojs_relays_update],
    ))
    .unwrap();
    let updated = client
        .post("/nojs/relays")
        .cookie((RELAY_LIST_COOKIE, write_relay_cookie(&seeded)))
        .header(ContentType::Form)
        .body("remove=wss%3A%2F%2Fleft.example")
        .dispatch();
    let (location, saved) = posted_home(updated);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("remove writes the cookie");
    assert_eq!(
        read_relay_cookie(Some(&saved)),
        RelayCookie {
            pinned: vec!["wss://kept.example".to_string()],
            dealt: vec!["wss://stay.example".to_string()],
        }
    );
    let body = client
        .get("/nojs/relays")
        .cookie((RELAY_LIST_COOKIE, saved))
        .dispatch()
        .into_string()
        .unwrap();
    assert!(body.contains("<li data-relay=\"wss://kept.example\">"));
    assert!(body.contains("<li data-relay=\"wss://stay.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://left.example\">"));
    assert_eq!(
        watch.assigned(),
        vec![
            "wss://kept.example".to_string(),
            "wss://stay.example".to_string(),
        ]
    );
    assert!(watch.remembers("wss://kept.example"));
    assert!(watch.remembers("wss://stay.example"));
    assert!(!watch.remembers("wss://left.example"));
    assert!(!watch.remembers("wss://config.example"));
}

#[test]
fn a_form_lowers_to_the_pins_and_raises_without_passing_six() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = open_document();
    let rows = (0..8)
        .map(|index| usable(&format!("wss://relay{index}.example"), open.clone()))
        .collect();
    save(&path, &Pool { rows }).unwrap();

    let _content = RelayContentGuard::set(dir.path());
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let token = csrf_field(&first.into_string().unwrap());
    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2Frelay0.example&csrf_token={token}"
        ))
        .dispatch();
    let (location, _) = posted_home(added);
    assert_eq!(location, "/nojs");

    let lowered = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!("action=count&count=1&csrf_token={token}"))
        .dispatch();
    let (location, saved) = posted_home(lowered);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("count writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert_eq!(list.pinned, vec!["wss://relay0.example".to_string()]);
    assert!(list.dealt.is_empty());
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(body.contains("<li data-relay=\"wss://relay0.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://relay1.example\">"));
    assert!(body.contains("value=\"1\""));

    let raised = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!("action=count&count=9&csrf_token={token}"))
        .dispatch();
    let (location, saved) = posted_home(raised);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("count writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert_eq!(list.pinned, vec!["wss://relay0.example".to_string()]);
    assert_eq!(
        list.dealt,
        vec![
            "wss://relay1.example".to_string(),
            "wss://relay2.example".to_string(),
            "wss://relay3.example".to_string(),
            "wss://relay4.example".to_string(),
            "wss://relay5.example".to_string(),
        ]
    );
    assert_eq!(list.dealt.len() + list.pinned.len(), 6);
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(body.contains("<li data-relay=\"wss://relay0.example\">"));
    assert!(body.contains("<li data-relay=\"wss://relay5.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://relay6.example\">"));
    assert!(body.contains("value=\"6\""));
}

#[test]
fn a_form_reshuffles_and_every_pin_remains() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let open = open_document();
    let rows = (0..8)
        .map(|index| usable(&format!("wss://relay{index}.example"), open.clone()))
        .collect();
    save(&path, &Pool { rows }).unwrap();

    let _content = RelayContentGuard::set(dir.path());
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let token = csrf_field(&first.into_string().unwrap());
    for relay in ["wss://relay0.example", "wss://hand.example"] {
        let added = client
            .post("/nojs/relays")
            .header(ContentType::Form)
            .body(format!(
                "action=add&relay={}&csrf_token={token}",
                relay.replace("://", "%3A%2F%2F")
            ))
            .dispatch();
        let (location, _) = posted_home(added);
        assert_eq!(location, "/nojs");
    }

    let shuffled = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!("action=reshuffle&count=4&csrf_token={token}"))
        .dispatch();
    let (location, saved) = posted_home(shuffled);
    assert_eq!(location, "/nojs");
    let saved = saved.expect("reshuffle writes the cookie");
    let list = read_relay_cookie(Some(&saved));
    assert_eq!(
        list.pinned,
        vec![
            "wss://relay0.example".to_string(),
            "wss://hand.example".to_string(),
        ]
    );
    assert_eq!(
        list.dealt,
        vec![
            "wss://relay3.example".to_string(),
            "wss://relay4.example".to_string(),
        ]
    );
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(body.contains("<li data-relay=\"wss://relay0.example\">"));
    assert!(body.contains("<li data-relay=\"wss://hand.example\">"));
    assert!(body.contains("<li data-relay=\"wss://relay3.example\">"));
    assert!(body.contains("<li data-relay=\"wss://relay4.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://relay1.example\">"));
    assert!(!body.contains("<li data-relay=\"wss://relay2.example\">"));
    assert!(body.contains("value=\"4\""));
}

#[test]
fn a_seventh_public_url_stays_off_a_full_pin_list() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_relays_page, super::nojs_relays_update],
            ),
    )
    .unwrap();

    let first = client.get("/nojs/relays").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let token = csrf_field(&first.into_string().unwrap());
    let pinned: Vec<String> = (0..6)
        .map(|index| format!("wss://pin{index}.example"))
        .collect();
    for url in &pinned {
        let added = client
            .post("/nojs/relays")
            .header(ContentType::Form)
            .body(format!(
                "action=add&relay={}&csrf_token={token}",
                url.replace("://", "%3A%2F%2F")
            ))
            .dispatch();
        let (location, _) = posted_home(added);
        assert_eq!(location, "/nojs");
    }
    let full = client
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("six pins are stored")
        .value()
        .to_string();
    assert_eq!(read_relay_cookie(Some(&full)).pinned, pinned);

    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2Fextra.example&csrf_token={token}"
        ))
        .dispatch();
    let (location, _) = posted_home(added);
    assert!(location.contains("error=relay_full"));
    assert!(location.contains("typed=wss%3A%2F%2Fextra.example"));
    let body = client.get("/nojs/relays").dispatch().into_string().unwrap();
    assert!(!body.contains("<li data-relay=\"wss://extra.example\">"));
    for url in &pinned {
        assert!(body.contains(&format!("<li data-relay=\"{url}\">")));
    }
    let kept = client
        .cookies()
        .get(RELAY_LIST_COOKIE)
        .expect("full list stays")
        .value()
        .to_string();
    assert_eq!(kept, full);
}

fn csrf_field(html: &str) -> String {
    let marker = "name=\"csrf_token\" value=\"";
    let start = html.find(marker).expect("csrf field") + marker.len();
    let end = html[start..].find('"').expect("csrf value");
    html[start..start + end].to_string()
}

#[test]
fn the_relay_list_round_trips_and_a_missing_store_is_empty() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
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
    run_javascript("nostorgraph-relay-list", &program);
}

#[test]
fn the_script_deal_returns_the_first_two_that_pass() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
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
    run_javascript("nostorgraph-relay-deal", &program);
}

#[test]
fn an_empty_store_deals_three_and_a_second_load_returns_them() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
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
const cleared = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
writeRelayList(cleared, { dealt: [], pinned: [] });
const stored = cleared.getItem("nostorgraph_relays");
const loadedEmpty = loadRelayList(cleared, candidates);
if (loadedEmpty.dealt.length !== 0 || loadedEmpty.pinned.length !== 0) {
    throw new Error(JSON.stringify(loadedEmpty));
}
if (cleared.getItem("nostorgraph_relays") !== stored) {
    throw new Error(cleared.getItem("nostorgraph_relays"));
}
"#,
    );
    run_javascript("nostorgraph-relay-visit", &program);
}

#[test]
fn the_relays_page_actions_call_the_saved_list() {
    let script = include_str!("../templates/relays.js");
    for name in [
        "reshuffleRelays",
        "setRelayCount",
        "addRelay",
        "removeRelay",
        "deleteRelays",
    ] {
        let marker = format!("function {name}(");
        let start = script
            .find(&marker)
            .unwrap_or_else(|| panic!("missing {name}"));
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
fn a_script_posts_the_new_list_and_leaves_out_a_relay_that_was_removed() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
        checks = r#"
const nodes = [];
function hasClass(node, name) {
    return (" " + (node.className || "") + " ").indexOf(" " + name + " ") !== -1;
}
function selectorMatches(node, selector) {
    if (selector.charAt(0) === ".") {
        return hasClass(node, selector.slice(1));
    }
    if (selector === "[data-relay]") {
        return typeof node.dataset.relay === "string";
    }
    if (selector === "input[name=\"csrf_token\"]") {
        return node.tag === "input" && node.name === "csrf_token";
    }
    return false;
}
function el(tag) {
    const node = {
        tag: tag,
        className: "",
        id: "",
        name: "",
        value: "",
        textContent: "",
        dataset: {},
        children: [],
        parentNode: null,
        listeners: {},
        setAttribute() {},
        append() {
            for (let index = 0; index < arguments.length; index += 1) {
                arguments[index].parentNode = node;
                node.children.push(arguments[index]);
            }
        },
        insertBefore(child, before) {
            child.parentNode = node;
            const at = node.children.indexOf(before);
            if (at === -1) {
                node.children.push(child);
            } else {
                node.children.splice(at, 0, child);
            }
        },
        remove() {
            if (!node.parentNode) {
                return;
            }
            node.parentNode.children = node.parentNode.children.filter(function (child) {
                return child !== node;
            });
        },
        replaceChildren() {
            node.children = [];
        },
        closest(selector) {
            let current = node;
            while (current) {
                if (selectorMatches(current, selector)) {
                    return current;
                }
                current = current.parentNode;
            }
            return null;
        },
        addEventListener(type, fn) {
            node.listeners[type] = fn;
        },
    };
    nodes.push(node);
    return node;
}
const body = el("body");
body.className = "relays-page";
const controls = el("div");
controls.className = "relay-deal-controls";
body.append(controls);
const count = el("input");
count.className = "relay-count";
controls.append(count);
const csrf = el("input");
csrf.name = "csrf_token";
csrf.value = "token";
body.append(csrf);
const candidates = el("script");
candidates.id = "relay-candidates";
candidates.textContent = "[]";
var document = {
    createElement(tag) {
        return el(tag);
    },
    getElementById(id) {
        for (let index = 0; index < nodes.length; index += 1) {
            if (nodes[index].id === id) {
                return nodes[index];
            }
        }
        return null;
    },
    querySelector(selector) {
        for (let index = 0; index < nodes.length; index += 1) {
            if (selectorMatches(nodes[index], selector)) {
                return nodes[index];
            }
        }
        return null;
    },
    addEventListener(type, fn) {
        body.addEventListener(type, fn);
    },
};
const memory = {
    values: {},
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
};
writeRelayList(memory, {
    pinned: ["wss://kept.example"],
    dealt: ["wss://stay.example", "wss://left.example"],
});
let posted = null;
globalThis.fetch = async function (url, options) {
    posted = { url: url, body: JSON.parse(options.body) };
    return { ok: true };
};
bindRelayActions(memory);
if (posted !== null) {
    throw new Error("opening the page posted a list");
}
const button = nodes.find(function (node) {
    return (
        node.className === "relay-remove" &&
        node.parentNode &&
        node.parentNode.dataset.relay === "wss://left.example"
    );
});
if (!button) {
    throw new Error("missing remove button");
}
body.listeners.click({ target: button });
const expected = ["wss://kept.example", "wss://stay.example"];
if (!posted || posted.url !== "/tor-circuits" || JSON.stringify(posted.body.relays) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(posted));
}
if (posted.body.relays.some(function (relay) { return relay.indexOf("left.example") !== -1; })) {
    throw new Error(JSON.stringify(posted.body.relays));
}
"#,
    );
    run_javascript("nostorgraph-relay-paths", &program);
}

#[test]
fn a_hand_added_url_survives_a_reshuffle() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
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
    run_javascript("nostorgraph-relay-actions", &program);
}

#[test]
fn a_seventh_pin_keeps_the_typed_url() {
    let program = format!(
        "{source}\n{checks}\n",
        source = include_str!("../templates/relays.js"),
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
const pinned = [];
for (let index = 0; index < 6; index += 1) {
    pinned.push("wss://pin" + index + ".example");
}
writeRelayList(memory, { dealt: [], pinned: pinned });
const before = memory.getItem("nostorgraph_relays");
const full = addRelayResult(memory, "wss://extra.example");
if (full.error !== "Six relays are already pinned." || full.clear) {
    throw new Error("full pin list was accepted");
}
if (full.list.pinned.indexOf("wss://extra.example") !== -1 || memory.getItem("nostorgraph_relays") !== before) {
    throw new Error("seventh pin was stored");
}
const again = addRelayResult(memory, "wss://pin0.example");
if (again.error !== "" || !again.clear || memory.getItem("nostorgraph_relays") !== before) {
    throw new Error("pinned url was treated as new");
}
"#,
    );
    run_javascript("nostorgraph-relay-full", &program);
}

fn run_javascript(prefix: &str, program: &str) {
    let path = std::env::temp_dir().join(format!("{prefix}-{}.js", std::process::id()));
    std::fs::write(&path, format!("{}\n{program}", script_shim())).expect("write relay list fixture");
    let output = script_output(&path);
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

fn script_shim() -> &'static str {
    r#"
if (typeof AbortSignal === "undefined") {
    var AbortSignal = function () {};
    AbortSignal.timeout = function () { return {}; };
}
if (typeof clearTimeout !== "function" && typeof setTimeout === "function") {
    var __nativeSetTimeout = setTimeout;
    var __timerState = [];
    setTimeout = function (fn, ms) {
        var id = __timerState.length + 1;
        var state = { dead: false };
        __timerState[id] = state;
        __nativeSetTimeout(function () {
            if (!state.dead) fn();
        }, ms);
        return id;
    };
    clearTimeout = function (id) {
        if (__timerState[id]) __timerState[id].dead = true;
    };
}
"#
}

fn script_output(path: &std::path::Path) -> std::process::Output {
    if let Ok(output) = std::process::Command::new("node").arg(path).output() {
        if output.status.success() || output.status.code().is_some() {
            return output;
        }
    }
    let jsc = "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc";
    if let Ok(output) = std::process::Command::new(jsc).arg(path).output() {
        return output;
    }
    std::process::Command::new("osascript")
        .args(["-l", "JavaScript"])
        .arg(path)
        .output()
        .expect("node or osascript must run the relay list")
}
