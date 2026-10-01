use super::*;
use std::collections::HashMap;

fn note_identifier(note: &crate::nostr::SignedNote) -> String {
    let parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    parsed["tags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tag| tag[0] == "d")
        .and_then(|tag| tag[1].as_str())
        .unwrap_or("")
        .to_string()
}

fn fetched_from(
    note: &crate::nostr::SignedNote,
    title: &str,
    author: &str,
    content: &str,
    created_at: i64,
) -> crate::nostr::FetchedNote {
    crate::nostr::FetchedNote {
        id_hex: note.id_hex(),
        title: title.to_string(),
        author: author.to_string(),
        content: content.to_string(),
        created_at,
        pubkey: note.pubkey,
        identifier: note_identifier(note),
    }
}

#[test]
fn static_assets_are_cacheable_and_html_is_not() {
    for path in [
        HOME_CSS_PATH,
        HOME_JS_PATH,
        NOSTR_JS_PATH,
        SECP256K1_JS_PATH,
        POST_CSS_PATH,
        POST_JS_PATH,
        POST_NOSCRIPT_CSS_PATH,
        WRITEMARK_JS_PATH,
    ] {
        assert_eq!(
            cache_control_for_path(path),
            "public, max-age=31536000, immutable",
            "{path}"
        );
    }
    // Stable renderer URLs revalidate. They are not pinned for a year and not discarded.
    assert_eq!(cache_control_for_path(PAGE_JS_PATH), "public, no-cache");
    assert_eq!(cache_control_for_path(PAGE_WASM_PATH), "public, no-cache");
    assert_eq!(cache_control_for_path("/a-post"), "no-store, max-age=0");
    assert_eq!(cache_control_for_path("/"), "no-store, max-age=0");
    assert_eq!(cache_control_for_path("/about"), "no-store, max-age=0");
}

#[test]
fn asset_routes_match_path_constants() {
    let source = include_str!("../src/pages.rs");
    for path in [
        PAGE_JS_PATH,
        PAGE_WASM_PATH,
        HOME_CSS_PATH,
        HOME_JS_PATH,
        NOSTR_JS_PATH,
        SECP256K1_JS_PATH,
        POST_CSS_PATH,
        POST_JS_PATH,
        POST_NOSCRIPT_CSS_PATH,
        WRITEMARK_JS_PATH,
    ] {
        assert!(
            source.contains(&format!("#[get(\"{path}\")]")),
            "route attribute drifted from {path}"
        );
    }
}

#[test]
fn robots_txt_is_the_file() {
    assert_eq!(robots_txt().0, include_str!("../robots.txt"));
}

#[test]
fn nojs_home_omits_scripts() {
    let config = crate::config::Config::default();
    let nojs = home_context(&config, true, None);
    assert_eq!(nojs.get("form_action").unwrap(), "/nojs/create");
    assert!(nojs.get("scripts").unwrap().is_empty());
    let field = nojs.get("content_field").unwrap();
    assert!(field.contains("<textarea name=\"content\""));
    assert!(field.contains(&format!(
        "maxlength=\"{}\"",
        config.limits.content_max_length
    )));
    assert!(!field.contains("writemark-editor"));

    let js = home_context(&config, false, None);
    assert_eq!(js.get("form_action").unwrap(), "/create");
    let scripts = js.get("scripts").unwrap();
    assert!(scripts.contains("/writemark.js?v="));
    assert!(scripts.contains("/home.js?v="));
    assert!(!scripts.contains("nonograph_page"));
    assert!(!scripts.contains(".wasm"));
    assert!(js
        .get("content_field")
        .unwrap()
        .contains("<writemark-editor"));
    assert!(!js
        .get("content_field")
        .unwrap()
        .contains("<textarea name=\"content\""));
}

#[test]
fn home_error_is_a_known_sentence() {
    assert_eq!(home_error_message(None), "");
    assert_eq!(home_error_message(Some("")), "");
    assert_eq!(
        home_error_message(Some("content_required")),
        "Write something before publishing."
    );
    assert_eq!(
        home_error_message(Some("<script>")),
        "Something went wrong. Try again."
    );
}

#[test]
fn rendered_home_switches_the_content_field() {
    let config = crate::config::Config::default();
    let engine = crate::template::TemplateEngine::new("templates");
    let nojs = engine
        .render(
            "home",
            &home_context(&config, true, Some("content_required")),
        )
        .unwrap();
    assert!(nojs.contains("<textarea name=\"content\""));
    assert!(!nojs.contains("writemark-editor"));
    assert!(!nojs.contains("<script"));
    assert!(nojs.contains("action=\"/nojs/create\""));
    assert!(nojs.contains("Write something before publishing."));
    assert!(nojs.contains("class=\"nojs\""));
    assert!(nojs.contains("128,000 characters"));
    assert!(!nojs.contains("256,000"));

    let js = engine
        .render("home", &home_context(&config, false, None))
        .unwrap();
    assert!(js.contains("<writemark-editor"));
    assert!(!js.contains("<textarea name=\"content\""));
    assert!(js.contains("/writemark.js?v="));
    assert!(js.contains("data-relays="));
    assert!(js.contains("wss://"));
    assert!(js.contains("data-timeout=\"10000\""));
    assert!(js.contains("<p class=\"form-error\"></p>"));
    assert!(js.contains("0 / 128,000"));
    assert!(js.contains("class=\"\""));
    assert!(!js.contains("class=\"nojs\""));
    assert!(js.contains("action=\"/create\""));
    assert_eq!(js.matches("On Nostr").count(), 3); // buttons + About copy
    assert_eq!(js.matches("class=\"nostr-publish\"").count(), 2);
    assert!(js.contains("type=\"submit\""));
    assert!(nojs.contains("On Nostr"));
    assert!(nojs.contains("class=\"nojs\""));
    assert!(nojs.contains("This page only saves a file on this host."));
    assert!(nojs.contains("Publishing on Nostr needs JavaScript."));
    assert!(js.contains("sidebar-relays"));
    assert!(js.contains("relay.primal.net"));
}

#[test]
fn nojs_post_links_the_fallback_stylesheet_outside_noscript() {
    let engine = crate::template::TemplateEngine::new("templates");
    let nojs = engine.render("post", &sample_post_context(true)).unwrap();
    let before_noscript = nojs.split("<noscript>").next().unwrap();
    assert!(before_noscript.contains("/post-noscript.css?v="));
    assert!(!nojs.contains("<script"));

    let js = engine.render("post", &sample_post_context(false)).unwrap();
    let js_before_noscript = js.split("<noscript>").next().unwrap();
    assert!(!js_before_noscript.contains("post-noscript.css"));
    assert!(js.contains("<script src=\"/post.js?v="));
    assert!(js.contains("<noscript>"));
}

fn sample_post_context(nojs: bool) -> HashMap<String, String> {
    let mut context = HashMap::new();
    for key in [
        "title",
        "author",
        "author_display",
        "created_at",
        "created_at_iso",
        "url",
        "description",
    ] {
        context.insert(key.to_string(), "x".to_string());
    }
    context.insert("content".to_string(), "<p>Hi</p>".to_string());
    fill_page_chrome(&mut context, nojs, "abc");
    context
}

#[test]
fn post_scripts_do_not_load_the_wasm_renderer() {
    let scripts = post_scripts("abc");
    assert!(scripts.contains("/post.js?v=abc"));
    assert!(!scripts.contains("wasm"));
    assert!(!scripts.contains("nonograph_page"));
}

#[test]
fn description_stops_after_160_characters() {
    let long = format!("🎯{}", "x".repeat(10_000));
    let description = post_description(&long);
    assert_eq!(description.chars().count(), 163);
    assert!(description.ends_with("..."));
    assert_eq!(
        description.chars().take(160).collect::<String>(),
        long.chars().take(160).collect::<String>()
    );

    let exact = "y".repeat(160);
    assert_eq!(post_description(&exact), exact);
    assert_eq!(post_description("short"), "short");
}

#[test]
fn long_description_is_escaped_once() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("post.html"),
        r#"<meta content="{{description}}" />"#,
    )
    .unwrap();
    let engine = crate::template::TemplateEngine::new(dir.path().to_str().unwrap());
    let raw = format!("Tom & Jerry <note>{}", "x".repeat(160));
    let mut context = HashMap::new();
    context.insert("description".to_string(), post_description(&raw));
    let html = engine.render("post", &context).unwrap();
    assert!(html.contains("Tom &amp; Jerry &lt;note&gt;"));
    assert!(!html.contains("&amp;amp;"));
    assert!(!html.contains("&amp;lt;"));
}

#[test]
fn renderer_etag_revalidates_without_pinning() {
    let wasm = entity_tag_for_path(PAGE_WASM_PATH).unwrap();
    let js = entity_tag_for_path(PAGE_JS_PATH).unwrap();
    assert!(wasm.starts_with('"') && wasm.ends_with('"'));
    assert_ne!(wasm, js);
    assert!(if_none_match_matches(wasm, wasm));
    assert!(if_none_match_matches(&format!("W/{wasm}"), wasm));
    assert!(if_none_match_matches("*", wasm));
    assert!(if_none_match_matches(&format!("\"other\", {wasm}"), wasm));
    assert!(!if_none_match_matches("\"other\"", wasm));
    assert_eq!(entity_tag_for_path("/about"), None);
}

#[test]
fn selection_hash_keeps_the_query_string() {
    let source = include_str!("../templates/post.js");
    assert!(source.contains("document.location.search"));
}

#[test]
fn nojs_static_page_omits_scripts() {
    let config = crate::config::Config::default();
    let nojs = match build_static_page("about", &config, true) {
        BuiltPage::Ready(html) => html,
        other => panic!("about page did not render: {other:?}"),
    };
    assert!(!nojs.contains("<script"));
    assert!(nojs.contains("href=\"/about\""));
    assert!(nojs.contains(">js<"));

    let js = match build_static_page("about", &config, false) {
        BuiltPage::Ready(html) => html,
        other => panic!("about page did not render: {other:?}"),
    };
    assert!(js.contains("<script src=\"/post.js?v="));
    assert!(js.contains("href=\"/nojs/about\""));
    assert!(js.contains(">nojs<"));
}

#[test]
fn mode_link_points_at_the_other_view() {
    assert_eq!(mode_href(false, "abc"), "/nojs/abc");
    assert_eq!(mode_label(false), "nojs");
    assert_eq!(mode_href(true, "abc"), "/abc");
    assert_eq!(mode_label(true), "js");
}

#[test]
fn leftover_wrap_link_names_the_file_by_event_id() {
    let wrapped = crate::nostr::wrap_note("Title", "Ada", "body", 1_700_000_000).unwrap();
    let nevent = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_GIFT_WRAP,
    );
    assert_eq!(leftover_wrap_file_id(&nevent), Some(wrapped.id_hex()));
    assert!(leftover_wrap_file_id("hello-world").is_none());
    assert!(!crate::save::post_file_exists(&wrapped.id_hex()));
}

#[test]
fn leftover_wrap_with_a_file_is_the_same_post() {
    let wrapped =
        crate::nostr::wrap_note("Wrapped", "Wrap", "ciphertext body", 1_700_000_000).unwrap();
    let nevent = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_GIFT_WRAP,
    );
    let file_id = leftover_wrap_file_id(&nevent).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let post = std::sync::Arc::new(crate::Post {
        id: file_id.clone(),
        title: "Kept".to_string(),
        author: "Ada".to_string(),
        content: "<p>still here</p>".to_string(),
        raw_content: "still here".to_string(),
        created_at: chrono::Utc::now(),
        nostr_id: None,
    });
    crate::save::save_post_to_file_in_dir(&post, base).unwrap();

    let opened = leftover_wrap_post_in_dir(
        &nevent,
        &crate::cache::PostCache::shared(1),
        &crate::config::Config::default(),
        base,
    )
    .expect("leftover wrap with a file should open");
    assert_eq!(opened.id, file_id);
    assert_eq!(opened.title, "Kept");
    assert_eq!(opened.author, "Ada");
    assert_eq!(opened.raw_content, "still here");
    assert_ne!(opened.title, "Wrapped");
    assert_ne!(opened.raw_content, "ciphertext body");
}

#[test]
fn leftover_wrap_without_a_file_cannot_open() {
    let wrapped = crate::nostr::wrap_note("Gone", "", "body", 1_700_000_000).unwrap();
    let nevent = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_GIFT_WRAP,
    );
    let dir = tempfile::tempdir().unwrap();
    let opened = leftover_wrap_post_in_dir(
        &nevent,
        &crate::cache::PostCache::shared(1),
        &crate::config::Config::default(),
        dir.path().to_str().unwrap(),
    );
    assert!(opened.is_none());
}

#[test]
fn wrap_link_without_a_file_is_refused() {
    let wrapped = crate::nostr::wrap_note("Gone", "", "body", 1_700_000_000).unwrap();
    let nevent = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_GIFT_WRAP,
    );
    let decoded = crate::nostr::decode_nevent(&nevent).unwrap();
    assert!(!may_fetch_public_kind(decoded.kind));
    assert_eq!(
        refuse_unopened_wrap(decoded.kind, false),
        Some(WRAP_REFUSED_HTML)
    );
    assert!(refuse_unopened_wrap(decoded.kind, true).is_none());

    let dir = tempfile::tempdir().unwrap();
    let opened = leftover_wrap_post_in_dir(
        &nevent,
        &crate::cache::PostCache::shared(1),
        &crate::config::Config::default(),
        dir.path().to_str().unwrap(),
    );
    assert!(opened.is_none());
    assert_eq!(
        refuse_unopened_wrap(decoded.kind, opened.is_some()),
        Some(WRAP_REFUSED_HTML)
    );

    let naddr =
        crate::nostr::encode_naddr("secret", &[], &wrapped.pubkey, crate::nostr::KIND_GIFT_WRAP);
    assert_eq!(
        refuse_unopened_wrap(
            Some(crate::nostr::decode_naddr(&naddr).unwrap().kind),
            false
        ),
        Some(WRAP_REFUSED_HTML)
    );

    let public = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_LONG_FORM,
    );
    let public_kind = crate::nostr::decode_nevent(&public).unwrap().kind;
    assert!(refuse_unopened_wrap(public_kind, false).is_none());
    assert!(refuse_unopened_wrap(None, false).is_none());

    assert!(WRAP_REFUSED_HTML.contains("extension"));
    assert!(!WRAP_REFUSED_HTML.contains("nsec"));
    let pages = include_str!("../src/pages.rs");
    assert!(pages.contains("refuse_unopened_wrap"));
    assert!(!pages.contains("decode_nsec"));
    assert!(!pages.contains("open_wrapped_note"));
}

#[test]
fn public_fetch_asks_nevent_relays_then_the_instance() {
    let nevent = crate::nostr::Nevent {
        event_id_hex: "ab".repeat(32),
        relays: vec![
            "wss://hint.example".to_string(),
            "wss://relay.damus.io".to_string(),
        ],
        kind: Some(crate::nostr::KIND_LONG_FORM),
    };
    let fallback = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ];
    assert_eq!(
        relays_for_public_fetch(&nevent.relays, &fallback),
        vec![
            "wss://hint.example".to_string(),
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
        ]
    );
    let empty = crate::nostr::Nevent {
        event_id_hex: "cd".repeat(32),
        relays: Vec::new(),
        kind: None,
    };
    assert_eq!(relays_for_public_fetch(&empty.relays, &fallback), fallback);
}

#[test]
fn public_fetch_drops_private_hints_and_caps_relays() {
    let mut hints = vec![
        "wss://127.0.0.1".to_string(),
        "wss://localhost".to_string(),
        "wss://10.0.0.1".to_string(),
    ];
    for index in 0..10 {
        hints.push(format!("wss://hint{index}.example"));
    }
    let nevent = crate::nostr::Nevent {
        event_id_hex: "ab".repeat(32),
        relays: hints,
        kind: Some(crate::nostr::KIND_LONG_FORM),
    };
    let fallback = vec!["wss://nos.lol".to_string()];
    let relays = relays_for_public_fetch(&nevent.relays, &fallback);
    assert_eq!(relays.len(), crate::nostr::MAX_FETCH_RELAYS);
    assert_eq!(relays[0], "wss://hint0.example");
    assert_eq!(relays.last().unwrap(), "wss://nos.lol");
    assert!(!relays.iter().any(|relay| relay.contains("127.0.0.1")));
    assert!(!relays.iter().any(|relay| relay.contains("localhost")));
}

#[test]
fn public_fetch_uses_every_hint_slot_when_the_instance_has_no_public_relay() {
    let mut hints = Vec::new();
    for index in 0..8 {
        hints.push(format!("wss://hint{index}.example"));
    }
    let nevent = crate::nostr::Nevent {
        event_id_hex: "ab".repeat(32),
        relays: hints,
        kind: Some(crate::nostr::KIND_LONG_FORM),
    };
    let fallback = vec!["wss://127.0.0.1".to_string()];
    let relays = relays_for_public_fetch(&nevent.relays, &fallback);
    assert_eq!(relays.len(), crate::nostr::MAX_FETCH_RELAYS);
    assert_eq!(relays[0], "wss://hint0.example");
    assert_eq!(relays[5], "wss://hint5.example");
    assert!(!relays.contains(&"wss://127.0.0.1".to_string()));
}

#[test]
fn public_fetch_slots_run_out() {
    let counter = std::sync::atomic::AtomicUsize::new(0);
    assert!(try_acquire_public_fetch(&counter, 2));
    assert!(try_acquire_public_fetch(&counter, 2));
    assert!(!try_acquire_public_fetch(&counter, 2));
    release_public_fetch(&counter);
    assert!(try_acquire_public_fetch(&counter, 2));
}

#[test]
fn public_note_becomes_the_same_kind_of_page_as_a_file() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let id = note.id_hex();
    let nevent =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    let fetched = fetched_from(
        &note,
        "<b>Hello</b>",
        "<em>Ada</em>",
        "**hi**",
        1_700_000_000,
    );
    let post = post_from_public_note(fetched, &storage, &config, &id, &nevent, base).unwrap();
    assert_ne!(post.id, id);
    assert!(post.id.contains("hello"));
    assert_eq!(post.nostr_id.as_deref(), Some(nevent.as_str()));
    assert_eq!(post.title, "Hello");
    assert_eq!(post.author, "Ada");
    assert_eq!(post.raw_content, "**hi**");
    assert!(post.content.contains("<strong>hi</strong>"));
    assert!(storage.read().unwrap().contains_key(&id));
    assert!(storage.read().unwrap().contains_key(&post.id));
    assert!(crate::save::post_file_exists_in_dir(&post.id, base));
    assert!(crate::save::post_file_exists_in_dir(&id, base));
    assert!(!post.id.contains("nsec"));
    assert!(!post.id.contains("nevent"));
}

#[test]
fn public_note_that_is_too_long_is_not_a_page() {
    let storage = crate::cache::PostCache::shared(1);
    let mut config = crate::config::Config::default();
    config.limits.content_max_length = 4;
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let id = "ab".repeat(32);
    let fetched = crate::nostr::FetchedNote {
        id_hex: id.clone(),
        title: "Hello".to_string(),
        author: String::new(),
        content: "hello".to_string(),
        created_at: 1_700_000_000,
        ..crate::nostr::FetchedNote::default()
    };
    assert!(post_from_public_note(fetched, &storage, &config, &id, "nevent1qq", base).is_none());
}

#[test]
fn public_note_with_a_long_title_or_author_is_not_a_page() {
    let storage = crate::cache::PostCache::shared(1);
    let mut config = crate::config::Config::default();
    config.limits.title_max_length = 4;
    config.limits.alias_max_length = 3;
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let id = "ab".repeat(32);
    let long_title = crate::nostr::FetchedNote {
        id_hex: id.clone(),
        title: "Hello".to_string(),
        author: "Ada".to_string(),
        content: "hi".to_string(),
        created_at: 1_700_000_000,
        ..crate::nostr::FetchedNote::default()
    };
    assert!(post_from_public_note(long_title, &storage, &config, &id, "nevent1qq", base).is_none());
    config.limits.title_max_length = 128;
    let long_author = crate::nostr::FetchedNote {
        id_hex: id.clone(),
        title: "Hello".to_string(),
        author: "Ada Lovelace".to_string(),
        content: "hi".to_string(),
        created_at: 1_700_000_000,
        ..crate::nostr::FetchedNote::default()
    };
    assert!(
        post_from_public_note(long_author, &storage, &config, &id, "nevent1qq", base).is_none()
    );
}

#[test]
fn view_path_fetches_public_notes_and_does_not_decrypt() {
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let public =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    let public_nevent = crate::nostr::decode_nevent(&public).unwrap();
    assert!(may_fetch_public_kind(public_nevent.kind));

    let wrapped = crate::nostr::wrap_note("Gone", "", "body", 1_700_000_000).unwrap();
    let wrap = crate::nostr::encode_nevent(
        &wrapped.id,
        &[],
        &wrapped.pubkey,
        crate::nostr::KIND_GIFT_WRAP,
    );
    let wrap_nevent = crate::nostr::decode_nevent(&wrap).unwrap();
    assert!(!may_fetch_public_kind(wrap_nevent.kind));
    assert_eq!(leftover_wrap_file_id(&wrap), Some(wrapped.id_hex()));

    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let file_id = leftover_wrap_file_id(&wrap).unwrap();
    let post = std::sync::Arc::new(crate::Post {
        id: file_id.clone(),
        title: "Kept".to_string(),
        author: "Ada".to_string(),
        content: "<p>still here</p>".to_string(),
        raw_content: "still here".to_string(),
        created_at: chrono::Utc::now(),
        nostr_id: None,
    });
    crate::save::save_post_to_file_in_dir(&post, base).unwrap();
    let opened = leftover_wrap_post_in_dir(
        &wrap,
        &crate::cache::PostCache::shared(1),
        &crate::config::Config::default(),
        base,
    )
    .expect("leftover wrap with a file should open");
    assert_eq!(opened.raw_content, "still here");
    assert!(!may_fetch_public_kind(wrap_nevent.kind));

    let pages = include_str!("../src/pages.rs");
    assert!(!pages.contains("decode_nsec"));
    assert!(!pages.contains("?<nsec>"));
    assert!(!pages.contains("fetch_missing_note"));
}

#[test]
fn view_path_fetches_naddr_like_nevent() {
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let parsed: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let d = parsed["tags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tag| tag[0] == "d")
        .unwrap()[1]
        .as_str()
        .unwrap();
    let encoded = crate::nostr::encode_naddr(d, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    let naddr = crate::nostr::decode_naddr(&encoded).unwrap();
    assert!(may_fetch_public_kind(Some(naddr.kind)));
    let cache_id = crate::nostr::naddr_cache_id(&naddr);
    assert!(is_valid_post_id(&cache_id));
    assert_ne!(cache_id, note.id_hex());

    let wrap =
        crate::nostr::encode_naddr("secret", &[], &note.pubkey, crate::nostr::KIND_GIFT_WRAP);
    assert!(!may_fetch_public_kind(Some(
        crate::nostr::decode_naddr(&wrap).unwrap().kind
    )));

    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let fetched = fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000);
    let post =
        post_from_public_note(fetched, &storage, &config, &cache_id, &encoded, base).unwrap();
    assert_ne!(post.id, note.id_hex());
    assert_ne!(post.id, cache_id);
    assert_eq!(post.nostr_id.as_deref(), Some(encoded.as_str()));
    assert!(storage.read().unwrap().contains_key(&cache_id));
    assert!(storage.read().unwrap().contains_key(&post.id));
    assert!(storage.read().unwrap().contains_key(&note.id_hex()));
    assert!(crate::save::post_file_exists_in_dir(&note.id_hex(), base));
    assert!(crate::save::post_file_exists_in_dir(&cache_id, base));

    let pages = include_str!("../src/pages.rs");
    assert!(pages.contains("fetch_public_addr"));
    assert!(pages.contains("decode_naddr"));
}

#[test]
fn create_redirects_to_a_local_id_with_no_nsec() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().to_str().unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays.clear();
    let storage = crate::cache::PostCache::shared(1);
    let form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };

    let href = create_location_in_dir(false, &form, &storage, &config, base);
    let names: Vec<_> = std::fs::read_dir(temp.path().join("content"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(names.len(), 1, "href={href} files={names:?}");
    let id = &names[0];
    assert_eq!(href, format!("/{id}"));
    assert_eq!(publish::published_href(true, id), format!("/nojs/{id}"));
    assert!(!href.contains("nsec"));
    assert!(!href.contains("nevent"));
    assert!(!href.contains('?'));
    assert!(href.contains("hello-world"));
}

#[test]
fn public_note_keeps_the_same_short_id() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let id = note.id_hex();
    let nevent =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    let fetched = || fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000);
    let first = post_from_public_note(fetched(), &storage, &config, &id, &nevent, base).unwrap();
    let fresh = crate::cache::PostCache::shared(1);
    let second = post_from_public_note(fetched(), &fresh, &config, &id, &nevent, base).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.nostr_id, first.nostr_id);

    let by_short = load_post_from_disk(&first.id, &fresh, &config, base).unwrap();
    assert_eq!(by_short.id, first.id);
    assert_eq!(by_short.nostr_id.as_deref(), Some(nevent.as_str()));
}

#[test]
fn public_note_footer_prints_the_nostr_id() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let id = note.id_hex();
    let nevent =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    let fetched = fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000);
    let post = post_from_public_note(fetched, &storage, &config, &id, &nevent, base).unwrap();
    let html = article_html(&post, true, &post.id).unwrap();
    assert!(html.contains(&format!("href=\"/{nevent}\"")));
    assert!(html.contains(&nevent));
    assert!(html.contains("class=\"nostr-id\""));
    assert!(html.contains(&format!("content=\"/{nevent}\"")));

    let local = crate::Post {
        id: "hello-local".to_string(),
        title: "Hello".to_string(),
        author: "Ada".to_string(),
        content: "<p>hi</p>".to_string(),
        raw_content: "hi".to_string(),
        created_at: chrono::Utc::now(),
        nostr_id: None,
    };
    let local_html = article_html(&local, true, &local.id).unwrap();
    assert!(!local_html.contains("nevent1"));
    assert!(!local_html.contains("class=\"nostr-id\""));
    assert!(local_html.contains("content=\"/hello-local\""));
}

#[test]
fn nostr_id_is_copied_from_the_footer() {
    let source = include_str!("../templates/post.js");
    assert!(source.contains("a.nostr-id"));
    assert!(source.contains("clipboard.writeText(label)"));
    assert!(source.contains("link.textContent"));
    assert!(source.contains(".catch("));
    assert!(!source.contains("location.assign"));
    assert!(source.contains("metaKey"));
    assert!(source.contains("ctrlKey"));
    assert!(source.contains("copied"));
    let html = include_str!("../templates/post.html");
    assert!(html.contains("{{nostr_link}}"));
}

fn public_note_fixture() -> (tempfile::TempDir, String, String, crate::nostr::FetchedNote) {
    let dir = tempfile::tempdir().unwrap();
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let id = note.id_hex();
    let fetched = fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000);
    let nevent =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);
    (dir, id, nevent, fetched)
}

#[test]
fn public_note_raw_markdown_follows_the_alias() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let (dir, id, nevent, fetched) = public_note_fixture();
    let base = dir.path().to_str().unwrap();
    let post = post_from_public_note(fetched, &storage, &config, &id, &nevent, base).unwrap();
    let by_short = crate::save::read_post_file_in_dir(&post.id, base).unwrap();
    let by_event = crate::save::read_post_file_in_dir(&id, base).unwrap();
    assert_eq!(by_short, by_event);
    assert!(by_event.contains("**hi**"));
    assert!(by_event.contains(&format!("nostr: {nevent}")));
    assert!(!by_event.contains("alias:"));
}

#[test]
fn public_note_replaces_a_dangling_alias() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let (dir, id, nevent, fetched) = public_note_fixture();
    let base = dir.path().to_str().unwrap();
    let first = post_from_public_note(fetched, &storage, &config, &id, &nevent, base).unwrap();
    assert!(crate::save::remove_post_file_in_dir(&first.id, base));
    assert!(!crate::save::post_file_is_live_in_dir(&id, base));

    let fetched = crate::nostr::FetchedNote {
        id_hex: id.clone(),
        title: "Hello".to_string(),
        author: "Ada".to_string(),
        content: "**hi**".to_string(),
        created_at: 1_700_000_000,
        ..crate::nostr::FetchedNote::default()
    };
    let fresh = crate::cache::PostCache::shared(1);
    let second = post_from_public_note(fetched, &fresh, &config, &id, &nevent, base).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.nostr_id.as_deref(), Some(nevent.as_str()));
    assert!(crate::save::post_file_is_live_in_dir(&id, base));
    assert!(crate::save::post_file_exists_in_dir(&second.id, base));
    let by_event = crate::save::read_post_file_in_dir(&id, base).unwrap();
    assert!(by_event.contains("**hi**"));
}

#[test]
fn public_note_upgrades_a_hex_file_to_a_short_link() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let (dir, id, nevent, fetched) = public_note_fixture();
    let base = dir.path().to_str().unwrap();
    let leftover = crate::Post {
        id: id.clone(),
        title: "Hello".to_string(),
        author: "Ada".to_string(),
        content: "<p>old</p>".to_string(),
        raw_content: "old body".to_string(),
        created_at: chrono::Utc::now(),
        nostr_id: None,
    };
    crate::save::save_post_to_file_in_dir(&leftover, base).unwrap();

    let post = post_from_public_note(fetched, &storage, &config, &id, &nevent, base).unwrap();
    assert_ne!(post.id, id);
    assert_eq!(post.nostr_id.as_deref(), Some(nevent.as_str()));
    assert_eq!(post.raw_content, "old body");
    let pointer =
        std::fs::read_to_string(dir.path().join("content").join(format!("{id}.md"))).unwrap();
    assert!(pointer.contains(&format!("alias: {}", post.id)));
    assert!(!pointer.contains("old body"));
    let short = crate::save::read_post_file_in_dir(&post.id, base).unwrap();
    assert!(short.contains("old body"));
    assert!(short.contains(&format!("nostr: {nevent}")));
    let html = article_html(&post, true, &post.id).unwrap();
    assert!(html.contains(&format!("href=\"/{nevent}\"")));
}

#[test]
fn public_note_shares_a_short_id_for_nevent_and_naddr() {
    let storage = crate::cache::PostCache::shared(1);
    let config = crate::config::Config::default();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let note = crate::nostr::sign_note("Hello", "Ada", "**hi**", 1_700_000_000);
    let hex = note.id_hex();
    let naddr = crate::nostr::encode_naddr(
        &note_identifier(&note),
        &[],
        &note.pubkey,
        crate::nostr::KIND_LONG_FORM,
    );
    let naddr_id = crate::nostr::naddr_cache_id(&crate::nostr::decode_naddr(&naddr).unwrap());
    let nevent =
        crate::nostr::encode_nevent(&note.id, &[], &note.pubkey, crate::nostr::KIND_LONG_FORM);

    let via_naddr = post_from_public_note(
        fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000),
        &storage,
        &config,
        &naddr_id,
        &naddr,
        base,
    )
    .unwrap();
    let via_nevent = post_from_public_note(
        fetched_from(&note, "Hello", "Ada", "**hi**", 1_700_000_000),
        &crate::cache::PostCache::shared(1),
        &config,
        &hex,
        &nevent,
        base,
    )
    .unwrap();
    assert_eq!(via_nevent.id, via_naddr.id);
    assert_eq!(
        crate::save::alias_target_in_dir(&hex, base).as_deref(),
        Some(via_naddr.id.as_str())
    );
    assert_eq!(
        crate::save::alias_target_in_dir(&naddr_id, base).as_deref(),
        Some(via_naddr.id.as_str())
    );
    let shorts: Vec<_> = std::fs::read_dir(dir.path().join("content"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".md")?;
            if stem == hex || stem == naddr_id {
                None
            } else {
                Some(stem.to_string())
            }
        })
        .collect();
    assert_eq!(shorts.len(), 1);
    assert_eq!(shorts[0], via_naddr.id);
}

#[test]
fn homepage_js_can_send_a_public_long_form_note() {
    let nostr = include_str!("../templates/nostr.js");
    assert!(nostr.contains("KIND_LONG_FORM = 30023"));
    assert!(nostr.contains("publishPublicNote"));
    assert!(nostr.contains("[\"EVENT\""));
    assert!(nostr.contains("new WebSocket"));
    assert!(nostr.contains("publicRelayUrl"));
    assert!(nostr.contains("relaysForPublicPublish"));
    assert!(nostr.contains("10_000"));
    assert!(nostr.contains("sha256Sync"));
    assert!(nostr.contains("ensureBip340"));
    assert!(nostr.contains("encodeNevent"));
    assert!(nostr.contains("encodeBech32"));
    assert!(nostr.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    assert!(nostr.contains(
        "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
    ));
    assert!(!nostr.contains("1059"));
    assert!(!nostr.contains("nsec"));
    assert!(!nostr.contains("gift"));
    assert!(!nostr.contains("wrap"));

    let home = include_str!("../templates/home.js");
    let helper = home.find("nonographPublishPublicNote").unwrap();
    let imported = home.find("await import").unwrap();
    assert!(helper < imported);
    assert!(home.contains("./nostr.js"));
    assert!(home.contains("publishPublicNote"));
    assert!(home.contains("location.assign(\"/\" + result.nevent)"));
    assert!(home.contains(".nostr-publish"));
    assert!(home.contains("Publishing failed. Try again."));
    assert!(home.contains("A title is required."));
    assert!(home.contains("if (publishing)"));
    assert!(home.contains("leaveBusy = true"));
    assert!(home.contains("if (!leaveBusy)"));
    assert!(home.contains("syncNostrButtons"));
    assert!(home.contains("nonograph_extra_relays"));
    assert!(!home.contains("1059"));
    assert!(!home.contains("nsec"));

    let html = include_str!("../templates/home.html");
    assert!(html.contains("data-relays=\"{{nostr_relays}}\""));
    assert!(html.contains("data-timeout=\"{{nostr_timeout_ms}}\""));
    assert!(html.contains("action=\"{{form_action}}\""));
    assert!(html.contains("type=\"submit\""));
    assert!(html.contains("class=\"nostr-publish\""));
    assert!(html.contains("On Nostr"));
    assert_eq!(html.matches("class=\"nostr-publish\"").count(), 2);
    assert_eq!(html.matches("type=\"button\" class=\"nostr-publish\"").count(), 2);
    assert!(html.contains("class=\"nojs-file-note\""));
    assert!(html.contains("This page only saves a file on this host."));
    assert!(html.contains("Publishing on Nostr needs JavaScript."));
    assert!(html.contains("<noscript>"));
    let note = html.find("class=\"nojs-file-note\"").unwrap();
    let sidebar = html.find("class=\"sidebar\"").unwrap();
    assert!(note < sidebar);

    let css = include_str!("../templates/home.css");
    assert!(css.contains("body.nojs .nostr-publish"));
    assert!(css.contains("body.nojs .nojs-file-note"));
}

#[test]
fn homepage_js_gets_the_instance_public_relays() {
    let config = crate::config::Config::default();
    let js = home_context(&config, false, None);
    let relays: Vec<String> = serde_json::from_str(js.get("nostr_relays").unwrap()).unwrap();
    assert_eq!(
        relays,
        vec![
            "wss://relay.primal.net".to_string(),
            "wss://relay.snort.social".to_string(),
            "wss://offchain.pub".to_string(),
        ]
    );
    assert!(relays.iter().all(|relay| relay.starts_with("wss://")));
    assert!(!relays.iter().any(|relay| relay.contains("127.0.0.1")));
    assert!(!relays.iter().any(|relay| relay.contains("localhost")));
    assert_eq!(js.get("nostr_timeout_ms").unwrap(), "10000");
    assert!(js.get("scripts").unwrap().contains("/home.js?v="));
    assert!(!js.get("scripts").unwrap().contains("/nostr.js"));

    let mut slow = config.clone();
    slow.nostr.timeout_secs = 20;
    let longer = home_context(&slow, false, None);
    assert_eq!(longer.get("nostr_timeout_ms").unwrap(), "20000");

    let mut private = config.clone();
    private.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let hidden = home_context(&private, false, None);
    assert_eq!(hidden.get("nostr_relays").unwrap(), "[]");

    let nojs = home_context(&config, true, None);
    assert!(nojs.get("scripts").unwrap().is_empty());
    assert!(nojs.get("nostr_relays").is_some());
}

#[test]
fn tab_relay_timeout_is_at_least_ten_seconds() {
    assert_eq!(tab_relay_timeout_ms(0), 10_000);
    assert_eq!(tab_relay_timeout_ms(3), 10_000);
    assert_eq!(tab_relay_timeout_ms(20), 20_000);
}
