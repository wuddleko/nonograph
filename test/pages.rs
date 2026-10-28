use super::*;
use serial_test::serial;
use std::collections::HashMap;

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
        HOME_NOSCRIPT_CSS_PATH,
        WRITEMARK_JS_PATH,
        WRITEMARK_LIVE_JS_PATH,
        WRITEMARK_CSS_PATH,
        NOTICE_CSS_PATH,
        THEME_JS_PATH,
        EDITOR_MODE_JS_PATH,
        RELAYS_JS_PATH,
        LOGO_PNG_PATH,
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
    assert_eq!(
        cache_control_for_request(PAGE_WASM_PATH, Some("v=abc")),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        cache_control_for_request(PAGE_JS_PATH, Some("v=abc")),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        cache_control_for_request(PDF_WASM_PATH, Some("v=abc")),
        "public, no-cache"
    );
    assert_eq!(
        cache_control_for_request(PDF_JS_PATH, Some("v=abc")),
        "public, no-cache"
    );
    assert_eq!(cache_control_for_path(PDF_JS_PATH), "public, no-cache");
    assert_eq!(cache_control_for_path(PDF_WASM_PATH), "public, no-cache");
    assert_eq!(cache_control_for_path(PDF_LICENSE_PATH), "public, no-cache");
    assert_eq!(cache_control_for_path("/a-post"), "no-store, max-age=0");
    assert_eq!(cache_control_for_path("/"), "no-store, max-age=0");
    assert_eq!(cache_control_for_path("/about"), "no-store, max-age=0");
    assert_eq!(cache_control_for_path("/nojs"), "no-store, max-age=0");
    assert_eq!(
        cache_control_for_path("/nojs/some-post"),
        "no-store, max-age=0"
    );
    assert_eq!(
        cache_control_for_path("/nojs/s/0123456789abcdef0123"),
        "no-store"
    );
}

#[test]
fn robots_txt_is_the_file() {
    assert_eq!(robots_txt().0, include_str!("../robots.txt"));
}

#[test]
fn nojs_home_omits_scripts() {
    let mut config = crate::config::Config::default();
    config.theme.syntax_highlighting = "ocean\"dark".to_string();
    config.security.external_link_security = false;
    let nojs = home_context(&config, true, None, None);
    assert_eq!(nojs.get("form_action").unwrap(), "/nojs/create");
    assert!(nojs.get("scripts").unwrap().is_empty());
    let field = nojs.get("content_field").unwrap();
    assert!(field.contains("<textarea name=\"content\""));
    assert!(field.contains(&format!(
        "maxlength=\"{}\"",
        config.limits.content_max_length
    )));
    assert!(!field.contains("writemark-editor"));

    let js = home_context(&config, false, None, None);
    assert_eq!(js.get("form_action").unwrap(), "/nojs");
    assert_eq!(js.get("form_method").unwrap(), "get");
    assert_eq!(nojs.get("form_method").unwrap(), "post");
    let scripts = js.get("scripts").unwrap();
    assert!(scripts.contains("/relays.js?v="));
    assert!(scripts.contains("/editor-mode.js?v="));
    assert!(scripts.contains("/writemark.js?v="));
    assert!(scripts.contains("/home.js?v="));
    assert!(scripts.contains("<link rel=\"modulepreload\" href=\"/writemark.js?v="));
    assert!(scripts.contains("<link rel=\"modulepreload\" href=\"/writemark-live.js?v="));
    assert!(scripts.contains("<link rel=\"preload\" href=\"/page/nostorgraph_page_bg.wasm?v="));
    assert!(
        scripts.find("<script src=\"/editor-mode.js?v=").unwrap()
            < scripts
                .find("<script type=\"module\" src=\"/writemark.js?v=")
                .unwrap()
    );
    assert!(
        scripts.find("<script src=\"/relays.js?v=").unwrap()
            < scripts
                .find("<script type=\"module\" src=\"/home.js?v=")
                .unwrap()
    );
    assert!(include_str!("../templates/editor-mode.js").contains("nostorgraph-editor-mode"));
    let field = js.get("content_field").unwrap();
    assert!(field.contains("<writemark-editor"));
    assert!(field.contains("data-syntax-theme=\"ocean&quot;dark\""));
    assert!(field.contains("data-max-url-length=\"4096\""));
    assert!(field.contains("data-external-links=\"false\""));
    assert!(!nojs
        .get("content_field")
        .unwrap()
        .contains("data-syntax-theme"));
    let writemark = include_str!("../templates/writemark.js");
    let load = writemark.find("async function loadPageModule(").unwrap();
    let load_end = writemark[load..]
        .find("\nclass WritemarkEditorElement")
        .map(|offset| load + offset)
        .unwrap();
    let loader = &writemark[load..load_end];
    assert!(loader.contains("./page/nostorgraph_page.js"));
    assert!(loader.contains("./page/nostorgraph_page_bg.wasm"));
    assert!(loader.contains("module.default({ module_or_path: wasm })"));
    assert!(writemark.contains("new URL(\"./writemark.css\", import.meta.url)"));
    assert!(writemark.contains("spacer.style.blockSize"));
    assert!(!writemark.contains("<style"));
    assert!(!writemark.contains("style=\"block-size:"));
    assert!(!js
        .get("content_field")
        .unwrap()
        .contains("<textarea name=\"content\""));
}

#[test]
fn the_home_form_stores_one_csrf_token_in_the_cookie_and_the_field() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![
                    super::index,
                    super::nojs_index,
                    super::nojs_create_post,
                    super::nojs_relays_update,
                    super::assign_tor_circuits,
                ],
            ),
    )
    .unwrap();

    let first = client.get("/").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let header = first.headers().get_one("Set-Cookie").unwrap().to_string();
    assert!(header.starts_with("nostorgraph_csrf="));
    assert!(header.contains("HttpOnly"));
    assert!(header.contains("SameSite=Lax"));
    assert!(header.contains("Path=/"));
    assert!(header.contains("Max-Age=86400"));
    assert!(!header.contains("Secure"));
    let token = first
        .cookies()
        .get(crate::csrf::COOKIE_NAME)
        .unwrap()
        .value()
        .to_string();
    assert!(crate::csrf::is_valid_csrf_token(&token));
    let body = first.into_string().unwrap();
    assert!(body.contains(&format!("value=\"{token}\"")));

    let again = client.get("/nojs").dispatch();
    assert_eq!(again.status(), Status::Ok);
    let set_cookies: Vec<String> = again
        .headers()
        .get("Set-Cookie")
        .map(str::to_string)
        .collect();
    assert!(set_cookies
        .iter()
        .any(|header| header.starts_with("nostorgraph_relays=")));
    assert!(set_cookies
        .iter()
        .all(|header| !header.starts_with("nostorgraph_csrf=")));
    let again_body = again.into_string().unwrap();
    assert!(again_body.contains(&format!("value=\"{token}\"")));

    let kept = client
        .post("/nojs/create")
        .header(ContentType::Form)
        .body("title=Hello&content=Body&alias=&csrf_token=not-a-token")
        .dispatch();
    assert_eq!(kept.status(), Status::Ok);
    assert!(kept.headers().get_one("Set-Cookie").is_none());
    let kept_body = kept.into_string().unwrap();
    assert!(kept_body.contains("The publish form expired. Try again."));
    assert!(kept_body.contains(&format!("value=\"{token}\"")));
    assert!(!kept_body.contains("not-a-token"));

    let foreign = crate::csrf::generate_csrf_token_with_timestamp();
    let foreign_post = client
        .post("/nojs/create")
        .header(ContentType::Form)
        .body(format!(
            "title=Hello&content=Body&alias=&csrf_token={foreign}"
        ))
        .dispatch();
    assert_eq!(foreign_post.status(), Status::Ok);
    assert!(foreign_post.headers().get_one("Set-Cookie").is_none());
    let foreign_body = foreign_post.into_string().unwrap();
    assert!(foreign_body.contains("The publish form expired. Try again."));
    assert!(foreign_body.contains(&format!("value=\"{token}\"")));
    assert!(!foreign_body.contains(&foreign));

    let passed = client
        .post("/nojs/create")
        .header(ContentType::Form)
        .body(format!("title=&content=&alias=&csrf_token={token}"))
        .dispatch();
    assert_eq!(passed.status(), Status::Ok);
    let passed_body = passed.into_string().unwrap();
    assert!(passed_body.contains("A title is required."));
    assert!(!passed_body.contains("The publish form expired. Try again."));
    assert!(passed_body.contains(&format!("value=\"{token}\"")));

    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2Fcsrf-check.example&csrf_token={token}"
        ))
        .dispatch();
    assert_eq!(added.status(), Status::SeeOther);
    assert_eq!(added.headers().get_one("Location"), Some("/nojs"));
    drop(added);
    let added_body = client.get("/nojs").dispatch().into_string().unwrap();
    assert!(added_body.contains("data-relay=\"wss://csrf-check.example\""));
    assert!(!added_body.contains("The publish form expired. Try again."));

    let refused = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2Fcsrf-other.example&csrf_token={foreign}"
        ))
        .dispatch();
    assert_eq!(refused.status(), Status::SeeOther);
    assert_eq!(
        refused.headers().get_one("Location"),
        Some("/nojs?error=csrf_token_invalid")
    );
    drop(refused);
    let refused_body = client
        .get("/nojs?error=csrf_token_invalid")
        .dispatch()
        .into_string()
        .unwrap();
    assert!(refused_body.contains("The publish form expired. Try again."));
    assert!(!refused_body.contains("data-relay=\"wss://csrf-other.example\""));

    let circuits = client
        .post("/tor-circuits")
        .header(ContentType::JSON)
        .body(format!(r#"{{"relays":[],"csrf_token":"{token}"}}"#))
        .dispatch();
    assert_eq!(circuits.status(), Status::NoContent);
    let circuits_foreign = client
        .post("/tor-circuits")
        .header(ContentType::JSON)
        .body(format!(r#"{{"relays":[],"csrf_token":"{foreign}"}}"#))
        .dispatch();
    assert_eq!(circuits_foreign.status(), Status::BadRequest);

    let bare = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount(
                "/",
                routes![super::nojs_create_post, super::assign_tor_circuits],
            ),
    )
    .unwrap();
    let minted = crate::csrf::generate_csrf_token_with_timestamp();
    let missing = bare
        .post("/nojs/create")
        .header(ContentType::Form)
        .body(format!(
            "title=Hello&content=Body&alias=&csrf_token={minted}"
        ))
        .dispatch();
    assert_eq!(missing.status(), Status::Ok);
    assert!(missing
        .into_string()
        .unwrap()
        .contains("The publish form expired. Try again."));
    let missing_circuits = bare
        .post("/tor-circuits")
        .header(ContentType::JSON)
        .body(format!(r#"{{"relays":[],"csrf_token":"{minted}"}}"#))
        .dispatch();
    assert_eq!(missing_circuits.status(), Status::BadRequest);

    let fresh = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::index]),
    )
    .unwrap();
    let replaced = fresh
        .get("/")
        .cookie(("nostorgraph_csrf", "not-a-token"))
        .dispatch();
    let replaced_header = replaced.headers().get_one("Set-Cookie").unwrap();
    assert!(replaced_header.contains("HttpOnly"));
    assert!(!replaced_header.contains("Secure"));
    let fresh = replaced
        .cookies()
        .get(crate::csrf::COOKIE_NAME)
        .unwrap()
        .value()
        .to_string();
    assert_ne!(fresh, token);
    assert!(crate::csrf::is_valid_csrf_token(&fresh));
    let replaced_body = replaced.into_string().unwrap();
    assert!(replaced_body.contains(&format!("value=\"{fresh}\"")));
    assert!(!replaced_body.contains("not-a-token"));

    let mut off = crate::config::Config::default();
    off.security.csrf_protection_enabled = false;
    let open = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(off)
            .mount("/", routes![super::index, super::nojs_create_post]),
    )
    .unwrap();
    let plain = open.get("/").dispatch();
    assert_eq!(plain.status(), Status::Ok);
    assert!(plain.headers().get_one("Set-Cookie").is_none());
    let plain_body = plain.into_string().unwrap();
    assert!(plain_body.contains("name=\"csrf_token\""));
    assert!(plain_body.contains("value=\"\""));
    let open_post = open
        .post("/nojs/create")
        .header(ContentType::Form)
        .body("title=&content=&alias=&csrf_token=")
        .dispatch();
    assert_eq!(open_post.status(), Status::Ok);
    let open_cookies: Vec<String> = open_post
        .headers()
        .get("Set-Cookie")
        .map(str::to_string)
        .collect();
    assert!(open_cookies
        .iter()
        .any(|header| header.starts_with("nostorgraph_relays=")));
    assert!(open_cookies
        .iter()
        .all(|header| !header.starts_with("nostorgraph_csrf=")));
    let open_body = open_post.into_string().unwrap();
    assert!(open_body.contains("A title is required."));
    assert!(!open_body.contains("The publish form expired. Try again."));
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
        home_error_message(Some("relay_invalid")),
        "Use a public wss:// relay URL."
    );
    assert_eq!(
        home_error_message(Some("relay_full")),
        "Six relays are already pinned."
    );
    assert_eq!(
        home_error_message(Some("<script>")),
        "Something went wrong. Try again."
    );
    assert_eq!(
        home_error_message(Some("javascript_required")),
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
            &home_context(&config, true, Some("content_required"), None),
        )
        .unwrap();
    assert!(nojs.contains("<textarea name=\"content\""));
    assert!(!nojs.contains("writemark-editor"));
    assert!(!nojs.contains("<script src"));
    assert!(!nojs.contains("/home.js"));
    assert!(!nojs.contains("/relays.js"));
    assert!(!nojs.contains("id=\"relay-candidates\""));
    assert!(!nojs.contains("<script"));
    assert!(nojs.contains("action=\"/nojs/create\""));
    assert!(nojs.contains("Write something before publishing."));
    assert!(nojs.contains("class=\"nojs\""));
    assert!(nojs.contains("128,000 characters"));
    assert!(!nojs.contains("256,000"));

    let js = engine
        .render("home", &home_context(&config, false, None, None))
        .unwrap();
    assert!(js.contains("<writemark-editor"));
    assert!(!js.contains("<textarea name=\"content\""));
    assert!(js.contains("/writemark.js?v="));
    assert!(js.contains("data-relays="));
    assert!(js.contains("wss://"));
    assert!(js.contains("data-timeout=\"10000\""));
    assert!(js.contains(&format!(
        "data-seal-max=\"{}\"",
        crate::seal::sealed_payload_max(
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
        )
    )));
    assert!(js.contains("<p class=\"form-error\"></p>"));
    assert!(js.contains("0 / 128,000"));
    assert!(js.contains("class=\"\""));
    assert!(!js.contains("class=\"nojs\""));
    assert!(js.contains("action=\"/nojs\""));
    assert!(!js.contains("action=\"/nojs/create\""));
    assert!(js.contains("method=\"get\""));
    assert!(nojs.contains("method=\"post\""));
    assert_eq!(js.matches("On Nostr").count(), 1); // About copy
    assert!(!js.contains("Config.toml"));
    assert!(!js.contains("stored only in this browser"));
    assert!(!js.contains("class=\"nostr-publish\""));
    assert_eq!(js.matches(">Store</button>").count(), 2);
    assert_eq!(
        js.matches("<button type=\"submit\" form=\"publishForm\" disabled>Store</button>")
            .count(),
        2
    );
    assert!(js.contains("type=\"submit\""));
    assert!(nojs.contains("This host seals the note and sends it to these relays over Tor."));
    assert!(nojs.contains("Add or remove relays with the form."));
    assert!(nojs.contains("stays in this browser"));
    assert!(nojs.contains("sent with the note"));
    assert!(!nojs.contains("signs in your browser"));
    assert!(!nojs.contains("stored only in this browser"));
    assert!(nojs.contains("id=\"sidebar-relays\""));
    assert!(nojs.contains("href=\"#sidebar-relays\""));
    assert!(nojs.contains("class=\"help-link relays-open-bar\""));
    assert!(nojs.contains("class=\"nojs\""));
    assert!(nojs.contains("This host seals the note and does not keep a copy."));
    assert!(nojs.contains("The key stays in the address."));
    assert!(nojs.contains(
        "The host encrypts the note and does not keep a copy.<br>Only you, and anyone with the link, can read it.<br>The writing is ordinary markup, not a proprietary format."
    ));
    assert!(!nojs.contains("stored as plaintext markup"));
    assert!(!js.contains("stored as plaintext markup"));
    assert!(js.contains(
        "Your post is encrypted in your browser the moment you click STORE.<br>Only you, and anyone with the link, can read it.<br>This host does not keep a copy.<br>The writing is ordinary markup, not a proprietary format.<br>You can also save a copy on this device as markdown or a PDF."
    ));
    assert!(nojs.contains("class=\"nojs-file-note\""));
    assert!(js.contains("class=\"nojs-file-note\""));
    assert!(js.contains("class=\"needs-js\""));
    assert!(js.contains("This page needs JavaScript."));
    assert!(js.contains("<a href=\"/nojs\">The no-JS page</a>"));
    assert!(!js.contains("name=\"javascript_off\""));
    assert!(!js.contains(".nojs-file-note {\n                    display: block"));
    assert!(nojs.contains("class=\"needs-js\""));
    assert!(!nojs.contains("name=\"javascript_off\""));
    let nojs_relay = relay_url_input_after(&nojs, "sidebar-relays");
    assert!(nojs_relay.contains("name=\"relay\""));
    assert!(!nojs_relay.contains("name=\"relay_box\""));
    assert!(!nojs_relay.contains("form=\"publishForm\""));
    assert!(nojs.contains("action=\"/nojs/relays\""));
    assert!(nojs.contains("class=\"relay-reshuffle relay-reshuffle-quiet\""));
    assert!(nojs.contains("class=\"relay-count\">"));
    assert!(!nojs.contains("relay-delete-all"));
    assert!(!nojs.contains("name=\"relay_box\""));
    assert!(!nojs.contains("<input type=\"hidden\" name=\"relay\""));
    let nojs_help_at = nojs
        .find("<div class=\"help-section\" id=\"relays\">")
        .unwrap();
    let nojs_help = &nojs[nojs_help_at..];
    let nojs_help = &nojs_help[..nojs_help.find("</div>").unwrap()];
    assert!(nojs.contains("data-relays=\"[]\""));
    assert!(nojs_help.contains("No relays."));
    assert!(!nojs_help.contains("No public relays configured."));
    assert!(!nojs_help.contains("relay.primal.net"));
    assert!(!nojs_help.contains("Config.toml"));
    assert!(!nojs_help.contains("relay-add-btn"));
    assert!(!nojs_help.contains("relay-url-input"));
    assert!(!nojs_help.contains("signs in your browser"));
    let css = include_str!("../templates/home.css");
    let tablet = css.find("@media (max-width: 768px)").unwrap();
    let mobile = css[tablet..]
        .find("@media (max-width: 480px)")
        .map(|offset| tablet + offset)
        .unwrap();
    let tablet_css = &css[tablet..mobile];
    let mobile_css = &css[mobile..];
    assert!(tablet_css.contains("input[type=\"text\"]:not(.relay-url-input)"));
    assert!(mobile_css.contains("input[type=\"text\"]:not(.relay-url-input)"));
    assert!(css.contains("body.nojs .relay-list-user .relay-remove"));
    assert!(css.contains("min-width: 32px"));
    assert!(css.contains("min-height: 32px"));
    assert!(mobile_css.contains("body.nojs .sidebar"));
    assert!(mobile_css.contains("body.nojs .relays-open-bar"));
    assert!(mobile_css.contains("body.nojs .relays-open-sheet"));
    let js_relay = relay_url_input_after(&js, "sidebar-relays");
    assert!(!js_relay.contains("name="));
    assert!(!js_relay.contains("form="));
    assert!(js.contains("sidebar-relays"));
    assert!(!js.contains("relay.primal.net"));
    assert!(js.contains("No relays."));
    assert!(!js.contains("No public relays configured."));
    assert!(!js.contains("Config.toml"));
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

#[test]
fn nojs_publish_submits_the_relay_and_add_posts_it() {
    let config = crate::config::Config::default();
    let engine = crate::template::TemplateEngine::new("templates");
    let nojs = engine
        .render("home", &home_context(&config, true, None, None))
        .unwrap();
    assert!(nojs.contains("action=\"/nojs/create\""));
    assert_eq!(
        nojs.matches("<button type=\"submit\" form=\"publishForm\" class=\"no-relays\" disabled>Store</button>")
            .count(),
        2
    );
    let relay = relay_url_input_after(&nojs, "sidebar-relays");
    assert!(relay.contains("name=\"relay\""));
    assert!(!relay.contains("form=\"publishForm\""));
    let publish = nojs
        .find("<button type=\"submit\" form=\"publishForm\" class=\"no-relays\" disabled>Store</button>")
        .unwrap();
    let input = nojs.find("class=\"relay-url-input\"").unwrap();
    assert!(publish < input);
    let first_submit = nojs.find("<button type=\"submit\"").unwrap();
    assert!(nojs[first_submit..].starts_with(
        "<button type=\"submit\" form=\"publishForm\" class=\"no-relays\" disabled>Store</button>"
    ));
    assert!(!nojs.contains("relay-add-default"));
    assert!(!nojs[first_submit..first_submit + 80].contains("formaction"));
    let add = tag_after(
        &nojs,
        "sidebar-relays",
        "<button type=\"submit\" class=\"relay-add-btn\"",
    );
    assert!(add.contains("name=\"action\""));
    assert!(add.contains("value=\"add\""));
    assert!(!add.contains("formaction"));
    assert!(!add.contains("form=\"publishForm\""));
    assert!(nojs.contains("action=\"/nojs/relays\""));
    assert!(publish < nojs.find("relay-add-btn").unwrap());
    assert!(!nojs.contains("<script src"));
    assert!(!nojs.contains("home.js"));
    assert!(!nojs.contains("addEventListener"));

    let js = engine
        .render("home", &home_context(&config, false, None, None))
        .unwrap();
    let js_add = tag_after(
        &js,
        "sidebar-relays",
        "<button type=\"button\" class=\"relay-add-btn\">",
    );
    assert!(js_add.starts_with("<button type=\"button\""));
    assert!(!js_add.contains("form="));
    assert!(js.contains("/relays.js?v="));
    assert!(js.contains("/home.js?v="));
    assert!(!js.contains("relay-add-default"));
    assert!(!include_str!("../templates/home.css").contains("relay-add-default"));
    let home_js = include_str!("../templates/home.js");
    assert!(home_js.contains("bindRelayActions(localStorage"));
    let relays_js = include_str!("../templates/relays.js");
    let listener = relays_js.find(".relay-add-btn").unwrap();
    let click = relays_js[listener..].find("addEventListener").unwrap();
    let add_relay = relays_js.find("addRelayResult(storage").unwrap();
    assert!(add_relay < listener + click);
}

#[test]
fn nojs_relay_form_pins_a_relay_and_keeps_a_duplicate_once() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let saved = super::relays::write_relay_cookie(&super::relays::RelayCookie {
        pinned: vec!["wss://pin.example".to_string()],
        dealt: vec!["wss://dealt.example".to_string()],
    });
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::nojs_index, super::nojs_relays_update]),
    )
    .unwrap();
    let refused = client
        .post("/nojs/relays")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved.clone()))
        .header(ContentType::Form)
        .body("action=add&relay=wss%3A%2F%2FPin.Example%2F&csrf_token=")
        .dispatch();
    assert_eq!(refused.status(), Status::SeeOther);
    assert_eq!(refused.headers().get_one("Location"), Some("/nojs"));
    drop(refused);
    let html = client
        .get("/nojs")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved.clone()))
        .dispatch()
        .into_string()
        .unwrap();
    let sidebar = region(&html, "id=\"sidebar-relays\"", "class=\"footer\"");
    assert_eq!(
        sidebar.matches("data-relay=\"wss://pin.example\"").count(),
        1
    );
    assert!(!sidebar.contains("/nojs/relay\""));
    assert!(!sidebar.contains("name=\"relay_box\""));

    let accepted = client
        .post("/nojs/relays")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved))
        .header(ContentType::Form)
        .body("action=add&relay=wss%3A%2F%2Fconfig.example&csrf_token=")
        .dispatch();
    assert_eq!(accepted.status(), Status::SeeOther);
    assert_eq!(accepted.headers().get_one("Location"), Some("/nojs"));
    let stored = accepted
        .cookies()
        .get(super::relays::RELAY_LIST_COOKIE)
        .expect("pinned relay cookie")
        .value()
        .to_string();
    drop(accepted);
    let html = client
        .get("/nojs")
        .cookie((super::relays::RELAY_LIST_COOKIE, stored))
        .dispatch()
        .into_string()
        .unwrap();
    let sidebar = region(&html, "id=\"sidebar-relays\"", "class=\"footer\"");
    let help = region(
        &html,
        "<div class=\"help-section\" id=\"relays\">",
        "</div>",
    );
    let pin = sidebar.find("data-relay=\"wss://pin.example\"").unwrap();
    let dealt = sidebar.find("data-relay=\"wss://dealt.example\"").unwrap();
    assert!(pin < dealt, "{sidebar}");
    assert!(sidebar.contains("data-relay=\"wss://config.example\""));
    assert!(help.contains("config.example"));
    assert_eq!(relay_field_error(&html), "");
}

fn pinned_relays(urls: &[&str]) -> String {
    super::relays::write_relay_cookie(&super::relays::RelayCookie {
        pinned: urls.iter().map(|url| (*url).to_string()).collect(),
        dealt: Vec::new(),
    })
}

fn form_error_text(html: &str) -> &str {
    let marker = "<p class=\"form-error\">";
    let start = html.find(marker).unwrap() + marker.len();
    let end = start + html[start..].find("</p>").unwrap();
    &html[start..end]
}

fn relay_field_error(html: &str) -> &str {
    let marker = "<p class=\"relay-add-error\">";
    let start = html.find(marker).unwrap() + marker.len();
    let end = start + html[start..].find("</p>").unwrap();
    &html[start..end]
}

fn tag_after<'a>(html: &'a str, marker: &str, tag: &str) -> &'a str {
    let region = html.find(marker).unwrap();
    let start = region + html[region..].find(tag).unwrap();
    let rest = &html[start..];
    &rest[..rest.find('>').unwrap()]
}

fn relay_url_input_after<'a>(html: &'a str, marker: &str) -> &'a str {
    let region = html.find(marker).unwrap();
    let start = region
        + html[region..]
            .find("<input class=\"relay-url-input\"")
            .unwrap();
    let rest = &html[start..];
    &rest[..rest.find('>').unwrap()]
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
    fill_layout(&mut context, nojs, "abc");
    context
}

#[test]
fn pdf_fonts_load_with_the_pdf_module_and_not_the_editor() {
    let (_, page_wasm) = parser_wasm();
    let (_, pdf_wasm) = pdf_wasm();
    let (_, license) = pdf_license();
    assert!(!page_wasm.windows(6).any(|window| window == b"DejaVu"));
    assert!(pdf_wasm.windows(6).any(|window| window == b"DejaVu"));
    assert!(license.contains("Bitstream"));
    assert!(license.contains("DejaVu"));
    assert!(PDF_JS.contains("export function note_pdf(title, alias, html)"));
    assert!(!PAGE_JS.contains("note_pdf"));
    assert!(include_str!("../templates/home.html").contains("href=\"/pdf/LICENSE\""));
}

#[test]
fn post_scripts_do_not_load_the_wasm_renderer() {
    let scripts = post_scripts("abc");
    assert!(scripts.contains("/post.js?v=abc"));
    assert!(!scripts.contains("wasm"));
    assert!(!scripts.contains("nostorgraph_page"));
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
    let pdf_wasm = entity_tag_for_path(PDF_WASM_PATH).unwrap();
    let license = entity_tag_for_path(PDF_LICENSE_PATH).unwrap();
    assert_ne!(pdf_wasm, wasm);
    assert_ne!(license, pdf_wasm);
}

#[test]
fn selection_hash_keeps_the_query_string() {
    let source = include_str!("../templates/post.js");
    assert!(source.contains("document.location.search"));
}

#[test]
fn public_origin_is_optional_and_a_bad_one_is_fatal() {
    let engine = crate::template::TemplateEngine::new("templates");
    let config = crate::config::Config::default();
    let home = engine
        .render("home", &home_context(&config, false, None, None))
        .unwrap();
    assert!(home.contains("rel=\"canonical\" href=\"/\""));
    assert!(home.contains("property=\"og:url\" content=\"/\""));
    assert!(!home.contains("nostorgraph.com"));

    let mut named = crate::config::Config::default();
    named.server.public_origin = "https://example.com".to_string();
    let home = engine
        .render("home", &home_context(&named, false, None, None))
        .unwrap();
    assert!(home.contains("rel=\"canonical\" href=\"https://example.com/\""));
    assert!(home.contains("property=\"og:url\" content=\"https://example.com/\""));
    assert!(!home.contains("nostorgraph.com"));
    let about = match build_static_page("about", &named, false) {
        BuiltPage::Ready(html) => html,
        other => panic!("about page did not render: {other:?}"),
    };
    assert!(about.contains("property=\"og:url\" content=\"https://example.com/about\""));

    let temp = tempfile::tempdir().unwrap();
    for bad in [
        "http://example.com",
        "https://abcd.onion",
        "javascript:alert(1)",
    ] {
        let mut rejected = crate::config::Config::default();
        rejected.server.public_origin = bad.to_string();
        let error = rejected.normalized_public_origin().unwrap_err();
        assert!(
            error.starts_with("[server].public_origin is not an https URL"),
            "{error}"
        );
        let home = engine
            .render("home", &home_context(&rejected, false, None, None))
            .unwrap();
        assert!(!home.contains(bad), "{bad}");
        assert!(home.contains("rel=\"canonical\" href=\"/\""));
        assert!(home.contains("property=\"og:url\" content=\"/\""));

        let path = temp.path().join("bad.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[limits]
title_max_length = 128
alias_max_length = 32
content_max_length = 10

[server]
port = 9
address = "127.0.0.1"
onion_url = ""
onion_hostname_file = "/var/lib/tor/hidden_service/hostname"
public_origin = "{bad}"

[security]
max_url_length = 4096
external_link_security = true
csrf_protection_enabled = true

[theme]
syntax_highlighting = "base16-ocean.dark"
"#
            ),
        )
        .unwrap();
        let error = crate::config::Config::load_from(&path).unwrap_err();
        assert!(
            error.starts_with("[server].public_origin is not an https URL"),
            "{error}"
        );
        assert!(!error.contains("href="), "{error}");
        assert!(!error.contains("<"), "{error}");
    }
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
fn theme_toggle_is_saved_only_when_script_is_allowed() {
    let config = crate::config::Config::default();
    let engine = crate::template::TemplateEngine::new("templates");
    let js = engine
        .render("home", &home_context(&config, false, None, None))
        .unwrap();
    assert!(js.contains("class=\"theme-toggle\""));
    assert!(js.contains("rel=\"icon\" type=\"image/png\" href=\"/logo.png?v="));
    assert!(js.contains("class=\"view-mode\""));
    assert!(js.contains("data-editor-mode=\"live\""));
    assert!(js.contains("data-editor-mode=\"source\""));
    assert!(js.contains(">view</button>"));
    assert!(js.contains(">markdown</button>"));
    let tools = js.find("class=\"footer-tools\"").unwrap();
    let view = js[tools..].find("class=\"view-mode\"").unwrap();
    let save = js[tools..].find("class=\"save-note\"").unwrap();
    assert!(view < save);
    assert!(js.contains("class=\"save-formats\" id=\"save-formats\" role=\"group\" aria-label=\"Save format\" hidden"));
    assert!(include_str!("../templates/home.css").contains(
        ".save-formats {\n                position: absolute;\n                left: 100%;\n                top: 0;"
    ));
    assert!(js.contains("data-save=\"markdown\""));
    assert!(js.contains("data-save=\"pdf\""));
    assert!(js.contains(">save</button>"));
    assert!(js.contains("class=\"save-status\" role=\"status\" hidden"));
    assert!(js.contains("<script src=\"/theme.js?v="));
    assert!(include_str!("../templates/theme.js").contains("nostorgraph-theme"));
    assert!(include_str!("../templates/editor-mode.js").contains("nostorgraph-editor-mode"));
    assert!(!js.contains("&lt;script"));
    let nojs = engine
        .render("home", &home_context(&config, true, None, None))
        .unwrap();
    assert!(nojs.contains("class=\"theme-toggle\""));
    assert!(!nojs.contains("theme.js"));
    assert!(!nojs.contains("<script"));

    let post = engine.render("post", &sample_post_context(false)).unwrap();
    assert!(post.contains("<script src=\"/theme.js?v="));
    assert!(post.contains("rel=\"icon\" type=\"image/png\" href=\"/logo.png?v="));
    assert!(!post.contains("class=\"save-note\""));
    let post_nojs = engine.render("post", &sample_post_context(true)).unwrap();
    assert!(post_nojs.contains("class=\"theme-toggle\""));
    assert!(!post_nojs.contains("<script"));

    let opened = sealed_shell(&["{}".to_string()], &config);
    assert!(opened.contains("<script src=\"/theme.js?v="));
    assert!(!opened.contains("</script><script"));
    let locked = sealed_shell(&[], &config);
    assert!(locked.contains("class=\"theme-toggle\""));
    assert!(!locked.contains("<script"));
}

#[test]
fn save_reads_the_note_in_the_tab_and_names_the_file_from_the_title() {
    let home = include_str!("../templates/home.js");
    let start = home.find("function noteFields()").unwrap();
    let save = &home[start..];
    assert!(save.contains("content: editor.value"));
    assert!(save.contains("!fields.title.trim() && !fields.alias.trim() && !fields.content.trim()"));
    assert!(save.contains("if (noteIsEmpty(noteFields())) return;"));
    assert!(save.contains("saveFilename(fields.title, \".md\")"));
    assert!(save.contains("fields.content,"));
    assert!(save.contains("const page = editor.readyPage ? editor.readyPage() : null;"));
    assert!(save.contains("showSaveError(\"PDF is not ready.\");"));
    assert!(save.contains("showSaveError(\"Could not save the PDF.\");"));
    assert!(save.contains("page.render_markdown("));
    assert!(save.contains("./pdf/nostorgraph_pdf.js"));
    assert!(save.contains("./pdf/nostorgraph_pdf_bg.wasm"));
    assert!(save.contains("module.default({ module_or_path: wasm })"));
    let rendered = save.find("page.render_markdown(").unwrap();
    let loaded = save.find("await loadPdfModule()").unwrap();
    let drawn = save.find("pdf.note_pdf(").unwrap();
    let download = save.find("saveFilename(fields.title, \".pdf\")").unwrap();
    assert!(rendered < loaded && loaded < drawn && drawn < download);
    assert!(save.contains("fields.title.trim(), fields.alias.trim(), html"));
    assert!(save.contains("cleaned || \"note\""));
    assert!(save.contains("savePdf(fields);"));
    let chosen = save.find("choice.getAttribute(\"data-save\")").unwrap();
    let after_choice = &save[chosen..];
    let markdown = after_choice.find("saveMarkdown(fields);").unwrap();
    let pdf = after_choice.find("savePdf(fields);").unwrap();
    assert!(after_choice[..markdown].contains("setSaveOpen(false);"));
    assert!(after_choice[markdown..pdf].contains("setSaveOpen(false);"));
    assert_eq!(save.matches("await ").count(), 1);
    assert!(!save.contains("page.note_pdf"));
    assert!(!save.contains("fetch("));
    assert!(!save.contains("XMLHttpRequest"));
    assert!(!save.contains("sendBeacon"));
    assert!(!save.contains("form.submit"));
    let writemark = include_str!("../templates/writemark.js");
    assert!(writemark.contains("readyPage() {\n    return pageModule;\n  }"));
}

#[test]
fn sealed_shell_keeps_the_note_out_of_the_title_and_preview() {
    let secret = "VisibleTitleSecretBody";
    let event = format!(
        r#"{{"content":"{secret}</script>","tags":[["title","{secret}"],["summary","{secret}"]]}}"#
    );
    let mut config = crate::config::Config::default();
    config.theme.syntax_highlighting = "ocean\"dark".to_string();
    config.security.max_url_length = 4096;
    config.security.external_link_security = false;
    let html = sealed_shell(std::slice::from_ref(&event), &config);
    assert!(html.contains("<title>Sealed note</title>"));
    assert!(html.contains("rel=\"icon\" type=\"image/png\" href=\"/logo.png?v="));
    assert!(html.contains("property=\"og:title\" content=\"Sealed note\""));
    assert!(html.contains("property=\"og:description\" content=\"A note sealed in the browser.\""));
    assert!(html.contains("name=\"description\" content=\"A note sealed in the browser.\""));
    assert!(html.contains("id=\"sealed-title\">Sealed note</h1>"));
    assert!(!html.contains("id=\"sealed-preview\""));
    assert!(html.contains("data-syntax-theme=\"ocean&quot;dark\""));
    assert!(html.contains("data-max-url-length=\"4096\""));
    assert!(html.contains("data-external-links=\"false\""));
    assert!(html.contains("id=\"CONTENT\""));
    assert!(html.contains("/post.css?v="));
    let head = html.split("<script").next().unwrap();
    assert!(!head.contains(secret));
    assert!(html.contains(secret));
    assert!(html.contains("\\u003c/script>"));
    assert!(html.contains(&format!(
        "<div id=\"sealed-event\" hidden>[{}]</div>",
        event.replace('<', "\\u003c")
    )));
    assert!(!html.contains("<script type=\"application/json\""));
    assert!(html.contains("<script type=\"module\" src=\"/nostr.js?v="));
    assert!(html.contains("id=\"sealed-copy\">Copy link</button>"));
    assert!(html.contains("id=\"sealed-open\""));
    assert!(html.contains("id=\"sealed-key\""));
    assert!(html.contains("type=\"button\">Open</button>"));
    assert!(html.contains("The key is the part of the address after #."));
    assert!(html.contains("Run locally to read the note."));
    assert!(html.contains("https://github.com/wuddleko/nostorgraph"));
    assert!(!html.contains("name=\"k\""));
    assert!(!html.contains("method=\"post\""));
    assert!(!html.contains("action="));
    assert!(!html.contains("/nojs/s/"));
    assert!(!html.contains("?k="));
    assert!(html.contains("href=\"#help-sheet\">about</a>"));
    assert!(html.contains("id=\"help-sheet\""));
    assert!(html.contains("About Nostorgraph"));
    assert!(html.contains("What's this?"));
    assert!(html.contains("encrypted in your browser"));
    assert!(html.contains("<h3>Links</h3>"));
    assert!(html.contains(
        "Most private. This site stores nothing. The event and the relays are in the address. The key is the part after #."
    ));
    assert!(html.contains(
        "This site stores the event id and the relays. It does not store the writing or the key."
    ));
    assert!(html.contains("Least private. This site stores the title, the event id, and the relays. The title is in the address. It does not store the writing or the key."));
    assert!(
        html.contains("https://&lt;site&gt;/&lt;locator&gt;/&lt;event-id&gt;?r=&lt;relay&gt;#key")
    );
    assert!(html.contains("https://&lt;site&gt;/&lt;locator&gt;#key"));
    assert!(html.contains("https://&lt;site&gt;/&lt;title&gt;#key"));
    assert!(!html.contains("ends in -2"));
    assert!(!html.contains("href=\"/about\">about</a>"));
    assert!(html.contains(">legal</a>"));
    assert!(html.contains(">api</a>"));
    assert!(html.contains(">source code</a>"));
    assert!(html.contains("class=\"save-note\" hidden"));
    assert!(html.contains("data-save=\"markdown\""));
    assert!(html.contains("data-save=\"pdf\""));
    assert!(html.contains(">save</button>"));
    assert!(include_str!("../templates/post.css").contains(
        ".footer .save-formats {\n                position: absolute;\n                left: 100%;\n                top: 0;"
    ));
    assert!(!html.contains("location.href"));
    assert!(!html.contains("location.hash"));
    assert!(!html.contains("</script><script"));
    assert!(crate::nostr::sealed_locator("0123456789abcdef0123"));
    assert!(!crate::nostr::sealed_locator("0123456789ABCDEF0123"));
    let nostr = include_str!("../templates/nostr.js");
    let sign = nostr.find("async function schnorrSign(").unwrap();
    let verify = nostr.find("async function schnorrVerify(").unwrap();
    assert!(sign < verify);
    let open = nostr
        .find("export async function openEmbeddedSeal(")
        .unwrap();
    let open_end = nostr[open..]
        .find("\nfunction renderOpenedNote")
        .map(|offset| open + offset)
        .unwrap();
    let body = &nostr[open..open_end];
    let checked = body.find("await verifySealedEvent(event)").unwrap();
    let fragment = body.find("secretFromFragment(").unwrap();
    let opened = body.find("await page.open(").unwrap();
    let rendered = body.find("renderOpenedNote(note, page)").unwrap();
    assert!(checked < fragment);
    assert!(fragment < opened);
    assert!(opened < rendered);
    assert!(body.contains("sealed-key"));
    let draw = nostr.find("function renderOpenedNote(").unwrap();
    let draw_end = nostr[draw..]
        .find("\nasync function verifySealedEvent")
        .map(|offset| draw + offset)
        .unwrap();
    let draw_body = &nostr[draw..draw_end];
    assert!(draw_body.contains("page.render_markdown(note.content, theme, maxUrl, external)"));
    assert!(draw_body.contains("content.innerHTML"));
    assert!(draw_body.contains("heading.textContent"));
    assert!(draw_body.contains("document.title = noteTitle"));
    assert!(draw_body.contains("showSealedSave(note, page)"));
    assert!(draw_body.contains("bindSecretReveal(content)"));
    assert!(nostr.contains("classList.toggle(\"revealed\")"));
    assert!(include_str!("../templates/post.js").contains("closest(\".secret\")"));
    assert!(include_str!("../templates/writemark.css").contains(".md-line .secret"));
    assert!(include_str!("../templates/writemark.js").contains("classList.add(\"revealed\")"));
    assert!(!draw_body.contains("location.hash"));
    assert!(!draw_body.contains("event.content"));
    assert!(nostr.contains("sealedSaveFilename(fields.title, \".md\")"));
    assert!(nostr.contains("fields.content,"));
    assert!(nostr.contains("pdf.note_pdf(fields.title.trim(), fields.alias.trim(), html)"));
    assert!(nostr.contains("./pdf/nostorgraph_pdf.js"));
    assert!(nostr.contains("./pdf/nostorgraph_pdf_bg.wasm"));
    assert!(nostr.contains("root.hidden = false"));
    let locked_shell = sealed_shell(&[], &config);
    assert!(!locked_shell.contains("save-note"));
    assert!(PAGE_JS.contains(
        "export function render_markdown(markdown, syntax_theme, max_url_length, external_link_security)"
    ));
    assert!(!PAGE_JS.contains("note_pdf"));
    assert_eq!(body.matches("location.hash").count(), 1);
    assert!(!body.contains("fetch("));
    assert!(!body.contains("XMLHttpRequest"));
    assert!(!body.contains("sendBeacon"));
    assert!(!body.contains("location.href"));
    assert!(!body.contains("location.assign"));
    assert!(!body.contains("innerHTML"));
    assert!(nostr.contains("await schnorrVerify(pubkey, message, sigBytes)"));
    assert!(nostr.contains("if (await schnorrVerify(pubkey, message, flipped))"));
    let copy = nostr.find("export function copySealedLink(").unwrap();
    let copy_end = nostr[copy..]
        .find("\nfunction copyText")
        .map(|offset| copy + offset)
        .unwrap();
    let copy_body = &nostr[copy..copy_end];
    assert!(copy_body.contains("clipboard.writeText(location.href)"));
    assert!(copy_body.contains("copyText(location.href)"));
    assert!(!copy_body.contains("location.hash"));
    assert!(!copy_body.contains("split("));
    assert!(!copy_body.contains("fetch("));
    assert!(!copy_body.contains("location.assign"));
    assert!(!copy_body.contains("XMLHttpRequest"));
    assert!(!copy_body.contains("sendBeacon"));

    let footer = nostr.find("export function sealedReadingFooter(").unwrap();
    let prefill = nostr.find("function prefillSealedKey(").unwrap();
    let load = nostr
        .rfind("if (typeof document !== \"undefined\")")
        .unwrap();
    assert!(footer < prefill);
    assert!(prefill < load);
    let footer_body = &nostr[footer..prefill];
    assert!(footer_body.contains(".footer a[href=\"/api\"]"));
    assert!(footer_body.contains("[\"#help-sheet\", \"about\"]"));
    let about = nostr.find("function bindSealedAbout(").unwrap();
    let about_end = nostr[about..].find("\nif (typeof document").unwrap();
    let about_body = &nostr[about..about + about_end];
    assert!(about_body.contains("getElementById(\"sealed-open\")"));
    assert!(about_body.contains("event.preventDefault()"));
    assert!(about_body.contains("classList.add(\"is-open\")"));
    assert!(about_body.contains("classList.remove(\"is-open\")"));
    assert!(!about_body.contains("location.hash"));
    assert!(!about_body.contains("location.href"));
    let save_bind = nostr.find("function bindSealedSave(").unwrap();
    let save_bind_end = nostr[save_bind..]
        .find("\nasync function verifySealedEvent")
        .unwrap();
    let save_bind_body = &nostr[save_bind..save_bind + save_bind_end];
    let save_guard = save_bind_body
        .find("getElementById(\"sealed-open\")")
        .unwrap();
    let save_note = save_bind_body
        .find("querySelector(\".save-note\")")
        .unwrap();
    let save_bound = save_bind_body.find("dataset.bound").unwrap();
    assert!(save_bind_body.contains("getElementById(\"sealed-open\")) return"));
    assert!(save_guard < save_note && save_note < save_bound);
    let sealed_choice = save_bind_body
        .find("choice.getAttribute(\"data-save\")")
        .unwrap();
    let after_sealed_choice = &save_bind_body[sealed_choice..];
    let sealed_markdown = after_sealed_choice
        .find("saveSealedMarkdown(fields);")
        .unwrap();
    let sealed_pdf = after_sealed_choice.find("saveSealedPdf(fields);").unwrap();
    assert!(after_sealed_choice[..sealed_markdown].contains("setSealedSaveOpen(false);"));
    assert!(after_sealed_choice[sealed_markdown..sealed_pdf].contains("setSealedSaveOpen(false);"));
    assert!(!footer_body.contains("?k="));
    assert!(!footer_body.contains("/nojs/s/"));
    assert!(!footer_body.contains("secretFromFragment"));
    let prefill_body = &nostr[prefill..load];
    assert!(prefill_body.contains("secretFromFragment(location.hash)"));
    assert!(prefill_body.contains("location.hash.slice(1)"));
    assert!(prefill_body.contains("sealed-key"));
    assert!(!prefill_body.contains("?k="));
    assert!(!prefill_body.contains("/nojs/s/"));
    let load_body = &nostr[load..];
    assert!(load_body.contains("bindSealedSave()"));
    assert!(load_body.contains("bindSealedAbout()"));
    assert!(load_body.contains("prefillSealedKey()"));
    assert!(load_body.contains("secretFromFragment(location.hash)"));
    let hidden = load_body.find("form.hidden = true").unwrap();
    let opened = load_body
        .find("openEmbeddedSeal(sealedEvent.textContent)")
        .unwrap();
    assert!(hidden < opened);
    assert!(load_body.contains("form.hidden = false"));
    assert!(load_body.contains("openButton.addEventListener(\"click\", openNote)"));
    assert!(load_body.contains("event.preventDefault()"));
    assert!(load_body.contains("openEmbeddedSeal(sealedEvent.textContent)"));
    assert!(!load_body.contains("form.addEventListener(\"submit\""));
    assert!(!load_body.contains("linkNojsFromHash"));
    assert!(!nostr.contains("?k="));
    assert!(!nostr.contains("/nojs/s/"));
}

fn sealed_route_headers(response: &rocket::local::blocking::LocalResponse<'_>) {
    assert_eq!(
        response.headers().get_one("Referrer-Policy"),
        Some("no-referrer")
    );
    assert_eq!(
        response.headers().get_one("Cache-Control"),
        Some("no-store")
    );
}

#[test]
fn nojs_sealed_route_sets_no_referrer_and_no_store() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::nojs_sealed_view]),
    )
    .unwrap();
    let event_id = "ab".repeat(32);
    let missing = client
        .get(format!("/nojs/s/not-a-locator/{event_id}?k=secret"))
        .dispatch();
    assert_eq!(missing.status(), Status::NotFound);
    sealed_route_headers(&missing);
    let missing_body = missing.into_string().unwrap();
    assert_eq!(missing_body, not_found_html());
    assert!(missing_body.contains("/notice.css?v="));
    assert!(!missing_body.contains("<style"));
    assert!(!missing_body.contains("secret"));

    let local = client
        .get(format!(
            "/nojs/s/0123456789abcdef0123/{event_id}?k=LOGSECRETVALUE"
        ))
        .dispatch();
    assert_eq!(local.status(), Status::Ok);
    sealed_route_headers(&local);
    assert!(local.headers().get_one("Location").is_none());
    let body = local.into_string().unwrap();
    assert!(body.contains("<h1>Run locally to read the note</h1>"));
    assert!(body.contains("https://github.com/wuddleko/nostorgraph"));
    assert!(body.contains("http-equiv=\"refresh\""));
    assert!(body.contains("name=\"referrer\" content=\"no-referrer\""));
    assert!(!body.contains("LOGSECRETVALUE"));
    assert!(!body.contains("?k="));
    assert!(!body.contains("id=\"sealed-open\""));
}

#[get("/seal-fragment")]
fn seal_fragment_redirect() -> rocket::response::Redirect {
    rocket::response::Redirect::to(
        "/s/0123456789abcdef0123/abababababababababababababababababababababababababababababababab?r=wss%3A%2F%2Fhint.example#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    )
}

#[test]
fn publish_redirect_keeps_the_key_fragment() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build().mount("/", routes![seal_fragment_redirect]),
    )
    .unwrap();
    let response = client.get("/seal-fragment").dispatch();
    assert_eq!(response.status(), Status::SeeOther);
    assert_eq!(
        response.headers().get_one("Location"),
        Some("/s/0123456789abcdef0123/abababababababababababababababababababababababababababababababab?r=wss%3A%2F%2Fhint.example#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
    );
}

#[test]
fn nojs_sealed_address_says_to_run_locally() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::nojs_sealed_view]),
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let key = "A".repeat(43);
    let page = client
        .get(format!(
            "/nojs/s/{locator}/{event_id}?k={key}&r=wss://hint.example"
        ))
        .dispatch();
    assert_eq!(page.status(), Status::Ok);
    let body = page.into_string().unwrap();
    assert!(body.contains("<h1>Run locally to read the note</h1>"));
    assert!(body.contains(
        "http-equiv=\"refresh\" content=\"0; url=https://github.com/wuddleko/nostorgraph\""
    ));
    assert!(body.contains("<a href=\"https://github.com/wuddleko/nostorgraph\">source code</a>"));
    assert!(!body.contains(&key));
    assert!(!body.contains("hint.example"));
    assert!(!body.contains(&format!("/{locator}/{event_id}")));
    let plain = client
        .get(format!("/nojs/s/{locator}/{event_id}?k=not-a-secret"))
        .dispatch();
    assert_eq!(plain.status(), Status::Ok);
    let plain_body = plain.into_string().unwrap();
    assert!(plain_body.contains("Run locally to read the note"));
    assert!(!plain_body.contains("not-a-secret"));
    let missing = client
        .get(format!("/nojs/s/not-a-locator/{event_id}?k={key}"))
        .dispatch();
    assert_eq!(missing.status(), Status::NotFound);
    assert!(missing.headers().get_one("Location").is_none());
    assert!(!missing.into_string().unwrap().contains(&key));
}

#[test]
fn nojs_short_address_says_to_run_locally() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::nojs_sealed_locator_view]),
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let key = "A".repeat(43);
    let page = client.get(format!("/nojs/s/{locator}?k={key}")).dispatch();
    assert_eq!(page.status(), Status::Ok);
    sealed_route_headers(&page);
    let body = page.into_string().unwrap();
    assert!(body.contains("<h1>Run locally to read the note</h1>"));
    assert!(!body.contains(&key));
    let missing = client.get("/nojs/s/not-a-locator").dispatch();
    assert_eq!(missing.status(), Status::NotFound);
    assert!(!missing.into_string().unwrap().contains(&key));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn sealed_view_uses_a_slot_and_a_missing_event_is_not_found() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
        }
    }
    let _clear = ClearFetch;

    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    set_nojs_test_fetch(NojsTestFetch::Missing);
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::sealed_view]),
    )
    .unwrap();

    let mut held = Vec::new();
    for _ in 0..MAX_PUBLIC_FETCHES {
        held.push(PublicFetchSlot::acquire().expect("public fetch slot"));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let busy = client.get(format!("/s/{locator}/{event_id}")).dispatch();
        assert_eq!(busy.status(), Status::ServiceUnavailable);
        assert_eq!(busy.into_string().unwrap(), "Service unavailable");

        held.pop();
        let missing = client.get(format!("/s/{locator}/{event_id}")).dispatch();
        assert_eq!(missing.status(), Status::NotFound);
        assert_eq!(missing.into_string().unwrap(), not_found_html());
    }));
    drop(held);
    result.unwrap();
}

#[test]
fn sealed_query_reuses_public_relays() {
    let hints = vec![
        "wss://relay.damus.io".to_string(),
        "wss://127.0.0.1".to_string(),
    ];
    let configured = vec!["wss://relay.primal.net".to_string()];
    assert_eq!(
        public_relays(&hints),
        vec!["wss://relay.damus.io".to_string()]
    );
    assert!(!public_relays(&hints)
        .iter()
        .any(|relay| configured.contains(relay)));
    let started = std::time::Instant::now();
    let event_id = "ab".repeat(32);
    let pool = std::path::Path::new("missing-relay-pool.json");
    assert!(fetch_sealed_by_id(
        &["wss://127.0.0.1".to_string()],
        &event_id,
        "0123456789abcdef0123",
        128,
        32,
        4,
        std::time::Duration::from_millis(20),
        pool,
    )
    .is_none());
    assert!(started.elapsed() < std::time::Duration::from_millis(200));
    assert!(fetch_sealed_by_id(
        &configured,
        &event_id,
        "0123456789ABCDEF0123",
        128,
        32,
        4,
        std::time::Duration::from_millis(20),
        pool,
    )
    .is_none());
}

#[test]
fn a_fetch_list_returns_only_the_link_relay() {
    let link = vec![
        "wss://First.Example/".to_string(),
        "wss://second.example".to_string(),
        "wss://127.0.0.1".to_string(),
    ];
    let configured = "wss://config.example";
    let relays = public_relays(&link);
    assert_eq!(
        relays,
        vec![
            "wss://first.example".to_string(),
            "wss://second.example".to_string(),
        ]
    );
    assert!(!relays.iter().any(|relay| relay.contains(configured)));
    let mut many = Vec::new();
    for index in 0..8 {
        many.push(format!("wss://link{index}.example"));
    }
    let capped = public_relays(&many);
    assert_eq!(capped.len(), crate::nostr::MAX_FETCH_RELAYS);
    assert_eq!(capped[0], "wss://link0.example");
    assert_eq!(capped[5], "wss://link5.example");
    assert!(!capped.iter().any(|relay| relay.contains(configured)));
}

#[test]
fn a_fetch_with_dead_link_relays_reads_from_a_usable_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    let row = |url: &str, usable: bool| crawler::Row {
        url: url.to_string(),
        first_seen: 1_700_000_000,
        next_try: 1_700_000_100,
        failures: 0,
        publish_refused: false,
        visit: Some(crawler::Visit {
            at: 1_700_000_100,
            socket_answered: usable,
            nip11: Some(document.clone()),
            probes: vec![crawler::ProbeResult {
                name: "30323".to_string(),
                accepted: usable,
                detail: "OK".to_string(),
            }],
        }),
    };
    crawler::save(
        &path,
        &crawler::Pool {
            rows: vec![
                row("wss://closed.example", false),
                row("wss://dead.example", true),
                row("wss://row.example", true),
            ],
        },
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_sealed_note("kept", locator, 1_700_000_000);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://row.example", &note.event_json);
    let event_id = note.id_hex();
    let started = std::time::Instant::now();
    assert_eq!(
        fetch_sealed_by_id(
            &["wss://dead.example".to_string()],
            &event_id,
            locator,
            128,
            32,
            4,
            std::time::Duration::from_secs(2),
            &path,
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert_eq!(
        fetch_sealed_by_id(
            &[],
            &event_id,
            locator,
            128,
            32,
            4,
            std::time::Duration::from_secs(2),
            &path,
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    drop(answers);
}

#[test]
fn a_fetch_whose_link_relay_has_the_note_does_not_read_the_fixture_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    crawler::save(
        &path,
        &crawler::Pool {
            rows: vec![crawler::Row {
                url: "wss://row.example".to_string(),
                first_seen: 1_700_000_000,
                next_try: 1_700_000_100,
                failures: 0,
                publish_refused: false,
                visit: Some(crawler::Visit {
                    at: 1_700_000_100,
                    socket_answered: true,
                    nip11: Some(document),
                    probes: vec![crawler::ProbeResult {
                        name: "30323".to_string(),
                        accepted: true,
                        detail: "OK".to_string(),
                    }],
                }),
            }],
        },
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_sealed_note("kept", locator, 1_700_000_000);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://live.example", &note.event_json);
    answers.answer("wss://row.example", "pool-note");
    let event_id = note.id_hex();
    assert_eq!(
        fetch_sealed_by_id(
            &["wss://live.example".to_string()],
            &event_id,
            locator,
            128,
            32,
            4,
            std::time::Duration::from_secs(2),
            &path,
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert!(answers.asked("wss://live.example"));
    assert!(!answers.asked("wss://row.example"));
}

#[test]
fn a_long_link_asks_every_named_relay_and_a_seventh_can_come_from_the_pool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    let row = |url: &str| crawler::Row {
        url: url.to_string(),
        first_seen: 1_700_000_000,
        next_try: 1_700_000_100,
        failures: 0,
        publish_refused: false,
        visit: Some(crawler::Visit {
            at: 1_700_000_100,
            socket_answered: true,
            nip11: Some(document.clone()),
            probes: vec![crawler::ProbeResult {
                name: "30323".to_string(),
                accepted: true,
                detail: "OK".to_string(),
            }],
        }),
    };
    crawler::save(
        &path,
        &crawler::Pool {
            rows: vec![row("wss://extra.example"), row("wss://row.example")],
        },
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_sealed_note("kept", locator, 1_700_000_000);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://row.example", &note.event_json);
    let event_id = note.id_hex();
    assert_eq!(
        fetch_sealed_by_id(
            &[
                "wss://hint0.example".to_string(),
                "wss://hint1.example".to_string(),
                "wss://hintonly.example".to_string(),
                "wss://extra.example".to_string(),
                "wss://row.example".to_string(),
            ],
            &event_id,
            locator,
            128,
            32,
            4,
            std::time::Duration::from_secs(2),
            &path,
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert!(answers.asked("wss://hint0.example"));
    assert!(answers.asked("wss://hint1.example"));
    assert!(answers.asked("wss://hintonly.example"));
    assert!(answers.asked("wss://extra.example"));
    assert!(answers.asked("wss://row.example"));

    let beyond: Vec<String> = (0..6)
        .map(|index| format!("wss://miss{index}.example"))
        .chain(["wss://row.example".to_string()])
        .collect();
    let again = crate::nostr::RelayAnswer::begin();
    again.answer("wss://row.example", &note.event_json);
    assert_eq!(
        fetch_sealed_by_id(
            &beyond,
            &event_id,
            locator,
            128,
            32,
            4,
            std::time::Duration::from_secs(2),
            &path,
        )
        .as_deref(),
        Some(note.event_json.as_str())
    );
    assert!(again.asked("wss://miss0.example"));
    assert!(again.asked("wss://miss5.example"));
    assert!(again.asked("wss://row.example"));
}

#[test]
fn public_fetch_keeps_the_link_relays_in_order() {
    let hints = vec![
        "wss://hint.example".to_string(),
        "wss://relay.damus.io".to_string(),
    ];
    assert_eq!(
        public_relays(&hints),
        vec![
            "wss://hint.example".to_string(),
            "wss://relay.damus.io".to_string(),
        ]
    );
    assert!(public_relays(&[]).is_empty());
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
    let relays = public_relays(&hints);
    assert_eq!(relays.len(), crate::nostr::MAX_FETCH_RELAYS);
    assert_eq!(relays[0], "wss://hint0.example");
    assert_eq!(relays.last().unwrap(), "wss://hint5.example");
    assert!(!relays.iter().any(|relay| relay.contains("127.0.0.1")));
    assert!(!relays.iter().any(|relay| relay.contains("localhost")));
}

#[test]
fn public_fetch_uses_every_hint_slot_when_the_instance_has_no_public_relay() {
    let mut hints = Vec::new();
    for index in 0..8 {
        hints.push(format!("wss://hint{index}.example"));
    }
    let relays = public_relays(&hints);
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
#[serial(nojs_sealed_fetch)]
fn nojs_create_keeps_csrf_and_validates_the_trimmed_fields_and_raw_body() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = true;
    let mut form = NewPost {
        title: format!(" {} ", "H".repeat(config.limits.title_max_length)),
        content: " \n\t ".to_string(),
        alias: format!(" {} ", "A".repeat(config.limits.alias_max_length)),
        csrf_token: "not-a-token".to_string(),
    };

    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=csrf_token_invalid");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.csrf_token = crate::csrf::generate_csrf_token_with_timestamp();
    let other = crate::csrf::generate_csrf_token_with_timestamp();
    let href = create_location(&form, &config, &other);
    assert_eq!(href, "/nojs?error=csrf_token_invalid");
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=csrf_token_invalid");
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, &form.csrf_token);
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = format!(" {} ", "y".repeat(config.limits.content_max_length));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, &form.csrf_token);
    assert_eq!(href, "/nojs?error=content_too_long");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = "  hi  ".to_string();
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://nojs-seal.invalid".to_string()];
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, &form.csrf_token);
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_post_writes_no_file_and_redirects_with_the_key_or_an_error() {
    struct ClearAccepted;
    impl Drop for ClearAccepted {
        fn drop(&mut self) {
            set_nojs_test_accepted(None);
        }
    }
    let _clear = ClearAccepted;
    let holds = HoldDir::new();

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    let instance = "wss://instance.example".to_string();
    config.nostr.relays = vec![instance.clone()];
    let form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };

    set_nojs_test_accepted(Some(vec![instance.clone()]));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&pinned_relays(&[&instance])));
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let link = seal_link_parts(&href);
    assert_eq!(link.query, "r=wss%3A%2F%2Finstance.example", "{href}");
    assert!(crate::nostr::sealed_locator(&link.locator), "{href}");
    assert!(
        crate::nostr::sealed_event_id(link.event_id.as_deref().unwrap_or("")),
        "{href}"
    );
    assert!(is_seal_key(&link.key), "{href}");
    assert!(href.contains(&link.locator) && href.contains(&link.key));
    assert_eq!(hold_dir_count(holds.path()), 0);
    assert!(super::seal_row::read(&link.locator).is_none());
    let locks: Vec<_> = std::fs::read_dir(holds.path().join("seal-locks"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| !entry.file_name().to_string_lossy().ends_with(".writing"))
        .collect();
    assert_eq!(locks.len(), 1);
    let stored = std::fs::read_to_string(locks[0].path()).unwrap();
    assert!(!stored.contains("Hello World"), "{stored}");
    assert!(!stored.contains(&link.key), "{stored}");
    assert!(
        !stored.contains(link.event_id.as_deref().unwrap()),
        "{stored}"
    );
    set_nojs_test_accepted(None);

    let mut rejected = form.clone();
    config.security.csrf_protection_enabled = true;
    rejected.csrf_token = "not-a-token".to_string();
    assert_nojs_error(&rejected, &config, "csrf_token_invalid");

    config.security.csrf_protection_enabled = false;
    rejected.csrf_token.clear();
    rejected.title.clear();
    assert_nojs_error(&rejected, &config, "title_required");

    rejected.title = "Hello World".to_string();
    rejected.content = "   ".to_string();
    assert_nojs_error(&rejected, &config, "content_required");

    rejected.content = "hi".to_string();
    set_nojs_test_accepted(Some(vec!["wss://hint.example".to_string()]));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(
        &rejected,
        &config,
        "",
        Some(&pinned_relays(&["wss://hint.example"])),
    );
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let link = seal_link_parts(&href);
    assert!(crate::nostr::sealed_locator(&link.locator), "{href}");
    assert!(
        crate::nostr::sealed_event_id(link.event_id.as_deref().unwrap_or("")),
        "{href}"
    );
    assert!(is_seal_key(&link.key), "{href}");
    assert_eq!(link.query, "r=wss%3A%2F%2Fhint.example", "{href}");
    assert!(!href.contains("nostr_publish_failed"), "{href}");
}

#[test]
#[serial(nojs_sealed_fetch)]
fn opening_a_sealed_note_stays_in_the_browser() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_accepted(None);
            set_nojs_test_fetch(NojsTestFetch::Live);
        }
    }
    let _clear = ClearFetch;
    let _holds = HoldDir::new();

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    let instance = "wss://instance.example".to_string();
    config.nostr.relays = vec![instance.clone()];
    let form = NewPost {
        title: "Redirect title".to_string(),
        content: "opened note body".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };
    set_nojs_test_accepted(Some(vec![instance.clone()]));
    set_nojs_test_fetch(NojsTestFetch::Capture);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&pinned_relays(&[&instance])));
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let link = seal_link_parts(&href);
    assert_eq!(link.query, "r=wss%3A%2F%2Finstance.example", "{href}");
    let locator = link.locator;
    let event_id = link.event_id.expect("event id");
    let key = link.key;

    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![
            super::sealed_view,
            super::sealed_locator_view,
            super::nojs_sealed_view,
            super::nojs_sealed_locator_view,
        ],
    ))
    .unwrap();

    let path = format!("/s/{locator}/{event_id}");
    let shell = client.get(&path).dispatch();
    assert_eq!(shell.status(), Status::Ok);
    let shell_body = shell.into_string().unwrap();
    assert!(shell_body.contains("id=\"sealed-open\""));
    assert!(shell_body.contains("id=\"sealed-event\""));
    assert!(shell_body.contains("Run locally to read the note."));
    assert!(!shell_body.contains("name=\"k\""));
    assert!(!shell_body.contains("Redirect title"));
    assert!(!shell_body.contains("opened note body"));

    let posted = client
        .post(&path)
        .header(ContentType::Form)
        .body(format!("k={key}"))
        .dispatch();
    assert_ne!(posted.status(), Status::Ok);
    let posted_body = posted.into_string().unwrap_or_default();
    assert!(!posted_body.contains("Redirect title"));
    assert!(!posted_body.contains("opened note body"));
    assert!(!posted_body.contains(&key));

    let local = client.get(format!("/nojs/s/{locator}?k={key}")).dispatch();
    assert_eq!(local.status(), Status::Ok);
    sealed_route_headers(&local);
    let local_body = local.into_string().unwrap();
    assert!(local_body.contains("Run locally to read the note"));
    assert!(!local_body.contains(&key));
    assert!(!local_body.contains("Redirect title"));
    assert!(!local_body.contains("opened note body"));
}

fn is_seal_key(key: &str) -> bool {
    key.len() == 43
        && key
            .bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

struct ParsedSealLink {
    locator: String,
    event_id: Option<String>,
    key: String,
    query: String,
}

fn seal_link_parts(href: &str) -> ParsedSealLink {
    let (before, key) = href.split_once('#').unwrap_or_else(|| panic!("{href}"));
    assert!(!before.contains("?k=") && !before.contains("&k="), "{href}");
    let (path, query) = before.split_once('?').unwrap_or((before, ""));
    let rest = path.strip_prefix('/').unwrap_or_else(|| panic!("{href}"));
    let (locator, event_id) = match rest.split_once('/') {
        Some((locator, event_id)) => (locator, Some(event_id.to_string())),
        None => (rest, None),
    };
    ParsedSealLink {
        locator: locator.to_string(),
        event_id,
        key: key.to_string(),
        query: query.to_string(),
    }
}

fn seal_link_with_event_id(href: &str) -> (String, String, String, String) {
    let link = seal_link_parts(href);
    let event_id = link.event_id.expect("event id");
    (link.locator, event_id, link.key, link.query)
}

#[test]
fn seal_address_keeps_the_event_id_until_it_is_omitted() {
    let event_id = "ab".repeat(32);
    let key = "Kw7s_pQ2mN4xR8vL0aBcDeFgHiJkLmNoPqRsTuVwXyZ";
    let hints = vec![
        "wss://hint.example".to_string(),
        "http://private.example".to_string(),
    ];
    let href = super::layout::seal_href(&super::layout::SealAddress {
        locator: "0123456789abcdef0123",
        event_id: Some(&event_id),
        key,
        hints: &hints,
    });
    let (locator, found_id, found_key, query) = seal_link_with_event_id(&href);
    assert_eq!(locator, "0123456789abcdef0123");
    assert_eq!(found_id, event_id);
    assert_eq!(found_key, key);
    assert_eq!(query, "r=wss%3A%2F%2Fhint.example");

    let short = super::layout::seal_href(&super::layout::SealAddress {
        locator: "0123456789abcdef0123",
        event_id: None,
        key,
        hints: &[],
    });
    let link = seal_link_parts(&short);
    assert_eq!(link.locator, "0123456789abcdef0123");
    assert!(link.event_id.is_none());
    assert_eq!(link.key, key);
    assert!(link.query.is_empty());
    assert_eq!(short, format!("/0123456789abcdef0123#{key}"));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn an_added_relay_keeps_the_link_and_the_open_asks_that_relay() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_accepted(None);
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;
    let _holds = HoldDir::new();

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://instance.example".to_string()];
    let form = NewPost {
        title: "Added relay title".to_string(),
        content: "added relay body".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };
    set_nojs_test_accepted(Some(vec!["wss://hint.example".to_string()]));
    set_nojs_test_fetch(NojsTestFetch::Capture);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(
        &form,
        &config,
        "",
        Some(&pinned_relays(&["wss://hint.example"])),
    );
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let link = seal_link_parts(&href);
    let event_id = link.event_id.expect("event id");
    assert_eq!(link.query, "r=wss%3A%2F%2Fhint.example", "{href}");
    assert!(is_seal_key(&link.key), "{href}");
    let locator = link.locator;
    let query = link.query;

    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::sealed_view, super::sealed_locator_view]),
    )
    .unwrap();

    let bare = client.get("/s/not-a-locator").dispatch();
    assert_eq!(bare.status(), Status::NotFound);

    let js = client
        .get(format!("/s/{locator}/{event_id}?{query}"))
        .dispatch();
    assert_eq!(js.status(), Status::Ok);
    let js_body = js.into_string().unwrap();
    assert!(js_body.contains("id=\"sealed-event\""));
    assert!(!js_body.contains("Added relay title"));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://hint.example".to_string()]
    );
}

#[test]
fn nojs_publish_failure_keeps_the_draft() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = true;
    config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
    let form = NewPost {
        title: "Hello <World>".to_string(),
        content: "  # ici  ".to_string(),
        alias: "Ada".to_string(),
        csrf_token: "not-a-token".to_string(),
    };

    let expired = nojs_publish_failure_page(&config, &form, "csrf_token_invalid", None, false).0;
    assert!(expired.contains("The publish form expired. Try again."));
    assert!(expired.contains("Hello &lt;World&gt;"));
    assert!(expired.contains("  # ici  "));
    assert!(expired.contains("value=\"Ada\""));
    assert!(expired.contains("action=\"/nojs/relays\""));
    assert!(!expired.contains("<input type=\"hidden\" name=\"relay\""));
    assert!(!expired.contains("<li data-relay=\"wss://damus.io\">"));
    assert!(!relay_url_input_after(&expired, "sidebar-relays").contains("value="));
    assert!(!expired.contains("<script src"));
    assert!(!expired.contains("not-a-token"));

    let mut blank = form.clone();
    blank.content = " \n ".to_string();
    let empty = nojs_publish_failure_page(&config, &blank, "content_required", None, false).0;
    assert!(empty.contains("Write something before publishing."));
    assert!(empty.contains("Hello &lt;World&gt;"));
    assert!(empty.contains(" \n "));
    assert!(empty.contains("No relays."));
    assert_eq!(
        form_error_text(&empty),
        "Write something before publishing."
    );
    assert_eq!(relay_field_error(&empty), "");

    let known = nojs_publish_failure_page(&config, &form, "nostr_publish_failed", None, false).0;
    assert!(known.contains("Publishing failed. Try again."));
    assert!(!relay_url_input_after(&known, "sidebar-relays").contains("value="));
    assert!(!known.contains("<li data-relay=\"wss://relay.primal.net\">"));
    assert!(!known.contains("<input type=\"hidden\" name=\"relay\""));

    assert_eq!(
        nojs_error_code("/nojs?error=content_required"),
        Some("content_required")
    );
    assert!(nojs_error_code("/nojs/s/abc?k=secret").is_none());
    assert!(nojs_error_code("/nojs?error=").is_none());
    assert!(nojs_error_code("/?error=title_required").is_none());

    let too_long = nojs_publish_failure_page(&config, &form, "content_too_long", None, false).0;
    assert!(too_long.contains("The post is too long."));
    assert!(too_long.contains("Hello &lt;World&gt;"));
    assert!(too_long.contains("  # ici  "));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_note_at_the_plaintext_cap_seals_and_an_over_cap_payload_keeps_the_error() {
    let _holds = HoldDir::new();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
    config.limits.title_max_length = 5;
    config.limits.alias_max_length = 3;
    config.limits.content_max_length = 48;
    let form = NewPost {
        title: "Title".to_string(),
        content: "\u{1}".repeat(config.limits.content_max_length),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };
    set_nojs_test_accepted(Some(vec!["wss://relay.primal.net".to_string()]));
    let relays = pinned_relays(&["wss://relay.primal.net"]);
    let published = publish_nojs_note_with_cookie(&form, &config, Some(&relays)).unwrap();
    let cap = crate::seal::sealed_payload_max(
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    );
    assert!(published.payload.len() > form.content.len());
    assert!(published.payload.len() <= cap);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&relays));
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    set_nojs_test_accepted(None);
    let link = seal_link_parts(&href);
    assert_eq!(link.locator.len(), nostorgraph_nip44::LOCATOR_LEN, "{href}");
    assert!(link.event_id.is_some(), "{href}");
    assert!(href.contains('#'), "{href}");
    assert!(!href.contains("error="));

    config.limits.title_max_length = 0;
    config.limits.alias_max_length = 0;
    config.limits.content_max_length = 4;
    let over = NewPost {
        title: "Hello <World>".to_string(),
        content: "b".repeat(64),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };
    match publish_nojs_note_with_cookie(&over, &config, Some(&relays)) {
        Err("content_too_long") => {}
        Err(error) => panic!("{error}"),
        Ok(_) => panic!("expected content_too_long"),
    }
    let page = nojs_publish_failure_page(&config, &over, "content_too_long", None, false).0;
    assert!(page.contains("The post is too long."));
    assert!(page.contains("Hello &lt;World&gt;"));
    assert!(page.contains(&"b".repeat(64)));

    let mut wide = crate::config::Config::default();
    wide.security.csrf_protection_enabled = false;
    wide.limits.content_max_length = 256_000;
    let controls = NewPost {
        title: "T".to_string(),
        content: "\u{1}".repeat(109_215),
        alias: String::new(),
        csrf_token: String::new(),
    };
    assert!(controls.content.len() <= wide.limits.content_max_length);
    let href = create_location_with_cookie(&controls, &wide, "", Some(&relays));
    assert_eq!(href, "/nojs?error=content_too_long");
    let kept = nojs_publish_failure_page(&wide, &controls, "content_too_long", None, false).0;
    assert!(kept.contains("The post is too long."));
    assert!(kept.contains("value=\"T\"") || kept.contains(">T</textarea>"));
    assert!(kept.contains(&controls.content));
}

fn assert_nojs_error(form: &NewPost, config: &crate::config::Config, error: &str) {
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(form, config, "");
    assert_eq!(href, format!("/nojs?error={error}"));
    assert!(href.starts_with("/nojs?error="), "{href}");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
fn nostr_publish_keeps_public_relays_and_drops_private_ones() {
    let note = crate::nostr::sign_v2_note("0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.damus.io".to_string()];
    let body = NostrPublishBody {
        event,
        relays: vec![
            "wss://pin.example".to_string(),
            "wss://127.0.0.1".to_string(),
            "wss://dealt.example".to_string(),
        ],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    let saved = vec![
        "wss://pin.example".to_string(),
        "wss://dealt.example".to_string(),
    ];
    let (parsed, relays, locator) = note_and_relays_for_publish(&body, &config, "").unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(relays, saved);
    assert_eq!(locator, "0123456789abcdef0123");

    config.security.csrf_protection_enabled = true;
    assert!(note_and_relays_for_publish(&body, &config, "").is_none());
    let token = crate::csrf::generate_csrf_token_with_timestamp();
    let mut paired = body;
    paired.csrf_token = token.clone();
    assert!(note_and_relays_for_publish(&paired, &config, "").is_none());
    assert!(note_and_relays_for_publish(
        &paired,
        &config,
        &crate::csrf::generate_csrf_token_with_timestamp(),
    )
    .is_none());
    let (parsed, relays, locator) = note_and_relays_for_publish(&paired, &config, &token).unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(relays, saved);
    assert_eq!(locator, "0123456789abcdef0123");
}

#[test]
fn nostr_publish_refuses_a_clear_long_form_note() {
    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let note = crate::nostr::signed_event(
        &keypair,
        1_700_000_000,
        30023,
        &[
            vec!["d".to_string(), "0123456789abcdef0123".to_string()],
            vec!["title".to_string(), "Hello".to_string()],
            vec!["author".to_string(), "Ada".to_string()],
        ],
        "body",
    );
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.damus.io".to_string()];
    let body = NostrPublishBody {
        event,
        relays: vec!["wss://127.0.0.1".to_string()],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    assert!(note_and_relays_for_publish(&body, &config, "").is_none());
}

#[test]
fn nostr_publish_accepts_a_sealed_note_that_passes_the_stamp() {
    let note = crate::nostr::sign_v2_note("0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.damus.io".to_string()];
    let body = NostrPublishBody {
        event,
        relays: vec![
            "wss://pin.example".to_string(),
            "wss://127.0.0.1".to_string(),
        ],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    let (parsed, relays, locator) = note_and_relays_for_publish(&body, &config, "").unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(relays, vec!["wss://pin.example".to_string()]);
    assert_eq!(locator, "0123456789abcdef0123");
    assert!(parsed.event_json.contains("\"kind\":30323"));
    assert!(!parsed.event_json.contains("\"title\""));

    config.limits.content_max_length = 4;
    let max = crate::seal::sealed_payload_max(
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    );
    let long =
        crate::nostr::sign_sealed_note(&"x".repeat(max + 1), "0123456789abcdef0123", 1_700_000_000);
    let long_event: serde_json::Value = serde_json::from_str(&long.event_json).unwrap();
    let long_body = NostrPublishBody {
        event: long_event,
        relays: vec!["wss://127.0.0.1".to_string()],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    assert!(note_and_relays_for_publish(&long_body, &config, "").is_none());
    assert!(note_and_relays_for_publish(&body, &config, "").is_some());
}

#[test]
fn publish_result_names_the_locator_and_accepted_relays() {
    let accepted = vec!["wss://relay.damus.io".to_string()];
    let result = publish_result("0123456789abcdef0123".to_string(), accepted.clone());
    assert_eq!(result.locator, "0123456789abcdef0123");
    assert_eq!(result.accepted, accepted);
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"locator\""));
    assert!(json.contains("\"accepted\""));
    assert!(!json.contains("/s/"));
    assert!(!json.contains('#'));
    assert!(!json.contains("secret"));
    assert!(!json.contains("\"key\""));
    assert!(!json.contains("nevent"));

    assert!(publish_result(String::new(), accepted).locator.is_empty());
    let failed = publish_result("0123456789abcdef0123".to_string(), Vec::new());
    assert!(failed.locator.is_empty());
    assert!(failed.accepted.is_empty());
    let failed_json = serde_json::to_string(&failed).unwrap();
    assert!(!failed_json.contains("0123456789abcdef0123"));

    let extra_only = vec!["wss://relay.primal.net".to_string()];
    let extra = publish_result("0123456789abcdef0123".to_string(), extra_only.clone());
    assert_eq!(extra.locator, "0123456789abcdef0123");
    assert_eq!(extra.accepted, extra_only);
    let both = vec![
        "wss://relay.primal.net".to_string(),
        "wss://relay.damus.io".to_string(),
    ];
    let landed = publish_result("0123456789abcdef0123".to_string(), both.clone());
    assert_eq!(landed.locator, "0123456789abcdef0123");
    assert_eq!(landed.accepted, both);
    let private = publish_result(
        "0123456789abcdef0123".to_string(),
        vec!["wss://127.0.0.1".to_string()],
    );
    assert!(private.locator.is_empty());
    let cased = publish_result(
        "0123456789abcdef0123".to_string(),
        vec!["wss://Relay.Primal.net/".to_string()],
    );
    assert_eq!(cased.locator, "0123456789abcdef0123");
}

#[test]
fn the_long_share_link_names_every_accepted_relay() {
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let key = "seal-key";
    let accepted = vec![
        "wss://pin.example".to_string(),
        "wss://Relay.Extra.example/".to_string(),
        "wss://third.example".to_string(),
        "wss://fourth.example".to_string(),
    ];
    assert_eq!(
        long_seal_href(locator, &event_id, key, &accepted),
        format!(
            "/{locator}/{event_id}?r=wss%3A%2F%2Fpin.example&r=wss%3A%2F%2Frelay.extra.example&r=wss%3A%2F%2Fthird.example&r=wss%3A%2F%2Ffourth.example#{key}"
        )
    );
    assert_eq!(short_seal_href(locator, key), format!("/{locator}#{key}"));
    let mut many = vec!["wss://127.0.0.1".to_string()];
    for index in 0..8 {
        many.push(format!("wss://hint{index}.example"));
    }
    let capped = long_seal_href(locator, &event_id, key, &many);
    assert!(capped.contains("r=wss%3A%2F%2Fhint0.example"));
    assert!(capped.contains("r=wss%3A%2F%2Fhint5.example"));
    assert!(!capped.contains("hint6.example"));
    assert!(!capped.contains("127.0.0.1"));

    let result = publish_result(locator.to_string(), accepted.clone());
    assert_eq!(result.locator, locator);
    assert_eq!(result.accepted, accepted);
    assert_eq!(
        result.hints,
        vec![
            "wss://pin.example".to_string(),
            "wss://relay.extra.example".to_string(),
            "wss://third.example".to_string(),
            "wss://fourth.example".to_string(),
        ]
    );

    let configured = vec!["wss://config.example".to_string()];

    let mut config = crate::config::Config::default();
    config.nostr.relays = configured;
    let engine = crate::template::TemplateEngine::new("templates");
    for nojs in [false, true] {
        let page = home_context(&config, nojs, None, None);
        assert_eq!(page.get("nostr_relays").unwrap(), "[]");
        let html = engine.render("home", &page).unwrap();
        assert!(html.contains("data-relays=\"[]\""));
        assert!(!html.contains("config.example"), "{html}");
    }

    let home = include_str!("../templates/home.js");
    let key_at = home.find("function relayKey(url)").expect("relay key");
    let key_end = home[key_at..]
        .find("\n            globalThis.paintSavedRelayLists")
        .map(|offset| key_at + offset)
        .expect("saved lists follow the relay key");
    let href_at = home.find("function sealHref(").expect("share link");
    let href_end = home[href_at..]
        .find("\n            async function publishOnNostr")
        .map(|offset| href_at + offset)
        .expect("publish follows the share link");
    let hints = serde_json::to_string(&result.hints).unwrap();
    let accepted_json = serde_json::to_string(&result.accepted).unwrap();
    let mut program = String::from(
        r#"
function publicRelayUrl(relay) {
    return typeof relay === "string" && relay.startsWith("wss://");
}
"#,
    );
    program.push_str(&home[key_at..key_end]);
    program.push_str(&home[href_at..href_end]);
    let slug_at = home.find("function shareTitleSlug(").expect("title slug");
    let slug_end = home[slug_at..]
        .find("\n            function showShareLinks(")
        .map(|offset| slug_at + offset)
        .expect("share links follow the title slug");
    program.push_str(&home[slug_at..slug_end]);
    let blocked = {
        let mut names = vec![
            "about".to_string(),
            "api".to_string(),
            "legal".to_string(),
            "markup".to_string(),
            "nojs".to_string(),
            "relays".to_string(),
            "tor-circuits".to_string(),
            "0123456789abcdef0123".to_string(),
        ];
        for path in [
            EDITOR_MODE_JS_PATH,
            HOME_CSS_PATH,
            HOME_JS_PATH,
            HOME_NOSCRIPT_CSS_PATH,
            LOGO_PNG_PATH,
            NOTICE_CSS_PATH,
            NOSTR_JS_PATH,
            POST_CSS_PATH,
            POST_JS_PATH,
            POST_NOSCRIPT_CSS_PATH,
            RELAYS_JS_PATH,
            SECP256K1_JS_PATH,
            THEME_JS_PATH,
            WRITEMARK_CSS_PATH,
            WRITEMARK_JS_PATH,
            WRITEMARK_LIVE_JS_PATH,
        ] {
            names.push(path.strip_prefix('/').expect(path).to_string());
        }
        names.push("robots.txt".to_string());
        serde_json::to_string(&names).unwrap()
    };
    program.push_str(&format!(
        r##"
const href = longSealHref({{
    locator: "0123456789abcdef0123",
    event: {{ id: "ab".repeat(32) }},
    key: "seal-key",
    accepted: {accepted_json},
    hints: {hints},
}});
const short = shortSealHref({{
    locator: "0123456789abcdef0123",
    key: "seal-key",
    hints: {hints},
}});
const expected =
    "/0123456789abcdef0123/" +
    "ab".repeat(32) +
    "?r=" +
    encodeURIComponent("wss://pin.example") +
    "&r=" +
    encodeURIComponent("wss://relay.extra.example") +
    "&r=" +
    encodeURIComponent("wss://third.example") +
    "&r=" +
    encodeURIComponent("wss://fourth.example") +
    "#seal-key";
if (href !== expected) {{
    throw new Error(href);
}}
if (short !== "/0123456789abcdef0123#seal-key") {{
    throw new Error(short);
}}
if (typeof TextEncoder === "undefined") {{
    function TextEncoder() {{}}
    TextEncoder.prototype.encode = function (text) {{
        return {{ length: unescape(encodeURIComponent(text)).length }};
    }};
}}
if (typeof URL === "undefined") {{
    function URL(path, origin) {{
        this.href = String(origin).replace(/\/$/, "") + path;
    }}
}}
const blocked = {blocked};
for (const name of blocked) {{
    const got = shareTitleSlug(name);
    if (got !== name + "-2") {{
        throw new Error(name + " -> " + got);
    }}
}}
if (shareTitleSlug("About") !== "About") {{
    throw new Error(shareTitleSlug("About"));
}}
if (shareTitleSlug("0123456789ABCDEF0123") !== "0123456789ABCDEF0123") {{
    throw new Error(shareTitleSlug("0123456789ABCDEF0123"));
}}
if (shareTitleSlug("Hello") !== "Hello") {{
    throw new Error(shareTitleSlug("Hello"));
}}
var location = {{ origin: "https://notes.example" }};
const titled = titleShareHref({{ key: "seal-key" }}, "about-2");
if (titled !== "https://notes.example/about-2#seal-key") {{
    throw new Error(titled);
}}
"##
    ));
    run_javascript("nostorgraph-share-link", &program);

    let nostr = include_str!("../templates/nostr.js");
    let returned = nostr
        .find("const hints = Array.isArray(result.hints)")
        .expect("publish hints");
    let block_end = nostr[returned..]
        .find("key: bytesToBase64Url(sealed.secret)")
        .map(|offset| returned + offset)
        .unwrap();
    let block = &nostr[returned..block_end];
    assert!(block.contains("hints,"));
    assert!(block.contains("!accepted.length"));
    assert!(!block.contains("!hints.length"));
}

fn seal_claim(locator: &str) -> String {
    super::seal_row::claim_hex(&[9u8; 32], locator)
}

fn lock_for(locator: &str) {
    assert!(matches!(
        super::seal_row::lock_claim(locator, &seal_claim(locator)),
        super::seal_row::LockPut::Created | super::seal_row::LockPut::Same
    ));
}

#[test]
#[serial(nojs_sealed_fetch, publish_script)]
fn copying_the_title_link_saves_the_slug_and_the_open_stays_there() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let first = "0123456789abcdef0123";
    let second = "fedcba9876543210abcd";
    let third = "00112233445566778899";
    let note = crate::nostr::sign_sealed_note("kept", first, 1_700_000_000);
    let event_id = note.id_hex();
    let other_id = "ab".repeat(32);
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![
            super::remember_title_link,
            super::remember_short_link,
            super::title_view
        ],
    ))
    .unwrap();
    let post = |title: &str, locator: &str, id: &str| {
        client
            .post("/title-link")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"title":"{title}","locator":"{locator}","event_id":"{id}","relays":["wss://saved.example"],"claim":"{}","csrf_token":""}}"#,
                seal_claim(locator)
            ))
            .dispatch()
    };
    let slug_of = |response: rocket::local::blocking::LocalResponse<'_>| {
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(value.get("key").is_none());
        assert!(value.get("secret").is_none());
        value["slug"].as_str().unwrap().to_string()
    };

    let rejected = client
        .post("/title-link")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"title":"Hello","locator":"{first}","event_id":"{event_id}","relays":["wss://saved.example"],"csrf_token":"","key":"secret"}}"#
        ))
        .dispatch();
    assert_eq!(rejected.status(), Status::UnprocessableEntity);
    assert!(std::fs::read_dir(dir.path().join("title-slugs")).is_err());
    assert!(std::fs::read_dir(dir.path().join("seal-rows")).is_err());

    let blank = post("   ", first, &event_id);
    assert_eq!(blank.status(), Status::BadRequest);
    assert!(std::fs::read_dir(dir.path().join("title-slugs")).is_err());

    lock_for(second);
    lock_for(first);
    lock_for(third);
    assert_eq!(slug_of(post("Hello-2", second, &other_id)), "Hello-2");
    assert_eq!(slug_of(post("Hello", first, &event_id)), "Hello");
    assert_eq!(slug_of(post("Hello", third, &other_id)), "Hello-3");
    assert_eq!(slug_of(post("Hello", first, &event_id)), "Hello");
    assert_eq!(
        std::fs::read_dir(dir.path().join("title-slugs"))
            .unwrap()
            .count(),
        3
    );
    let fourth = "aabbccddeeff00112233";
    lock_for(fourth);
    assert_eq!(
        slug_of(post("Hello/World", fourth, &other_id)),
        "Hello-World"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("title-slugs").join("Hello")).unwrap(),
        first
    );
    let row = super::seal_row::read(first).expect("locator row");
    assert_eq!(row.event_id, event_id);
    assert_eq!(row.relays, vec!["wss://saved.example".to_string()]);
    let retarget = client
        .post("/title-link")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"title":"Hello","locator":"{first}","event_id":"{other_id}","relays":["wss://other.example"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(first)
        ))
        .dispatch();
    assert_eq!(retarget.status(), Status::BadRequest);
    let retarget_short = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{first}","event_id":"{other_id}","relays":["wss://other.example"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(first)
        ))
        .dispatch();
    assert_eq!(retarget_short.status(), Status::BadRequest);
    let kept = super::seal_row::read(first).expect("locator row");
    assert_eq!(kept.event_id, event_id);
    assert_eq!(kept.relays, vec!["wss://saved.example".to_string()]);

    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://saved.example", &note.event_json);
    let page = client.get("/t/Hello").dispatch();
    assert_eq!(page.status(), Status::Ok);
    let html = page.into_string().unwrap();
    assert!(html.contains(&format!(r#"<div id="sealed-locator" hidden>{first}</div>"#)));
    assert!(html.contains("id=\"sealed-event\""));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://saved.example".to_string()]
    );
    let missing = client.get("/t/Missing").dispatch();
    assert_eq!(missing.status(), Status::NotFound);

    let home = include_str!("../templates/home.js");
    let copy_at = home
        .find("async function copyTitleLink")
        .expect("title copy");
    let copy_end = home[copy_at..]
        .find("\n            async function publishOnNostr")
        .map(|offset| copy_at + offset)
        .expect("open follows title copy");
    let copy_body = &home[copy_at..copy_end];
    assert!(copy_body.contains("\"/title-link\""));
    assert!(copy_body.contains("title: title"));
    assert!(copy_body.contains("event_id: eventId"));
    assert!(copy_body.contains("if (!response || !response.ok)"));
    assert!(copy_body.contains("ok: false"));
    let posted = copy_body
        .split("JSON.stringify")
        .nth(1)
        .expect("title body");
    let posted = &posted[..posted.find("});").expect("end of title body")];
    assert!(posted.contains("claim: result.claim"));
    assert!(!posted.contains("result.key"));
    assert!(!posted.contains("secret"));
    assert!(copy_body.contains("location.origin"));
    assert!(copy_body.contains("new URL("));

    let nostr = include_str!("../templates/nostr.js");
    let publish_at = nostr
        .find("async function publishPublicNote(")
        .expect("publish");
    let publish_end = nostr[publish_at..]
        .find("\nfunction xOnlyPubkey")
        .map(|offset| publish_at + offset)
        .expect("publish ends");
    let publish = &nostr[publish_at..publish_end];
    let returned = publish.find("return {").expect("publish result");
    let returned_end = publish[returned..]
        .find("};")
        .map(|offset| returned + offset)
        .expect("publish result ends");
    assert!(publish[returned..returned_end].contains("title: String(title ?? \"\")"));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_multibyte_title_opens_and_a_failed_save_leaves_nothing() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;
    let encode = |slug: &str| {
        let mut encoded = String::new();
        for byte in slug.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
                encoded.push(*byte as char);
            } else {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
        encoded
    };
    let names = |path: &std::path::Path| {
        let Ok(dir) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        let mut found = dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        found.sort();
        found
    };

    let blocked = tempfile::tempdir().unwrap();
    let _blocked_rows = super::seal_row::SealRowDirGuard::set(blocked.path());
    std::fs::write(blocked.path().join("seal-rows"), b"blocked").unwrap();
    let blocked_locator = "c4c4c4c4c4c4c4c4c4c4";
    let blocked_note = crate::nostr::sign_sealed_note("kept", blocked_locator, 1_700_000_000);
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::remember_title_link, super::title_view]),
    )
    .unwrap();
    let post = |title: &str, locator: &str, id: &str, relay: &str| {
        client
            .post("/title-link")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"title":"{title}","locator":"{locator}","event_id":"{id}","relays":["{relay}"],"claim":"{}","csrf_token":""}}"#,
                seal_claim(locator)
            ))
            .dispatch()
    };
    let slug_of = |response: rocket::local::blocking::LocalResponse<'_>| {
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        value["slug"].as_str().unwrap().to_string()
    };
    let failed = post(
        "Hello",
        blocked_locator,
        &blocked_note.id_hex(),
        "wss://saved.example",
    );
    assert_eq!(failed.status(), Status::BadRequest);
    assert!(names(&blocked.path().join("title-slugs")).is_empty());
    assert!(names(&blocked.path().join("title-for")).is_empty());
    assert!(super::seal_row::read(blocked_locator).is_none());

    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let first = "c1c1c1c1c1c1c1c1c1c1";
    let second = "c2c2c2c2c2c2c2c2c2c2";
    let third = "c3c3c3c3c3c3c3c3c3c3";
    let short_title = "字".repeat(49);
    let long_title = "字".repeat(128);
    let first_note = crate::nostr::sign_sealed_note("kept", first, 1_700_000_000);
    let second_note = crate::nostr::sign_sealed_note("kept", second, 1_700_000_000);
    let third_note = crate::nostr::sign_sealed_note("kept", third, 1_700_000_000);
    let private = post("Hello", first, &first_note.id_hex(), "wss://127.0.0.1");
    assert_eq!(private.status(), Status::BadRequest);
    assert!(names(&dir.path().join("title-slugs")).is_empty());
    assert!(names(&dir.path().join("seal-rows")).is_empty());

    lock_for(first);
    let slug = slug_of(post(
        &short_title,
        first,
        &first_note.id_hex(),
        "wss://saved.example",
    ));
    assert!(slug.len() > 144, "{}", slug.len());
    assert_eq!(
        slug_of(post(
            &short_title,
            first,
            &first_note.id_hex(),
            "wss://saved.example",
        )),
        slug
    );
    lock_for(second);
    let collided = slug_of(post(
        &short_title,
        second,
        &second_note.id_hex(),
        "wss://other.example",
    ));
    assert_eq!(collided, format!("{slug}-2"));
    lock_for(third);
    let long_slug = slug_of(post(
        &long_title,
        third,
        &third_note.id_hex(),
        "wss://third.example",
    ));
    assert!(long_slug.chars().count() < 128);
    assert!(long_slug.len() + 5 <= 255);
    assert_eq!(
        slug_of(post(
            &long_title,
            third,
            &third_note.id_hex(),
            "wss://third.example",
        )),
        long_slug
    );

    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://saved.example", &first_note.event_json);
    answers.answer("wss://other.example", &second_note.event_json);
    answers.answer("wss://third.example", &third_note.event_json);
    for (slug, locator) in [(&slug, first), (&collided, second), (&long_slug, third)] {
        let page = client.get(format!("/t/{}", encode(slug))).dispatch();
        assert_eq!(page.status(), Status::Ok, "{slug}");
        let html = page.into_string().unwrap();
        assert!(
            html.contains(&format!(
                r#"<div id="sealed-locator" hidden>{locator}</div>"#
            )),
            "{slug}"
        );
    }
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_root_address_opens_the_note_and_a_site_page_stays() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;

    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let long_locator = "a1a1a1a1a1a1a1a1a1a1";
    let short_locator = "b2b2b2b2b2b2b2b2b2b2";
    let title_locator = "c3c3c3c3c3c3c3c3c3c3";
    let long_note = crate::nostr::sign_sealed_note("kept", long_locator, 1_700_000_000);
    let short_note = crate::nostr::sign_sealed_note("kept", short_locator, 1_700_000_000);
    let title_note = crate::nostr::sign_sealed_note("kept", title_locator, 1_700_000_000);
    let long_id = long_note.id_hex();
    lock_for(short_locator);
    super::seal_row::remember(
        short_locator,
        &short_note.id_hex(),
        &["wss://saved.example".to_string()],
        &seal_claim(short_locator),
    )
    .expect("short row");
    lock_for(title_locator);
    assert_eq!(
        super::title_link::save(
            "Hello",
            title_locator,
            &title_note.id_hex(),
            &["wss://title.example".to_string()],
            &seal_claim(title_locator),
        )
        .expect("title slug"),
        "Hello"
    );

    let mut config = crate::config::Config::default();
    config.nostr.timeout_secs = 1;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![
            super::index,
            super::nojs_index,
            super::about_page,
            super::legal_page,
            super::api_page,
            super::markup_page,
            super::nojs_static_page,
            super::relays_page,
            super::tor_circuits,
            super::robots_txt,
            super::logo_png,
            super::home_js,
            super::root_link_view,
            super::root_sealed_view,
            super::sealed_locator_view,
            super::sealed_view,
            super::title_view,
        ],
    ))
    .expect("root routes mount beside the site pages");

    for path in [
        "/",
        "/nojs",
        "/about",
        "/legal",
        "/api",
        "/markup",
        "/nojs/about",
        "/relays",
        "/tor-circuits",
        "/robots.txt",
        "/logo.png",
        "/home.js",
    ] {
        let page = client.get(path).dispatch();
        assert_eq!(page.status(), Status::Ok, "{path}");
        if path == "/about" {
            let html = page.into_string().unwrap();
            assert!(html.contains("A Minimalist Publishing Platform"));
            assert!(!html.contains("id=\"sealed-event\""));
        }
    }

    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://hint.example", &long_note.event_json);
    answers.answer("wss://saved.example", &short_note.event_json);
    answers.answer("wss://title.example", &title_note.event_json);

    let long = client
        .get(format!(
            "/{long_locator}/{long_id}?r=wss%3A%2F%2Fhint.example"
        ))
        .dispatch();
    assert_eq!(long.status(), Status::Ok);
    let long_html = long.into_string().unwrap();
    assert!(long_html.contains("id=\"sealed-event\""));
    assert!(!long_html.contains("id=\"sealed-locator\""));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://hint.example".to_string()]
    );
    let long_again = client
        .get(format!(
            "/s/{long_locator}/{long_id}?r=wss%3A%2F%2Fhint.example"
        ))
        .dispatch();
    assert_eq!(long_again.status(), Status::Ok);

    let short = client
        .get(format!("/{short_locator}?r=wss%3A%2F%2Fother.example"))
        .dispatch();
    assert_eq!(short.status(), Status::Ok);
    let short_html = short.into_string().unwrap();
    assert!(short_html.contains("id=\"sealed-event\""));
    assert!(!short_html.contains("id=\"sealed-locator\""));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://saved.example".to_string()]
    );
    let short_again = client.get(format!("/s/{short_locator}")).dispatch();
    assert_eq!(short_again.status(), Status::Ok);

    let title = client.get("/Hello?r=wss%3A%2F%2Fother.example").dispatch();
    assert_eq!(title.status(), Status::Ok);
    let title_html = title.into_string().unwrap();
    assert!(title_html.contains(&format!(
        r#"<div id="sealed-locator" hidden>{title_locator}</div>"#
    )));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://title.example".to_string()]
    );
    let title_again = client.get("/t/Hello").dispatch();
    assert_eq!(title_again.status(), Status::Ok);

    let missing_title = client.get("/Missing").dispatch();
    assert_eq!(missing_title.status(), Status::NotFound);
    let missing_short = client.get("/fedcba9876543210abcd").dispatch();
    assert_eq!(missing_short.status(), Status::NotFound);
    let upper = client.get("/0123456789ABCDEF0123").dispatch();
    assert_eq!(upper.status(), Status::NotFound);
    let bad_long = client.get(format!("/{long_locator}/abcd")).dispatch();
    assert_eq!(bad_long.status(), Status::NotFound);
    let two = client.get("/about/extra").dispatch();
    assert_eq!(two.status(), Status::NotFound);
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_reserved_title_takes_the_next_slug_and_an_old_one_stays() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;

    for name in [
        "about",
        "api",
        "legal",
        "markup",
        "nojs",
        "relays",
        "tor-circuits",
        "0123456789abcdef0123",
    ] {
        assert!(super::title_link::slug_blocked(name), "{name}");
    }
    for path in [
        HOME_CSS_PATH,
        HOME_JS_PATH,
        NOSTR_JS_PATH,
        SECP256K1_JS_PATH,
        POST_CSS_PATH,
        POST_JS_PATH,
        POST_NOSCRIPT_CSS_PATH,
        HOME_NOSCRIPT_CSS_PATH,
        WRITEMARK_JS_PATH,
        WRITEMARK_LIVE_JS_PATH,
        WRITEMARK_CSS_PATH,
        NOTICE_CSS_PATH,
        THEME_JS_PATH,
        EDITOR_MODE_JS_PATH,
        RELAYS_JS_PATH,
        LOGO_PNG_PATH,
    ] {
        let name = path.strip_prefix('/').expect(path);
        assert!(!name.contains('/'), "{path}");
        assert!(super::title_link::slug_blocked(name), "{path}");
    }
    assert!(super::title_link::slug_blocked("robots.txt"));
    for path in [
        PAGE_JS_PATH,
        PAGE_WASM_PATH,
        PDF_JS_PATH,
        PDF_WASM_PATH,
        PDF_LICENSE_PATH,
    ] {
        let name = path.strip_prefix('/').expect(path);
        assert!(name.contains('/'), "{path}");
        assert!(!super::title_link::slug_blocked(name), "{path}");
    }
    for name in [
        "About",
        "s",
        "t",
        "page",
        "pdf",
        "0123456789abcdef012",
        "0123456789ABCDEF0123",
    ] {
        assert!(!super::title_link::slug_blocked(name), "{name}");
    }

    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let old = "aaaaaaaaaaaaaaaaaaaa";
    let fresh = "bbbbbbbbbbbbbbbbbbbb";
    let other = "cccccccccccccccccccc";
    let hex = "dddddddddddddddddddd";
    let hex_title = "0123456789abcdef0123";
    let old_note = crate::nostr::sign_sealed_note("kept", old, 1_700_000_000);
    let fresh_note = crate::nostr::sign_sealed_note("kept", fresh, 1_700_000_000);
    let hex_note = crate::nostr::sign_sealed_note("kept", hex, 1_700_000_000);
    let event_id = "ab".repeat(32);
    let save = |title: &str, locator: &str, id: &str, relay: &str| {
        lock_for(locator);
        super::title_link::save(
            title,
            locator,
            id,
            &[relay.to_string()],
            &seal_claim(locator),
        )
        .unwrap_or_else(|_| panic!("{title}"))
    };
    let slug_file = |name: &str| dir.path().join("title-slugs").join(name);

    assert_eq!(
        save("About", other, &event_id, "wss://saved.example"),
        "About"
    );
    std::fs::remove_file(slug_file("About")).unwrap();
    for entry in std::fs::read_dir(dir.path().join("title-for")).unwrap() {
        let path = entry.unwrap().path();
        if std::fs::read_to_string(&path).ok().as_deref() == Some("About") {
            std::fs::remove_file(path).unwrap();
        }
    }

    assert_eq!(
        save("about", old, &old_note.id_hex(), "wss://old.example"),
        "about-2"
    );
    assert!(!slug_file("about").exists());
    std::fs::rename(slug_file("about-2"), slug_file("about")).unwrap();
    let mut planted = false;
    for entry in std::fs::read_dir(dir.path().join("title-for")).unwrap() {
        let path = entry.unwrap().path();
        if std::fs::read_to_string(&path).ok().as_deref() == Some("about-2") {
            std::fs::write(&path, "about").unwrap();
            planted = true;
        }
    }
    assert!(planted);
    assert_eq!(
        save("about", old, &old_note.id_hex(), "wss://old.example"),
        "about"
    );
    assert_eq!(std::fs::read_to_string(slug_file("about")).unwrap(), old);
    assert!(!slug_file("about-2").exists());

    assert_eq!(
        save(hex_title, hex, &hex_note.id_hex(), "wss://hex.example",),
        format!("{hex_title}-2")
    );
    assert!(!slug_file(hex_title).exists());
    assert_eq!(
        save(
            "0123456789ABCDEF0123",
            "eeeeeeeeeeeeeeeeeeee",
            &event_id,
            "wss://saved.example",
        ),
        "0123456789ABCDEF0123"
    );
    assert_eq!(
        save(
            "logo.png",
            "ffffffffffffffffffff",
            &event_id,
            "wss://saved.example",
        ),
        "logo.png-2"
    );
    assert!(!slug_file("logo.png").exists());
    assert_eq!(
        save("about", fresh, &fresh_note.id_hex(), "wss://new.example"),
        "about-2"
    );
    assert_eq!(std::fs::read_to_string(slug_file("about")).unwrap(), old);
    assert_eq!(
        std::fs::read_to_string(slug_file("about-2")).unwrap(),
        fresh
    );

    let mut config = crate::config::Config::default();
    config.nostr.timeout_secs = 1;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![super::about_page, super::root_link_view, super::title_view,],
    ))
    .unwrap();
    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://old.example", &old_note.event_json);
    answers.answer("wss://new.example", &fresh_note.event_json);
    answers.answer("wss://hex.example", &hex_note.event_json);

    let site = client.get("/about").dispatch();
    assert_eq!(site.status(), Status::Ok);
    let site_html = site.into_string().unwrap();
    assert!(site_html.contains("A Minimalist Publishing Platform"));
    assert!(!site_html.contains("id=\"sealed-locator\""));

    let kept = client.get("/t/about").dispatch();
    assert_eq!(kept.status(), Status::Ok);
    assert!(kept
        .into_string()
        .unwrap()
        .contains(&format!(r#"<div id="sealed-locator" hidden>{old}</div>"#)));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://old.example".to_string()]
    );

    let opened = client.get("/about-2").dispatch();
    assert_eq!(opened.status(), Status::Ok);
    assert!(opened
        .into_string()
        .unwrap()
        .contains(&format!(r#"<div id="sealed-locator" hidden>{fresh}</div>"#)));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://new.example".to_string()]
    );
    let opened_old = client.get("/t/about-2").dispatch();
    assert_eq!(opened_old.status(), Status::Ok);

    let bare_hex = client.get(format!("/{hex_title}")).dispatch();
    assert_eq!(bare_hex.status(), Status::NotFound);
    assert!(!bare_hex.into_string().unwrap().contains(hex));
    let hex_page = client.get(format!("/{hex_title}-2")).dispatch();
    assert_eq!(hex_page.status(), Status::Ok);
    assert!(hex_page
        .into_string()
        .unwrap()
        .contains(&format!(r#"<div id="sealed-locator" hidden>{hex}</div>"#)));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://hex.example".to_string()]
    );
    let hex_old = client.get(format!("/t/{hex_title}-2")).dispatch();
    assert_eq!(hex_old.status(), Status::Ok);
}

#[test]
#[serial(nojs_sealed_fetch)]
fn copying_the_short_link_saves_a_locator_row_and_the_open_uses_it() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_sealed_note("kept", locator, 1_700_000_000);
    let event_id = note.id_hex();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![super::remember_short_link, super::sealed_locator_view],
    ))
    .unwrap();

    let rejected = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{event_id}","relays":["wss://saved.example"],"csrf_token":"","key":"secret"}}"#
        ))
        .dispatch();
    assert_eq!(rejected.status(), Status::UnprocessableEntity);
    assert!(std::fs::read_dir(dir.path().join("seal-rows")).is_err());

    let private = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{event_id}","relays":["wss://127.0.0.1"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(locator)
        ))
        .dispatch();
    assert_eq!(private.status(), Status::BadRequest);
    assert!(std::fs::read_dir(dir.path().join("seal-rows")).is_err());

    lock_for(locator);
    let saved = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{event_id}","relays":["wss://Saved.Example/","wss://127.0.0.1"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(locator)
        ))
        .dispatch();
    assert_eq!(saved.status(), Status::NoContent);
    let wrong_claim = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{event_id}","relays":["wss://Saved.Example/","wss://127.0.0.1"],"claim":"{}","csrf_token":""}}"#,
            "00".repeat(32)
        ))
        .dispatch();
    assert_eq!(wrong_claim.status(), Status::BadRequest);
    let row = super::seal_row::read(locator).expect("locator row");
    assert_eq!(row.event_id, event_id);
    assert_eq!(row.relays, vec!["wss://saved.example".to_string()]);
    let stored: Vec<_> = std::fs::read_dir(dir.path().join("seal-rows"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(stored.len(), 1);
    let name = stored[0].file_name().into_string().unwrap();
    assert_eq!(name.len(), 64);
    assert_ne!(name, locator);
    let text = std::fs::read_to_string(stored[0].path()).unwrap();
    assert!(!text.contains(locator), "{text}");
    assert!(!text.contains(&event_id), "{text}");
    assert!(!text.contains("saved.example"), "{text}");

    let other_id = "ab".repeat(32);
    let replaced = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{other_id}","relays":["wss://other.example"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(locator)
        ))
        .dispatch();
    assert_eq!(replaced.status(), Status::BadRequest);
    let kept = super::seal_row::read(locator).expect("locator row");
    assert_eq!(kept.event_id, event_id);
    assert_eq!(kept.relays, vec!["wss://saved.example".to_string()]);
    assert_eq!(
        std::fs::read_dir(dir.path().join("seal-rows"))
            .unwrap()
            .count(),
        1
    );

    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://saved.example", &note.event_json);
    let page = client.get(format!("/s/{locator}")).dispatch();
    assert_eq!(page.status(), Status::Ok);
    assert!(page.into_string().unwrap().contains("id=\"sealed-event\""));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://saved.example".to_string()]
    );
    let hinted = client
        .get(format!("/s/{locator}?r=wss%3A%2F%2Fother.example"))
        .dispatch();
    assert_eq!(hinted.status(), Status::Ok);
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://saved.example".to_string()]
    );
    let other = "fedcba9876543210abcd";
    let other_note = crate::nostr::sign_sealed_note("other", other, 1_700_000_000);
    answers.answer("wss://saved.example", &other_note.event_json);
    let unsaved = client
        .get(format!("/s/{other}?r=wss%3A%2F%2Fsaved.example"))
        .dispatch();
    assert_eq!(unsaved.status(), Status::NotFound);
    let missing = client.get("/s/fedcba9876543210abcd").dispatch();
    assert_eq!(missing.status(), Status::NotFound);

    let home = include_str!("../templates/home.js");
    let copy_at = home
        .find("async function copyShortLink")
        .expect("short copy");
    let copy_end = home[copy_at..]
        .find("\n            async function copyTitleLink")
        .map(|offset| copy_at + offset)
        .expect("title copy follows short copy");
    let copy_body = &home[copy_at..copy_end];
    assert!(copy_body.contains("\"/seal-row\""));
    assert!(copy_body.contains("event_id: eventId"));
    assert!(copy_body.contains("if (!response || !response.ok)"));
    assert!(copy_body.contains("ok: false"));
    assert!(copy_body.contains("claim: result.claim"));
    assert!(!copy_body.contains("result.key"));
    assert!(!copy_body.contains("secret"));
    let paint = copy_body.find("paintShareUrl(").expect("short url");
    let copied = copy_body
        .find("writeClipboard(href)")
        .expect("clipboard follows the saved url");
    assert!(paint < copied);
}

#[test]
#[serial(nojs_sealed_fetch)]
fn copying_the_short_link_still_allows_the_title_link() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_fetch(NojsTestFetch::Live);
            NOJS_RECORD_HINTS.store(false, std::sync::atomic::Ordering::Relaxed);
            *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = Vec::new();
        }
    }
    let _clear = ClearFetch;
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_sealed_note("kept", locator, 1_700_000_000);
    let event_id = note.id_hex();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![
            super::remember_short_link,
            super::remember_title_link,
            super::title_view
        ],
    ))
    .unwrap();
    lock_for(locator);
    let saved = client
        .post("/seal-row")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"locator":"{locator}","event_id":"{event_id}","relays":["wss://saved.example"],"claim":"{}","csrf_token":""}}"#,
            seal_claim(locator)
        ))
        .dispatch();
    assert_eq!(saved.status(), Status::NoContent);
    assert_eq!(hold_dir_count(dir.path()), 0);

    let title = |title: &str, id: &str, claim: &str| {
        client
            .post("/title-link")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"title":"{title}","locator":"{locator}","event_id":"{id}","relays":["wss://saved.example"],"claim":"{claim}","csrf_token":""}}"#
            ))
            .dispatch()
    };
    let rejected = title("Hello", &event_id, &"00".repeat(32));
    assert_eq!(rejected.status(), Status::BadRequest);
    let retarget = title("Hello", &"ab".repeat(32), &seal_claim(locator));
    assert_eq!(retarget.status(), Status::BadRequest);
    let blank = title("   ", &event_id, &seal_claim(locator));
    assert_eq!(blank.status(), Status::BadRequest);
    assert!(std::fs::read_dir(dir.path().join("title-slugs")).is_err());

    let created = title("Hello", &event_id, &seal_claim(locator));
    assert_eq!(created.status(), Status::Ok);
    let body = created.into_string().unwrap();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["slug"], "Hello");
    let again = title("Hello", &event_id, &seal_claim(locator));
    assert_eq!(again.status(), Status::Ok);
    let again: serde_json::Value = serde_json::from_str(&again.into_string().unwrap()).unwrap();
    assert_eq!(again["slug"], "Hello");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("title-slugs").join("Hello")).unwrap(),
        locator
    );
    let kept = super::seal_row::read(locator).expect("locator row");
    assert_eq!(kept.event_id, event_id);
    assert_eq!(kept.relays, vec!["wss://saved.example".to_string()]);

    set_nojs_test_fetch(NojsTestFetch::Live);
    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let answers = crate::nostr::RelayAnswer::begin();
    answers.answer("wss://saved.example", &note.event_json);
    let page = client.get("/t/Hello").dispatch();
    assert_eq!(page.status(), Status::Ok);
    let html = page.into_string().unwrap();
    assert!(html.contains(&format!(
        r#"<div id="sealed-locator" hidden>{locator}</div>"#
    )));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://saved.example".to_string()]
    );
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_full_link_directory_refuses_another_row() {
    let _cap = super::seal_row::LinkCapGuard::set(1);
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "d1d1d1d1d1d1d1d1d1d1";
    let other = "d2d2d2d2d2d2d2d2d2d2";
    let event_id = "ab".repeat(32);
    let stored = vec!["wss://saved.example".to_string()];
    lock_for(locator);
    lock_for(other);
    assert!(super::seal_row::remember(locator, &event_id, &stored, &seal_claim(locator)).is_ok());
    assert!(super::seal_row::remember(other, &event_id, &stored, &seal_claim(other)).is_err());
    assert!(super::seal_row::read(other).is_none());
    assert_eq!(
        super::seal_row::read(locator).expect("first row").event_id,
        event_id
    );
}

#[test]
fn the_browser_seal_claim_matches_the_server() {
    let locator = "0123456789abcdef0123";
    let claim = super::seal_row::claim_hex(&[3u8; 32], locator);
    let nostr = include_str!("../templates/nostr.js");
    let sha = nostr.find("async function sha256(").expect("sha256");
    let sha_end = nostr[sha..]
        .find("\nlet bip340Ready")
        .map(|offset| sha + offset)
        .expect("sha256 ends");
    let hex = nostr.find("function bytesToHex(").expect("hex");
    let hex_end = nostr[hex..]
        .find("\nfunction blockedRelayHost")
        .map(|offset| hex + offset)
        .expect("hex ends");
    let mac = nostr.find("async function hmacSha256(").expect("hmac");
    let mac_end = nostr[mac..]
        .find("\nexport async function publishPublicNote")
        .map(|offset| mac + offset)
        .expect("claim ends");
    let mut program = String::new();
    program.push_str(&nostr[sha..sha_end]);
    program.push_str(&nostr[hex..hex_end]);
    program.push_str(&nostr[mac..mac_end]);
    program.push_str(&format!(
        r#"
sealClaim(hexToBytes("{}"), "{locator}").then((got) => {{
    if (got !== "{claim}") {{
        throw new Error(got);
    }}
}}).catch((error) => {{
    console.error(error);
    if (typeof process !== "undefined") process.exit(1);
    throw error;
}});
"#,
        "03".repeat(32)
    ));
    run_javascript("nostorgraph-seal-claim", &program);
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_second_claim_cannot_replace_a_saved_link() {
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "bbbbbbbbbbbbbbbbbbbb";
    let event_id = "cd".repeat(32);
    let claim = super::seal_row::claim_hex(&[4u8; 32], locator);
    let other = super::seal_row::claim_hex(&[5u8; 32], locator);
    let relays = vec!["wss://saved.example".to_string()];
    assert!(matches!(
        super::seal_row::lock_claim(locator, &claim),
        super::seal_row::LockPut::Created
    ));
    assert!(super::seal_row::remember(locator, &event_id, &relays, &claim).is_ok());
    assert!(super::seal_row::remember(locator, &event_id, &relays, &other).is_err());
    let kept = super::seal_row::read(locator).expect("saved row");
    assert_eq!(kept.event_id, event_id);
    assert_eq!(kept.relays, relays);
}

struct HoldDir {
    dir: tempfile::TempDir,
    _guard: super::seal_row::SealRowDirGuard,
}

impl HoldDir {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let _guard = super::seal_row::SealRowDirGuard::set(dir.path());
        Self { dir, _guard }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

fn hold_dir_count(root: &std::path::Path) -> usize {
    let Ok(dir) = std::fs::read_dir(root.join("seal-holds")) else {
        return 0;
    };
    dir.filter_map(|entry| entry.ok())
        .filter(|entry| !entry.file_name().to_string_lossy().ends_with(".writing"))
        .count()
}

#[test]
#[serial(nojs_sealed_fetch, publish_script)]
fn a_refused_publish_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "0123456789abcdef0123";
    let note = crate::nostr::sign_v2_note(locator, 1_700_000_000);
    let event = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let body = NostrPublishBody {
        event,
        relays: vec!["wss://marked.example".to_string()],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    let script = crate::nostr::PublishScript::begin();
    script.refuse("wss://marked.example");
    let result = publish_sealed_note(&body, &config, "").unwrap();
    drop(script);
    assert!(result.locator.is_empty());
    assert!(result.accepted.is_empty());
    assert_eq!(hold_dir_count(dir.path()), 0);
    assert!(super::seal_row::read(locator).is_none());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_short_link_needs_no_publish_hold() {
    let dir = tempfile::tempdir().unwrap();
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let relays = vec!["wss://saved.example".to_string()];
    let claim = super::seal_row::claim_hex(&[4u8; 32], locator);
    assert!(super::seal_row::remember(locator, &event_id, &relays, &claim).is_err());
    assert!(matches!(
        super::seal_row::lock_claim(locator, &claim),
        super::seal_row::LockPut::Created
    ));
    let other = super::seal_row::claim_hex(&[5u8; 32], locator);
    assert!(super::seal_row::remember(locator, &event_id, &relays, &other).is_err());
    assert!(super::seal_row::remember(locator, &event_id, &relays, &claim).is_ok());
    assert_eq!(hold_dir_count(dir.path()), 0);
    let row = super::seal_row::read(locator).expect("saved row");
    assert_eq!(row.event_id, event_id);
    assert_eq!(row.relays, relays);
    assert!(super::seal_row::remember(locator, &event_id, &relays, &other).is_err());
}

#[test]
#[serial(nojs_sealed_fetch, publish_script)]
fn a_refused_publish_marks_the_pool_row_and_leaves_the_pinned_relay() {
    set_shared_nojs_accepted(None);
    let dir = tempfile::tempdir().unwrap();
    let _content = super::relays::RelayContentGuard::set(dir.path());
    let _rows = super::seal_row::SealRowDirGuard::set(dir.path());
    let path = dir.path().join("relay-pool.json");
    let row = |url: &str, refused: bool| crawler::Row {
        url: url.to_string(),
        first_seen: 1,
        next_try: 1,
        failures: 0,
        publish_refused: refused,
        visit: None,
    };
    crawler::save(
        &path,
        &crawler::Pool {
            rows: vec![
                row("wss://pool.example", false),
                row("wss://slow.example", false),
                row("wss://marked.example", true),
            ],
        },
    )
    .unwrap();
    let list = super::relays::RelayCookie {
        dealt: vec![
            "wss://pool.example".to_string(),
            "wss://slow.example".to_string(),
        ],
        pinned: vec!["wss://hand.example".to_string()],
    };
    let cookie = super::relays::write_relay_cookie(&list);
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    let form = NewPost {
        title: "Hello".to_string(),
        content: "Body".to_string(),
        alias: String::new(),
        csrf_token: String::new(),
    };

    let script = crate::nostr::PublishScript::begin();
    script.refuse("wss://pool.example");
    script.refuse("wss://hand.example");
    script.limit("wss://slow.example");
    let published = publish_nojs_note_with_cookie(&form, &config, Some(&cookie)).unwrap();
    assert!(published.accepted.is_empty());
    assert_eq!(hold_dir_count(dir.path()), 0);
    drop(script);

    let pool = crawler::load(&path);
    assert_eq!(pool.rows.len(), 3);
    assert!(
        pool.rows
            .iter()
            .find(|row| row.url == "wss://pool.example")
            .unwrap()
            .publish_refused
    );
    assert!(
        !pool
            .rows
            .iter()
            .find(|row| row.url == "wss://slow.example")
            .unwrap()
            .publish_refused
    );
    assert!(
        pool.rows
            .iter()
            .find(|row| row.url == "wss://marked.example")
            .unwrap()
            .publish_refused
    );
    assert!(!pool.rows.iter().any(|row| row.url == "wss://hand.example"));
    assert_eq!(super::relays::read_relay_cookie(Some(&cookie)), list);

    let note = crate::nostr::sign_v2_note("0123456789abcdef0123", 1_700_000_000);
    let event = serde_json::from_str(&note.event_json).unwrap();
    let body = NostrPublishBody {
        event,
        relays: vec!["wss://marked.example".to_string()],
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    let script = crate::nostr::PublishScript::begin();
    script.accept("wss://marked.example");
    let result = publish_sealed_note(&body, &config, "").unwrap();
    assert_eq!(result.accepted, vec!["wss://marked.example".to_string()]);
    drop(script);

    let pool = crawler::load(&path);
    assert_eq!(pool.rows.len(), 3);
    assert!(
        !pool
            .rows
            .iter()
            .find(|row| row.url == "wss://marked.example")
            .unwrap()
            .publish_refused
    );
    assert!(
        pool.rows
            .iter()
            .find(|row| row.url == "wss://pool.example")
            .unwrap()
            .publish_refused
    );
    assert!(
        !pool
            .rows
            .iter()
            .find(|row| row.url == "wss://slow.example")
            .unwrap()
            .publish_refused
    );
    assert_eq!(super::relays::read_relay_cookie(Some(&cookie)), list);
}

#[test]
fn nostr_publish_sends_the_saved_relays_and_leaves_out_configured_names() {
    let note = crate::nostr::sign_v2_note("0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec![
        "wss://relay.primal.net".to_string(),
        "wss://relay.snort.social".to_string(),
        "wss://offchain.pub".to_string(),
    ];
    let saved = vec![
        "wss://pin.example".to_string(),
        "wss://dealt.example".to_string(),
        "wss://other.example".to_string(),
    ];
    let body = NostrPublishBody {
        event,
        relays: saved.clone(),
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    let (_, chosen, _) = note_and_relays_for_publish(&body, &config, "").unwrap();
    assert_eq!(chosen, saved);
    for name in ["relay.primal.net", "relay.snort.social", "offchain.pub"] {
        assert!(
            !chosen.iter().any(|relay| relay.contains(name)),
            "{chosen:?}"
        );
    }
}

#[test]
fn tor_circuit_post_uses_the_publish_csrf_token() {
    let mut config = crate::config::Config::default();
    let relays = vec!["wss://relay.damus.io".to_string()];
    let missing = TorCircuitsBody {
        relays: relays.clone(),
        csrf_token: String::new(),
    };
    config.security.csrf_protection_enabled = true;
    assert!(relays_for_circuit_pass(&missing, &config, "").is_none());

    let token = crate::csrf::generate_csrf_token_with_timestamp();
    let accepted = TorCircuitsBody {
        relays: relays.clone(),
        csrf_token: token.clone(),
    };
    assert!(relays_for_circuit_pass(&accepted, &config, "").is_none());
    let other = crate::csrf::generate_csrf_token_with_timestamp();
    assert!(relays_for_circuit_pass(&accepted, &config, &other).is_none());
    assert_eq!(
        relays_for_circuit_pass(&accepted, &config, &token).as_deref(),
        Some(relays.as_slice())
    );
    config.security.csrf_protection_enabled = false;
    assert_eq!(
        relays_for_circuit_pass(&missing, &config, "").as_deref(),
        Some(relays.as_slice())
    );
}

#[test]
fn a_circuit_post_assigns_the_body_and_leaves_out_the_configured_name() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let watch = crate::nostr::RelayAssignmentWatch::begin(&[
        "wss://kept.example",
        "wss://stay.example",
        "wss://config.example",
    ]);
    let client = rocket::local::blocking::Client::untracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::assign_tor_circuits]),
    )
    .unwrap();
    let posted = client
        .post("/tor-circuits")
        .header(ContentType::JSON)
        .body(r#"{"relays":["wss://kept.example","wss://stay.example"]}"#)
        .dispatch();
    assert_eq!(posted.status(), Status::NoContent);
    assert_eq!(
        watch.assigned(),
        vec![
            "wss://kept.example".to_string(),
            "wss://stay.example".to_string(),
        ]
    );
    assert!(watch.remembers("wss://kept.example"));
    assert!(watch.remembers("wss://stay.example"));
    assert!(!watch.remembers("wss://config.example"));

    let empty = client
        .post("/tor-circuits")
        .header(ContentType::JSON)
        .body(r#"{"relays":[]}"#)
        .dispatch();
    assert_eq!(empty.status(), Status::NoContent);
    assert!(watch.assigned().is_empty());
    assert!(!watch.remembers("wss://kept.example"));
    assert!(!watch.remembers("wss://stay.example"));
    assert!(!watch.remembers("wss://config.example"));
}

#[test]
fn relays_to_assign_follow_a_publish() {
    let configured = vec![
        "wss://relay.primal.net".to_string(),
        "wss://relay.snort.social".to_string(),
        "wss://offchain.pub".to_string(),
    ];
    let mut page = configured.clone();
    page.push("wss://127.0.0.1".to_string());
    page.push("wss://relay.primal.net".to_string());
    for n in 1..=8 {
        page.push(format!("wss://extra{n}.example"));
    }
    let chosen = relays_to_assign(&page, &configured);
    assert!(chosen.len() <= crate::nostr::MAX_FETCH_RELAYS);
    assert!(chosen.starts_with(&configured));
    assert!(chosen.iter().all(|relay| !relay.contains("127.0.0.1")));
    assert_eq!(
        chosen,
        vec![
            "wss://relay.primal.net".to_string(),
            "wss://relay.snort.social".to_string(),
            "wss://offchain.pub".to_string(),
            "wss://extra1.example".to_string(),
            "wss://extra2.example".to_string(),
            "wss://extra3.example".to_string(),
        ]
    );
}

#[test]
fn relays_to_assign_stops_at_six_when_none_are_configured() {
    let configured = vec!["wss://127.0.0.1".to_string()];
    let page: Vec<String> = (1..=8).map(|n| format!("wss://extra{n}.example")).collect();
    let chosen = relays_to_assign(&page, &configured);
    assert_eq!(
        chosen,
        (1..=6)
            .map(|n| format!("wss://extra{n}.example"))
            .collect::<Vec<_>>()
    );
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
    assert!(!html.contains("{{nostr_link}}"));
}

#[test]
fn compose_page_posts_its_relays_and_still_paints_paths() {
    let home = include_str!("../templates/home.js");
    let relays = include_str!("../templates/relays.js");
    let bind = home.find("bindRelayActions(localStorage").unwrap();
    let send = home.find("async function sendRelayList()").unwrap();
    let refresh = home.find("async function refreshRelayCircuits(").unwrap();
    let get = home
        .find("fetch(\"/tor-circuits\", { cache: \"no-store\" })")
        .unwrap();
    let paint = home[get..].find("paintRelayCircuits()").unwrap();
    let publish = home.find("async function publishOnNostr()").unwrap();
    assert!(bind < refresh);
    assert!(relays.contains("postRelayList(next)"));
    assert!(relays.contains("show(reshuffleRelays(storage"));
    assert!(relays.contains("show(removeRelay(storage"));
    assert!(home[send..refresh].contains("method: \"POST\""));
    assert!(home[send..refresh].contains("savedRelayUrls(localStorage)"));
    assert!(home[send..refresh].contains("relays: relays"));
    assert!(home[send..refresh].contains("return relayListSend"));
    assert!(!home[send..refresh].contains("postedRelayList"));
    assert!(home[send..refresh].contains("response.ok"));
    assert!(home[send..refresh].contains("csrf_token:"));
    assert!(refresh < get);
    assert!(home[refresh..get].contains("await sendRelayList()"));
    assert!(!home[get..get + paint].contains("method: \"POST\""));
    assert!(home.contains("relay-spinner"));
    assert!(home.contains("everyRelayHasPath()"));
    let waiting = home.find("function everyRelayHasPath()").unwrap();
    let sync = home.find("function syncNostrButtons()").unwrap();
    assert!(home[waiting..sync].contains("if (!circuitsTracked)"));
    assert!(home[waiting..sync].contains("relaysForPublicPublish(savedRelayUrls(localStorage))"));
    assert!(home.contains("payload.tracking === true"));
    assert!(home.contains("circuits-pending"));
    assert!(!home.contains("waiting for publish"));
    assert!(home[publish..publish + 250].contains("!everyRelayHasPath()"));
    let css = include_str!("../templates/home.css");
    assert!(css.contains(".relay-spinner"));
    assert!(css.contains(".nostr-publish.circuits-pending"));
    assert!(css.contains("filter: blur(3px)"));
}

#[test]
fn seal_material_comes_from_get_random_values() {
    let nostr = include_str!("../templates/nostr.js");
    assert!(nostr.contains(&format!("SECRET_LEN = {}", nostorgraph_nip44::SECRET_LEN)));
    assert!(nostr.contains(&format!("LOCATOR_LEN = {}", nostorgraph_nip44::LOCATOR_LEN)));
    let start = nostr.find("function generateSealMaterial()").unwrap();
    let end = nostr[start..]
        .find("\nlet pageModule")
        .map(|offset| start + offset)
        .unwrap();
    let body = &nostr[start..end];
    assert!(body.contains("crypto.getRandomValues"));
    assert!(!body.contains("subtle"));
    assert!(body.contains("LOCATOR_LEN / 2"));
}

#[test]
fn seal_note_loads_page_wasm_and_stops_when_ciphertext_exceeds_the_cap() {
    let content = "hello";
    let locator = "0123456789abcdef0123";
    let secret = [4u8; nostorgraph_nip44::SECRET_LEN];
    let nonce = [5u8; 32];
    let payload =
        nostorgraph_nip44::seal("Title", "Ada", content, locator, &secret, &nonce).unwrap();
    let opened = nostorgraph_nip44::open(&payload, &secret, locator).unwrap();
    assert_eq!(opened.content, content);
    let limit = content.len();
    assert!(
        content.len() <= limit && payload.len() > limit,
        "NIP-44 padding must exceed a cap the plaintext still fits"
    );

    assert!(
        PAGE_JS.contains("export function seal(title, author, content, locator, secret, nonce)")
    );
    assert!(PAGE_JS.contains("({module_or_path} = module_or_path)"));

    let nostr = include_str!("../templates/nostr.js");
    let load = nostr.find("async function loadPage()").unwrap();
    let load_end = nostr[load..]
        .find("\nexport async function sealNote")
        .map(|offset| load + offset)
        .unwrap();
    let loader = &nostr[load..load_end];
    assert!(loader.contains("./page/nostorgraph_page.js"));
    assert!(loader.contains("./page/nostorgraph_page_bg.wasm"));
    assert!(loader.contains("module.default({ module_or_path: wasm })"));
    assert!(!loader.contains("subtle"));

    let start = nostr.find("function sealNote(").unwrap();
    let end = nostr[start..]
        .find("\nexport function mineSeal")
        .map(|offset| start + offset)
        .unwrap();
    let body = &nostr[start..end];
    assert!(body.contains("generateSealMaterial"));
    assert!(body.contains("crypto.getRandomValues"));
    assert!(body.contains("await loadPage()"));
    let sealed = body.find("const payload = page.seal(").unwrap();
    let check = body.find("if (payload.length > limit)").unwrap();
    assert!(sealed < check);
    assert!(!body.contains("content.length"));
    assert!(body.contains("The sealed note is too long."));
    assert!(!body.contains("subtle"));
    assert!(!body.contains("sha256"));
    assert!(!body.contains("schnorrSign"));
}

#[test]
fn seal_pow_bits_match_the_crate() {
    let nostr = include_str!("../templates/nostr.js");
    assert!(nostr.contains(&format!("POW_BITS = {}", nostorgraph_nip44::POW_BITS)));
    assert!(nostr.contains(&format!("KIND_SEAL = {}", nostorgraph_nip44::KIND)));
    let start = nostr.find("function mineSeal(").unwrap();
    let end = nostr[start..]
        .find("\nfunction ")
        .map(|offset| start + offset)
        .unwrap();
    let body = &nostr[start..end];
    assert!(body.contains("sha256Sync"));
    assert!(
        body.contains("encode.encode(canonicalEvent(pubkey, created_at, KIND_SEAL, tags, body))")
    );
    assert!(!body.contains("JSON.stringify"));
    assert!(body.contains("[\"nonce\", String(nonce), String(POW_BITS)]"));
    assert!(body.contains("leadingZeroBits(digest) >= POW_BITS"));
    assert!(body.contains("nonce += 1"));
    assert!(body.contains("KIND_SEAL"));
    assert!(!body.contains("subtle"));
    assert!(!body.contains("schnorrSign"));

    let zeros = nostr.find("function leadingZeroBits(").unwrap();
    let zeros_end = nostr[zeros..]
        .find("\nfunction ")
        .map(|offset| zeros + offset)
        .unwrap_or(nostr.len());
    let counter = &nostr[zeros..zeros_end];
    assert!(counter.contains("bits += 8"));
    assert!(counter.contains("Math.clz32(byte) - 24"));

    // mineSeal on these inputs finds nonce 5306. The crate must hash the same
    // canonical bytes or check_stamp will reject the event as Id / Stamp.
    const PUBKEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CREATED_AT: i64 = 1_700_000_000;
    const CONTENT: &str = "ciphertext";
    const LOCATOR: &str = "0123456789abcdef0123";
    const NONCE: &str = "5306";
    const CANONICAL: &str = concat!(
        r#"[0,"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","#,
        r#"1700000000,30323,"#,
        r#"[["d","0123456789abcdef0123"],["nonce","5306","12"]],"#,
        r#""ciphertext"]"#,
    );
    const ID: &str = "000ba9727f24f4d4fc85a44f54e6caca8577e1e04d898d0e26a81c7faf01dcc8";
    let tags = vec![
        vec!["d".to_string(), LOCATOR.to_string()],
        vec![
            "nonce".to_string(),
            NONCE.to_string(),
            nostorgraph_nip44::POW_BITS.to_string(),
        ],
    ];
    let mut rebuilt = String::from("[0,");
    nostorgraph_nip44::push_json_string(&mut rebuilt, PUBKEY);
    rebuilt.push(',');
    rebuilt.push_str(&CREATED_AT.to_string());
    rebuilt.push(',');
    rebuilt.push_str(&nostorgraph_nip44::KIND.to_string());
    rebuilt.push(',');
    nostorgraph_nip44::push_tags(&mut rebuilt, &tags);
    rebuilt.push(',');
    nostorgraph_nip44::push_json_string(&mut rebuilt, CONTENT);
    rebuilt.push(']');
    assert_eq!(rebuilt, CANONICAL);
    let id =
        nostorgraph_nip44::event_id(PUBKEY, CREATED_AT, nostorgraph_nip44::KIND, &tags, CONTENT);
    let hex: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, ID);
    assert!(nostorgraph_nip44::check_stamp(
        &id,
        PUBKEY,
        CREATED_AT,
        nostorgraph_nip44::KIND,
        &tags,
        CONTENT,
    )
    .is_ok());
}

#[test]
fn publish_signs_once_and_posts_the_event_without_the_key() {
    let nostr = include_str!("../templates/nostr.js");
    let start = nostr.find("async function publishPublicNote(").unwrap();
    let end = nostr[start..]
        .find("\nfunction xOnlyPubkey")
        .map(|offset| start + offset)
        .unwrap();
    let body = &nostr[start..end];
    let sealed = body.find("await sealNote(").unwrap();
    let signing = body
        .find("const signingKey = utils.randomPrivateKey()")
        .unwrap();
    let mined = body.find("mineSeal(").unwrap();
    let signed = body
        .find("await schnorrSign(signingKey, hexToBytes(mined.id))")
        .unwrap();
    let posted = body.find("fetch(\"/nostr/publish\"").unwrap();
    assert!(sealed < signing);
    assert!(signing < mined);
    assert!(mined < signed);
    assert!(signed < posted);
    assert_eq!(body.matches("schnorrSign(").count(), 1);
    assert_eq!(body.matches("utils.randomPrivateKey()").count(), 1);
    assert!(!body.contains("schnorrSign(key,"));
    assert!(!body.contains("signLongForm"));
    assert!(!body.contains("KIND_LONG_FORM"));
    assert!(!body.contains("fetch(\"/nostr/publish?"));
    assert!(body.contains("tags: mined.tags"));
    assert!(body.contains("locator !== sealed.locator"));
    assert!(body.contains("key: bytesToBase64Url(sealed.secret)"));
    assert!(!body.contains("nevent"));
    let returned = body.find("return {").unwrap();
    let returned_end = body[returned..]
        .find("};")
        .map(|offset| returned + offset)
        .unwrap();
    let returned = &body[returned..returned_end];
    assert!(returned.contains("title: String(title ?? \"\")"));
    let payload_at = body.find("JSON.stringify({").unwrap();
    let payload_end = body[payload_at..]
        .find("})")
        .map(|offset| payload_at + offset)
        .unwrap();
    let payload = &body[payload_at..payload_end];
    assert!(payload.contains("event,"));
    assert!(payload.contains("relays: relaysForPublicPublish(relays)"));
    assert!(payload.contains("seal_claim: claim"));
    assert!(payload.contains("csrf_token: csrfToken || \"\""));
    assert!(!payload.contains("secret"));
    assert!(!payload.contains("key"));
    assert!(!payload.contains("locator"));
    assert!(!payload.contains("sealed"));
    assert!(!payload.contains("title"));

    let home = include_str!("../templates/home.js");
    assert!(home.contains("contentMax: Number(form.dataset.sealMax)"));
    assert!(home.contains("The post is too long."));
    assert!(!home.contains("contentMax: contentLimit"));
    assert!(home.contains("showShareLinks(result, title)"));
    assert!(!home.contains("location.assign(longSealHref(result))"));
    assert!(home.contains("history.pushState"));
    assert!(home.contains("noteLocked || !hasAnyPublishRelay()"));
    assert!(home.contains("That link was not saved."));
    assert!(home.contains("Saved, but the link was not copied."));
    let href = home.find("function sealHref(").unwrap();
    let href_end = home[href..]
        .find("\n            async function publishOnNostr")
        .map(|offset| href + offset)
        .unwrap();
    let href_body = &home[href..href_end];
    assert!(href_body.contains("\"/\" + locator"));
    assert!(href_body.contains("\"/\" + encodeURIComponent(slug)"));
    assert!(href_body.contains("result.locator"));
    assert!(href_body.contains("result.event.id"));
    assert!(href_body.contains("result.hints"));
    assert!(href_body.contains("\"#\""));
    assert!(href_body.contains("result.key"));
    assert!(href_body.contains("encodeURIComponent(relay)"));
    assert!(!home.contains("result.secret"));
    assert!(!home.contains("result.nevent"));
    assert!(!home.contains("location.hash"));
}

#[test]
fn homepage_js_can_send_a_public_long_form_note() {
    let nostr = include_str!("../templates/nostr.js");
    assert!(nostr.contains(&format!("KIND_SEAL = {}", nostorgraph_nip44::KIND)));
    let publish_at = nostr.find("async function publishPublicNote(").unwrap();
    let publish_end = nostr[publish_at..]
        .find("\nfunction xOnlyPubkey")
        .map(|offset| publish_at + offset)
        .unwrap();
    let publish = &nostr[publish_at..publish_end];
    assert!(publish.contains("await sealNote("));
    assert!(publish.contains("mineSeal("));
    assert!(publish.contains("await schnorrSign(signingKey, hexToBytes(mined.id))"));
    assert!(publish.contains("fetch(\"/nostr/publish\""));
    assert!(publish.contains("kind: mined.kind"));
    assert!(!publish.contains("nevent"));
    assert!(!publish.contains("30023"));
    assert!(!publish.contains("KIND_LONG_FORM"));
    assert!(nostr.contains("publishPublicNote"));
    assert!(nostr.contains("fetch(\"/nostr/publish\""));
    assert!(!nostr.contains("new WebSocket"));
    assert!(nostr.contains("publicRelayUrl"));
    assert!(nostr.contains("relaysForPublicPublish"));
    assert!(nostr.contains("10_000"));
    assert!(nostr.contains("sha256Sync"));
    assert!(nostr.contains("ensureBip340"));
    assert!(nostr.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    assert!(nostr.contains(
        "e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0"
    ));
    assert!(!nostr.contains("1059"));
    assert!(!nostr.contains("nsec"));
    assert!(!nostr.contains("gift"));
    assert!(!nostr.contains("wrap"));

    let home = include_str!("../templates/home.js");
    let helper = home.find("nostorgraphPublishPublicNote").unwrap();
    let imported = home.find("await import").unwrap();
    assert!(helper < imported);
    assert!(home.contains("./nostr.js"));
    assert!(home.contains("publishPublicNote"));
    assert!(home.contains("csrfToken"));
    assert!(home.contains("showShareLinks(result, title)"));
    assert!(!home.contains("location.assign(longSealHref(result))"));
    assert!(home.contains("history.pushState"));
    assert!(home.contains("noteLocked || !hasAnyPublishRelay()"));
    assert!(home.contains("That link was not saved."));
    assert!(home.contains("Saved, but the link was not copied."));
    let href = home.find("function sealHref(").unwrap();
    let href_end = home[href..]
        .find("\n            async function publishOnNostr")
        .map(|offset| href + offset)
        .unwrap();
    let href_body = &home[href..href_end];
    assert!(href_body.contains("\"/\" + locator"));
    assert!(href_body.contains("\"/\" + encodeURIComponent(slug)"));
    assert!(href_body.contains("result.locator"));
    assert!(href_body.contains("result.event.id"));
    assert!(href_body.contains("result.hints"));
    assert!(href_body.contains("\"#\""));
    assert!(href_body.contains("result.key"));
    assert!(href_body.contains("encodeURIComponent(relay)"));
    assert!(home.contains("button[type=\"submit\"][form=\"publishForm\"]"));
    assert!(home.contains("e.preventDefault()"));
    assert!(home.contains("publishOnNostr()"));
    assert!(home.contains("Publishing failed. Try again."));
    assert!(home.contains("A title is required."));
    assert!(home.contains("if (noteLocked || !hasAnyPublishRelay())"));
    assert!(home.contains("if (publishing || !everyRelayHasPath())"));
    assert!(home.contains("setPublishBusy(false)"));
    assert!(!home.contains("leaveBusy"));
    assert!(home.contains("syncNostrButtons"));
    assert!(home.contains("savedRelayUrls(localStorage)"));
    assert!(home.contains("bindRelayActions(localStorage"));
    assert!(include_str!("../templates/relays.js").contains("nostorgraph_extra_relays"));
    assert!(home.contains("nostr.MAX_FETCH_RELAYS"));
    assert!(!home.contains("nostorgraph_extra_relays"));
    assert!(!home.contains("mergePublishRelays"));
    assert!(!home.contains("MAX_EXTRA_RELAYS"));
    assert!(!home.contains("acceptedRelayHints"));
    assert!(!home.contains("1059"));
    assert!(!home.contains("nsec"));

    let html = include_str!("../templates/home.html");
    assert!(html.contains("data-relays=\"{{nostr_relays}}\""));
    assert!(html.contains("data-timeout=\"{{nostr_timeout_ms}}\""));
    assert!(html.contains("data-seal-max=\"{{seal_max}}\""));
    assert!(html.contains("action=\"{{form_action}}\""));
    assert!(html.contains("type=\"submit\""));
    assert!(!html.contains("class=\"nostr-publish\""));
    assert!(!html.contains("On Nostr"));
    assert_eq!(html.matches(">Store</button>").count(), 2);
    assert!(html.contains("id=\"share-links\" class=\"share-links\" hidden"));
    assert!(html.contains(
        "Most private. This site stores nothing. The event and the relays are in the address. The key is the part after #."
    ));
    assert!(html.contains(
        "This site stores the event id and the relays. It does not store the writing or the key."
    ));
    assert!(html.contains("Least private. This site stores the title, the event id, and the relays. The title is in the address. It does not store the writing or the key."));
    assert!(html.contains("id=\"share-long-url\""));
    assert!(html.contains("id=\"share-short-url\""));
    assert!(html.contains("id=\"share-title-url\""));
    assert!(html.contains("class=\"share-label\" id=\"share-long-label\">Long</p>"));
    assert!(html.contains("class=\"share-label\" id=\"share-short-label\">Short</p>"));
    assert!(html.contains("class=\"share-label\" id=\"share-title-label\">Title</p>"));
    assert!(html.contains("class=\"share-bang\""));
    assert!(html.contains("class=\"share-pop\""));
    assert!(html.contains("role=\"tooltip\""));
    assert!(html.contains("id=\"share-long\""));
    assert!(html.contains("id=\"share-short\""));
    assert!(html.contains("id=\"share-title\""));
    let long_copy = home.find("async function copyLongLink").expect("long copy");
    let long_end = home[long_copy..]
        .find("\n            async function publishOnNostr")
        .map(|offset| long_copy + offset)
        .unwrap();
    let long_body = &home[long_copy..long_end];
    assert!(!long_body.contains("/seal-row"));
    assert!(!long_body.contains("/title-link"));
    assert!(home.contains("copyShortLink(sharedResult, csrf)"));
    assert!(home.contains("copyTitleLink(sharedResult, csrf)"));
    assert!(home.contains("copyLongLink(sharedResult)"));
    assert!(home.contains("editor.readonly = true"));
    assert!(home.contains("classList.add(\"is-stored\")"));
    assert!(html.contains("class=\"nojs-file-note\""));
    assert!(html.contains("This host seals the note and does not keep a copy."));
    assert!(html.contains("The key stays in the address."));
    assert!(html.contains("class=\"needs-js\""));
    assert!(html.contains("method=\"{{form_method}}\""));
    assert!(!html.contains("name=\"javascript_off\""));
    assert!(!html.contains("This page only saves a file on this host."));
    assert!(html.contains("<noscript>"));
    assert!(!html.contains("<style"));
    assert!(!html.contains(".nojs-file-note {"));
    let noscript = html.split("<noscript>").nth(1).unwrap();
    assert!(noscript.contains("/home-noscript.css?v={{asset_version}}"));
    assert!(!html
        .split("<noscript>")
        .next()
        .unwrap()
        .contains("home-noscript.css"));
    let noscript_css = include_str!("../templates/home-noscript.css");
    assert!(noscript_css.contains(".nostr-publish"));
    assert!(noscript_css.contains("display: none !important"));
    assert!(noscript_css.contains("body:not(.nojs) .needs-js"));
    assert!(noscript_css.contains("body:not(.nojs) button[type=\"submit\"][form=\"publishForm\"]"));
    let note = html.find("class=\"nojs-file-note\"").unwrap();
    let sidebar = html.find("class=\"sidebar\"").unwrap();
    assert!(note < sidebar);

    let css = include_str!("../templates/home.css");
    assert!(css.contains("body.nojs .nostr-publish"));
    assert!(css.contains("body.nojs .nojs-file-note"));
    assert!(css.contains(".needs-js"));
}

#[test]
fn the_homepage_omits_the_configured_names() {
    let config = crate::config::Config::default();
    let js = home_context(&config, false, None, None);
    let relays: Vec<String> = serde_json::from_str(js.get("nostr_relays").unwrap()).unwrap();
    assert!(relays.is_empty());
    let sidebar = js.get("relays_sidebar").unwrap();
    let help = js.get("relays_section").unwrap();
    for name in ["relay.primal.net", "relay.snort.social", "offchain.pub"] {
        assert!(!sidebar.contains(name), "{sidebar}");
        assert!(!help.contains(name), "{help}");
    }
    assert!(!sidebar.contains("Config.toml"), "{sidebar}");
    assert!(!help.contains("Config.toml"), "{help}");
    assert!(
        !sidebar.contains("No public relays configured."),
        "{sidebar}"
    );
    assert!(!help.contains("No public relays configured."), "{help}");
    assert!(sidebar.contains("No relays."));
    assert!(help.contains("No relays."));
    assert_eq!(js.get("nostr_timeout_ms").unwrap(), "10000");
    assert_eq!(
        js.get("seal_max").unwrap(),
        &crate::seal::sealed_payload_max(
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
        )
        .to_string()
    );
    assert_eq!(
        js.get("seal_max").unwrap(),
        &nostorgraph_nip44::MAX_PAYLOAD_CHARS.to_string()
    );
    assert!(js.get("scripts").unwrap().contains("/home.js?v="));
    assert!(!js.get("scripts").unwrap().contains("/nostr.js"));

    let mut slow = config.clone();
    slow.nostr.timeout_secs = 20;
    let longer = home_context(&slow, false, None, None);
    assert_eq!(longer.get("nostr_timeout_ms").unwrap(), "20000");

    let mut private = config.clone();
    private.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let hidden = home_context(&private, false, None, None);
    assert_eq!(hidden.get("nostr_relays").unwrap(), "[]");

    let nojs = home_context(&config, true, None, None);
    assert!(nojs.get("scripts").unwrap().is_empty());
    assert_eq!(nojs.get("nostr_relays").unwrap(), "[]");
}

#[test]
fn the_writing_page_script_paints_the_saved_rows() {
    let home = include_str!("../templates/home.js");
    let start = home
        .find("function savedRelayUrls(")
        .expect("saved relay urls");
    let end = home.find("            const editor").expect("editor");
    let mut program = home[start..end].to_string();
    program.push_str(
        r##"
function element(className) {
    const node = {
        className: className || "",
        classList: {
            contains(token) {
                return (" " + node.className + " ").indexOf(" " + token + " ") !== -1;
            },
        },
        dataset: {},
        textContent: "",
        children: [],
        parentNode: null,
        append() {
            for (let index = 0; index < arguments.length; index += 1) {
                arguments[index].parentNode = node;
                node.children.push(arguments[index]);
            }
        },
        replaceChildren() {
            node.children = [];
            for (let index = 0; index < arguments.length; index += 1) {
                node.append(arguments[index]);
            }
        },
        remove() {
            if (!node.parentNode) {
                return;
            }
            node.parentNode.children = node.parentNode.children.filter((child) => child !== node);
            node.parentNode = null;
        },
        insertBefore(child, before) {
            child.parentNode = node;
            const index = node.children.indexOf(before);
            if (index < 0) {
                node.children.push(child);
            } else {
                node.children.splice(index, 0, child);
            }
        },
        querySelector(selector) {
            return findNode(node, selector);
        },
        querySelectorAll(selector) {
            const found = [];
            gatherNodes(node, selector, found);
            return found;
        },
    };
    return node;
}
function matches(node, selector) {
    if (selector === ".relay-list") {
        return node.classList.contains("relay-list");
    }
    if (selector === ".relay-empty") {
        return node.classList.contains("relay-empty");
    }
    if (selector === ".relay-add") {
        return node.classList.contains("relay-add");
    }
    return false;
}
function findNode(node, selector) {
    for (let index = 0; index < node.children.length; index += 1) {
        const child = node.children[index];
        if (matches(child, selector)) {
            return child;
        }
        const nested = findNode(child, selector);
        if (nested) {
            return nested;
        }
    }
    return null;
}
function gatherNodes(node, selector, found) {
    for (let index = 0; index < node.children.length; index += 1) {
        const child = node.children[index];
        if (matches(child, selector)) {
            found.push(child);
        }
        gatherNodes(child, selector, found);
    }
}
function section() {
    const root = element("");
    const empty = element("relay-empty");
    const add = element("relay-add");
    const kept = element("relay-list relay-list-user");
    add.append(kept);
    root.append(empty, add);
    return root;
}
const sidebar = section();
const help = section();
const document = {
    createElement() {
        return element("");
    },
    querySelector(selector) {
        if (selector === "#sidebar-relays") {
            return sidebar;
        }
        if (selector === "#relays") {
            return help;
        }
        return null;
    },
};
const storage = {
    getItem(key) {
        if (key !== "nostorgraph_relays") {
            return null;
        }
        return JSON.stringify({
            pinned: ["wss://pin.example"],
            dealt: ["wss://dealt.example", "wss://other.example"],
        });
    },
};
paintSavedRelayLists(document, storage);
function painted(root) {
    const list = root.querySelector(".relay-list");
    if (!list || list.classList.contains("relay-list-user")) {
        throw new Error("saved list was not painted");
    }
    return list.children.map((row) => row.dataset.relay + " " + row.children[0].textContent);
}
const expected = [
    "wss://pin.example pin.example",
    "wss://dealt.example dealt.example",
    "wss://other.example other.example",
];
if (JSON.stringify(painted(sidebar)) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(painted(sidebar)));
}
if (JSON.stringify(painted(help)) !== JSON.stringify(expected)) {
    throw new Error(JSON.stringify(painted(help)));
}
if (sidebar.querySelector(".relay-empty") || help.querySelector(".relay-empty")) {
    throw new Error("empty message stayed");
}
const user = sidebar.querySelector(".relay-add").querySelector(".relay-list");
if (!user.classList.contains("relay-list-user") || user.children.length !== 0) {
    throw new Error("user list changed");
}
"##,
    );
    run_javascript("nostorgraph-saved-rows", &program);
}

#[test]
fn publish_sends_the_saved_relays() {
    let home = include_str!("../templates/home.js");
    let urls_at = home
        .find("function savedRelayUrls(")
        .expect("saved relay urls");
    let urls_end = home[urls_at..]
        .find("\nfunction paintSavedRelayLists(")
        .map(|offset| urls_at + offset)
        .expect("paint saved lists");
    let publish_at = home
        .find("globalThis.nostorgraphPublishPublicNote")
        .expect("publish helper");
    let publish_end = home[publish_at..]
        .find("\n            async function loadNostr(")
        .map(|offset| publish_at + offset)
        .expect("load nostr");
    let ready_at = home
        .find("function hasAnyPublishRelay()")
        .expect("publish gate");
    let ready_end = home[ready_at..]
        .find("\n            globalThis.nostorgraphPublishPublicNote")
        .map(|offset| ready_at + offset)
        .expect("publish helper follows the gate");
    let path_at = home
        .find("function everyRelayHasPath()")
        .expect("path gate");
    let path_end = home[path_at..]
        .find("\n            function syncNostrButtons()")
        .map(|offset| path_at + offset)
        .expect("button sync follows the path gate");
    let key_at = home.find("function relayKey(url)").expect("relay key");
    let key_end = home[key_at..]
        .find("\n            globalThis.paintSavedRelayLists")
        .map(|offset| key_at + offset)
        .expect("saved lists follow the relay key");
    let post_at = home
        .find("let relayListSending = false;")
        .expect("relay post");
    let post_end = home[post_at..]
        .find("\n            async function refreshRelayCircuits(")
        .map(|offset| post_at + offset)
        .expect("circuit refresh follows the relay post");
    let mut program = home[urls_at..urls_end].to_string();
    program.push_str(
        r##"
const bags = {
    saved: JSON.stringify({
        pinned: ["wss://pin.example"],
        dealt: ["wss://dealt.example"],
    }),
};
const localStorage = {
    getItem(key) {
        if (key === "nostorgraph_relays") {
            return bags.saved;
        }
        return null;
    },
    setItem() {},
};
const form = {
    dataset: {
        timeout: "10000",
        sealMax: "100",
        relays: JSON.stringify([
            "wss://relay.primal.net",
            "wss://relay.snort.social",
            "wss://offchain.pub",
        ]),
    },
    csrf_token: { value: "token" },
};
let sent = null;
async function loadNostr() {
    return {
        publishPublicNote(fields) {
            sent = fields.relays;
            return {};
        },
    };
}
let relaysForPublicPublish = (relays) => relays.slice();
const circuitByRelay = new Map();
let circuitsTracked = false;
"##,
    );
    program.push_str(&home[publish_at..publish_end]);
    program.push_str(&home[ready_at..ready_end]);
    program.push_str(&home[key_at..key_end]);
    program.push_str(&home[path_at..path_end]);
    program.push_str(&home[post_at..post_end]);
    program.push_str(
        r##"
(async () => {
    try {
    const expected = ["wss://pin.example", "wss://dealt.example"];
    await nostorgraphPublishPublicNote({ title: "Hello", author: "Ada", content: "hi" });
    if (JSON.stringify(sent) !== JSON.stringify(expected)) {
        throw new Error(JSON.stringify(sent));
    }
    for (const name of ["relay.primal.net", "relay.snort.social", "offchain.pub"]) {
        if (sent.some((relay) => relay.includes(name))) {
            throw new Error(JSON.stringify(sent));
        }
    }
    if (!hasAnyPublishRelay()) {
        throw new Error("saved list should show publish");
    }
    circuitsTracked = true;
    for (const relay of expected) {
        circuitByRelay.set(relay, "path-" + relay);
    }
    if (!everyRelayHasPath()) {
        throw new Error("saved relays with paths should allow publish");
    }
    circuitByRelay.delete("wss://dealt.example");
    if (everyRelayHasPath()) {
        throw new Error("a saved relay without a path should wait");
    }
    circuitsTracked = false;
    if (!everyRelayHasPath()) {
        throw new Error("untracked circuits should allow publish");
    }
    let posted = null;
    globalThis.fetch = async (_url, options) => {
        posted = JSON.parse(options.body).relays;
        return { ok: true };
    };
    await sendRelayList();
    if (JSON.stringify(posted) !== JSON.stringify(expected)) {
        throw new Error(JSON.stringify(posted));
    }
    bags.saved = null;
    await nostorgraphPublishPublicNote({ title: "Hello", author: "Ada", content: "hi" });
    if (JSON.stringify(sent) !== JSON.stringify([])) {
        throw new Error(JSON.stringify(sent));
    }
    await sendRelayList();
    if (JSON.stringify(posted) !== JSON.stringify([])) {
        throw new Error(JSON.stringify(posted));
    }
    if (hasAnyPublishRelay()) {
        throw new Error("configured names should stay off the button");
    }
    circuitsTracked = true;
    if (everyRelayHasPath()) {
        throw new Error("an empty list should wait");
    }
    } catch (error) {
        if (typeof setTimeout === "function") {
            setTimeout(function () { throw error; }, 0);
        }
        throw error;
    }
})();
"##,
    );
    run_javascript("nostorgraph-publish-saved", &program);
}

#[test]
fn the_home_script_deals_three_and_publishes_the_saved_list() {
    let relays = include_str!("../templates/relays.js");
    let home = include_str!("../templates/home.js");
    let urls_at = home
        .find("function savedRelayUrls(")
        .expect("saved relay urls");
    let urls_end = home[urls_at..]
        .find("\nfunction paintSavedRelayLists(")
        .map(|offset| urls_at + offset)
        .expect("paint saved lists");
    let publish_at = home
        .find("globalThis.nostorgraphPublishPublicNote")
        .expect("publish helper");
    let publish_end = home[publish_at..]
        .find("\n            async function loadNostr(")
        .map(|offset| publish_at + offset)
        .expect("load nostr");
    let mut program = relays.to_string();
    program.push_str(&home[urls_at..urls_end]);
    program.push_str(&home[publish_at..publish_end]);
    program.push_str(
        r##"
const nodes = [];
function hasClass(node, name) {
    return (" " + (node.className || "") + " ").indexOf(" " + name + " ") !== -1;
}
function selectorMatches(node, selector) {
    if (selector.charAt(0) === ".") {
        return hasClass(node, selector.slice(1));
    }
    if (selector.charAt(0) === "#") {
        return node.id === selector.slice(1);
    }
    if (selector === "[data-relay]") {
        return typeof node.dataset.relay === "string";
    }
    return false;
}
function walk(node, selector, found) {
    for (let index = 0; index < node.children.length; index += 1) {
        const child = node.children[index];
        if (selectorMatches(child, selector)) {
            found.push(child);
        }
        walk(child, selector, found);
    }
}
function el(tag) {
    const node = {
        tag: tag,
        className: "",
        id: "",
        value: "",
        textContent: "",
        dataset: {},
        attributes: {},
        children: [],
        parentNode: null,
        listeners: {},
        classList: {
            contains(token) {
                return hasClass(node, token);
            },
            add(token) {
                if (!hasClass(node, token)) {
                    node.className = (node.className + " " + token).trim();
                }
            },
            remove(token) {
                node.className = node.className
                    .split(/\s+/)
                    .filter(function (part) {
                        return part && part !== token;
                    })
                    .join(" ");
            },
            toggle(token, force) {
                const on = force === undefined ? !hasClass(node, token) : force;
                if (on) {
                    this.add(token);
                } else {
                    this.remove(token);
                }
                return on;
            },
        },
        setAttribute(name, value) {
            node.attributes[name] = String(value);
        },
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
            node.parentNode = null;
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
        querySelector(selector) {
            const found = [];
            walk(node, selector, found);
            return found[0] || null;
        },
        addEventListener(type, fn) {
            node.listeners[type] = fn;
        },
    };
    nodes.push(node);
    return node;
}
const sidebar = el("div");
sidebar.id = "sidebar-relays";
const controls = el("div");
controls.className = "relay-deal-controls";
sidebar.append(controls);
const count = el("input");
count.className = "relay-count";
controls.append(count);
const reshuffle = el("button");
reshuffle.className = "relay-reshuffle";
controls.append(reshuffle);
const input = el("input");
input.className = "relay-url-input";
controls.append(input);
const add = el("button");
add.className = "relay-add-btn";
controls.append(add);
const candidates = el("script");
candidates.id = "relay-candidates";
candidates.textContent = JSON.stringify([
    "wss://a.example",
    "wss://b.example",
    "wss://c.example",
    "wss://d.example",
    "wss://e.example",
]);
sidebar.append(candidates);
const csrf = el("input");
csrf.value = "token";
var document = {
    createElement(tag) {
        return el(tag);
    },
    createElementNS(_namespace, tag) {
        return el(tag);
    },
    querySelector(selector) {
        if (selector === "input[name=\"csrf_token\"]") {
            return csrf;
        }
        throw new Error("unscoped " + selector);
    },
    querySelectorAll(selector) {
        const found = [];
        function visit(node) {
            for (let index = 0; index < node.children.length; index += 1) {
                const child = node.children[index];
                if (
                    selector === ".relay-list li[data-relay]" &&
                    child.dataset &&
                    child.dataset.relay &&
                    hasClass(node, "relay-list")
                ) {
                    found.push(child);
                }
                visit(child);
            }
        }
        visit(sidebar);
        return found;
    },
};
const localStorage = {
    values: {
        nostorgraph_extra_relays: JSON.stringify(["wss://extra.example"]),
    },
    getItem(key) {
        return Object.prototype.hasOwnProperty.call(this.values, key) ? this.values[key] : null;
    },
    setItem(key, value) {
        this.values[key] = String(value);
    },
    removeItem(key) {
        delete this.values[key];
    },
};
const form = {
    dataset: {
        timeout: "10000",
        sealMax: "100",
        relays: JSON.stringify([
            "wss://relay.primal.net",
            "wss://relay.snort.social",
            "wss://offchain.pub",
        ]),
    },
    csrf_token: { value: "token" },
};
let sent = null;
async function loadNostr() {
    return {
        publishPublicNote(fields) {
            sent = fields.relays;
            return {};
        },
    };
}
globalThis.fetch = async function () {
    return { ok: true };
};
const publishButton = el("button");
const publishButtons = [publishButton];
let relaysForPublicPublish = (relays) => relays.slice();
const circuitByRelay = new Map();
let circuitsTracked = false;
function syncPublishButtons() {}
bindRelayActions(localStorage, sidebar);
const saved = JSON.parse(localStorage.getItem("nostorgraph_relays"));
const firstThree = ["wss://a.example", "wss://b.example", "wss://c.example"];
if (JSON.stringify(saved.dealt) !== JSON.stringify(firstThree)) {
    throw new Error(JSON.stringify(saved));
}
if (JSON.stringify(saved.pinned) !== JSON.stringify(["wss://extra.example"])) {
    throw new Error(JSON.stringify(saved));
}
if (localStorage.getItem("nostorgraph_extra_relays") != null) {
    throw new Error("extra list was kept");
}
const painted = sidebar.querySelector(".relay-list");
if (!painted || painted.children.length !== 4) {
    throw new Error("sidebar did not paint the saved list");
}
input.value = "wss://hand.example";
add.listeners.click();
reshuffle.listeners.click();
const after = JSON.parse(localStorage.getItem("nostorgraph_relays"));
if (after.pinned.indexOf("wss://hand.example") === -1) {
    throw new Error(JSON.stringify(after));
}
if (after.pinned.indexOf("wss://extra.example") === -1) {
    throw new Error(JSON.stringify(after));
}
"##,
    );
    let gate_at = home
        .find("function hasAnyPublishRelay()")
        .expect("publish gate");
    let gate_end = home[gate_at..]
        .find("\n            globalThis.nostorgraphPublishPublicNote")
        .map(|offset| gate_at + offset)
        .expect("publish helper follows the gate");
    let path_at = home
        .find("function everyRelayHasPath()")
        .expect("path gate");
    let path_end = home[path_at..]
        .find("\n            globalThis.paintSavedRelayLists")
        .map(|offset| path_at + offset)
        .expect("saved lists follow the path gate");
    let post_at = home
        .find("let relayListSending = false;")
        .expect("relay post");
    let post_end = home[post_at..]
        .find("\n            globalThis.refreshRelayCircuits")
        .map(|offset| post_at + offset)
        .expect("circuit refresh is published");
    program.push_str(&home[gate_at..gate_end]);
    program.push_str(&home[path_at..path_end]);
    program.push_str(&home[post_at..post_end]);
    program.push_str(
        r##"
globalThis.refreshRelayCircuits = refreshRelayCircuits;
(async () => {
    try {
    await nostorgraphPublishPublicNote({ title: "Hello", author: "Ada", content: "hi" });
    const expected = after.pinned.concat(after.dealt);
    if (JSON.stringify(sent) !== JSON.stringify(expected)) {
        throw new Error(JSON.stringify(sent));
    }
    if (sent.indexOf("wss://hand.example") === -1) {
        throw new Error(JSON.stringify(sent));
    }
    if (sent.filter((relay) => relay === "wss://extra.example").length !== 1) {
        throw new Error(JSON.stringify(sent));
    }
    for (const name of ["relay.primal.net", "relay.snort.social", "offchain.pub"]) {
        if (sent.some((relay) => relay.includes(name))) {
            throw new Error(JSON.stringify(sent));
        }
    }
    circuitsTracked = true;
    for (const row of document.querySelectorAll(".relay-list li[data-relay]")) {
        circuitByRelay.set(row.dataset.relay, "OldPath");
    }
    paintRelayCircuits();
    for (const row of document.querySelectorAll(".relay-list li[data-relay]")) {
        const line = row.querySelector(".relay-circuit");
        const hops = line && line.querySelector(".relay-circuit-hop");
        const guard = line && line.querySelector(".relay-guard-label");
        if (!hops || hops.textContent !== "OldPath" || !guard || guard.textContent !== "guard") {
            throw new Error("rows did not show the old paths");
        }
    }
    let circuitPaths = {};
    const calls = [];
    let refreshPromise = Promise.resolve();
    const refreshCircuits = globalThis.refreshRelayCircuits;
    globalThis.refreshRelayCircuits = function () {
        refreshPromise = refreshCircuits();
        return refreshPromise;
    };
    globalThis.fetch = async (url, options) => {
        calls.push({
            url: url,
            method: options && options.method,
            body: options && options.body,
        });
        if (options && options.method === "POST") {
            return { ok: true };
        }
        return {
            ok: true,
            json: async () => ({ tracking: true, relays: circuitPaths }),
        };
    };
    const hand = sidebar.querySelector(".relay-list").children.find(function (row) {
        return row.dataset.relay === "wss://hand.example";
    });
    const remove = hand.children.find(function (child) {
        return hasClass(child, "relay-remove");
    });
    sidebar.listeners.click({ target: remove });
    await refreshPromise;
    const savedNow = JSON.parse(localStorage.getItem("nostorgraph_relays"));
    const nextList = savedNow.pinned.concat(savedNow.dealt);
    if (nextList.indexOf("wss://hand.example") !== -1) {
        throw new Error(JSON.stringify(nextList));
    }
    const posted = calls.filter(function (call) {
        return call.method === "POST";
    });
    if (posted.length !== 1) {
        throw new Error("relay change posted more than once: " + JSON.stringify(posted));
    }
    if (
        !posted.some(function (call) {
            return JSON.stringify(JSON.parse(call.body).relays) === JSON.stringify(nextList);
        })
    ) {
        throw new Error(JSON.stringify(posted));
    }
    if (!calls.some(function (call) { return call.url === "/tor-circuits" && call.method !== "POST"; })) {
        throw new Error(JSON.stringify(calls));
    }
    const rows = document.querySelectorAll(".relay-list li[data-relay]");
    if (rows.length !== nextList.length) {
        throw new Error("painted rows do not match the saved list");
    }
    for (const row of rows) {
        if (row.dataset.relay === "wss://hand.example") {
            throw new Error("removed relay stayed on the row");
        }
        const line = row.querySelector(".relay-circuit");
        if (!line || line.textContent === "OldPath" || !line.classList.contains("relay-circuit-wait")) {
            throw new Error("paths did not start over");
        }
        if (!row.querySelector(".relay-spinner")) {
            throw new Error("a row is missing its circuit spinner");
        }
    }
    if (everyRelayHasPath() || publishButton.attributes["aria-disabled"] !== "true") {
        throw new Error("publish should stay quiet until every relay has a path");
    }
    if (!publishButton.classList.contains("circuits-pending")) {
        throw new Error("publish should show that circuits are pending");
    }
    for (const url of nextList) {
        circuitPaths[url] = "path-" + url;
    }
    await refreshCircuits();
    for (const row of document.querySelectorAll(".relay-list li[data-relay]")) {
        const line = row.querySelector(".relay-circuit");
        const hops = line && line.querySelector(".relay-circuit-hop");
        const guard = line && line.querySelector(".relay-guard-label");
        if (!hops || hops.textContent !== "path-" + row.dataset.relay) {
            throw new Error(hops && hops.textContent);
        }
        if (!guard || guard.textContent !== "guard") {
            throw new Error("a path is missing its guard");
        }
        if (line.classList.contains("relay-circuit-wait")) {
            throw new Error("a finished path is still waiting");
        }
    }
    if (!everyRelayHasPath() || publishButton.attributes["aria-disabled"] !== "false") {
        throw new Error("publish should proceed once every relay has a path");
    }
    const sampleRow = document.querySelectorAll(".relay-list li[data-relay]")[0];
    const hopBefore = sampleRow.querySelector(".relay-circuit-hop");
    const postsBefore = calls.filter(function (call) {
        return call.method === "POST";
    }).length;
    await refreshCircuits();
    const hopAfter = sampleRow.querySelector(".relay-circuit-hop");
    const postsAfter = calls.filter(function (call) {
        return call.method === "POST";
    }).length;
    if (hopBefore !== hopAfter) {
        throw new Error("an unchanged circuit rebuilt the row");
    }
    if (postsAfter !== postsBefore + 1) {
        throw new Error("a poll did not post the relay list");
    }
    const pollPost = calls.filter(function (call) {
        return call.method === "POST";
    }).pop();
    const listed = JSON.parse(localStorage.getItem("nostorgraph_relays"));
    const currentList = listed.pinned.concat(listed.dealt);
    if (JSON.stringify(JSON.parse(pollPost.body).relays) !== JSON.stringify(currentList)) {
        throw new Error(JSON.stringify(pollPost.body));
    }
    let rejected = 0;
    globalThis.fetch = async (url, options) => {
        if (options && options.method === "POST") {
            rejected += 1;
            return { ok: false };
        }
        return {
            ok: true,
            json: async () => ({ tracking: true, relays: circuitPaths }),
        };
    };
    await sendRelayList();
    if (rejected !== 1) {
        throw new Error("a rejected relay list was posted more than once");
    }
    let retried = 0;
    globalThis.fetch = async (url, options) => {
        if (options && options.method === "POST") {
            retried += 1;
            return { ok: true };
        }
        return {
            ok: true,
            json: async () => ({ tracking: true, relays: circuitPaths }),
        };
    };
    await sendRelayList();
    if (retried !== 1) {
        throw new Error("a rejected relay list was not retried");
    }
    const quietRow = document.querySelectorAll(".relay-list li[data-relay]")[0];
    const quietHop = quietRow.querySelector(".relay-circuit-hop");
    lastTypingAt = Date.now();
    circuitPaths[quietRow.dataset.relay] = "fresh-during-typing";
    await refreshCircuits();
    clearTimeout(circuitPollTimer);
    circuitPollTimer = 0;
    if (quietRow.querySelector(".relay-circuit-hop") !== quietHop) {
        throw new Error("a poll during typing rebuilt the row");
    }
    circuitPaths[quietRow.dataset.relay] = "wait";
    await refreshCircuits({ force: true });
    clearTimeout(circuitPollTimer);
    circuitPollTimer = 0;
    const forcedHop = quietRow.querySelector(".relay-circuit-hop");
    if (!forcedHop || forcedHop.textContent !== "wait" || quietRow.querySelector(".relay-spinner")) {
        throw new Error("a relay change should paint while typing");
    }
    lastTypingAt = 0;
    const target = quietRow.dataset.relay;
    const releaseGet = [];
    globalThis.fetch = async (url, options) => {
        if (options && options.method === "POST") {
            return { ok: true };
        }
        return new Promise((resolve) => {
            releaseGet.push(resolve);
        });
    };
    const older = refreshCircuits();
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (releaseGet.length !== 1) {
        throw new Error("the older circuit read did not start");
    }
    const newer = refreshCircuits();
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (releaseGet.length !== 2) {
        throw new Error("the newer circuit read did not start");
    }
    releaseGet[1]({
        ok: true,
        json: async () => ({ tracking: true, relays: { [target]: "fresh-path" } }),
    });
    await newer;
    releaseGet[0]({
        ok: true,
        json: async () => ({ tracking: true, relays: { [target]: "stale-path" } }),
    });
    await older;
    const shown = quietRow.querySelector(".relay-circuit-hop");
    if (!shown || shown.textContent !== "fresh-path") {
        throw new Error(shown && shown.textContent);
    }
    const releasePost = [];
    const postBodies = [];
    let readsAfterPost = 0;
    globalThis.fetch = async (url, options) => {
        if (options && options.method === "POST") {
            postBodies.push(options.body);
            return new Promise((resolve) => {
                releasePost.push(resolve);
            });
        }
        readsAfterPost += 1;
        return {
            ok: true,
            json: async () => ({ tracking: true, relays: { [target]: "after-post" } }),
        };
    };
    const earlier = refreshCircuits();
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (releasePost.length !== 1 || readsAfterPost !== 0) {
        throw new Error("the circuit read started before its post");
    }
    const savedList = JSON.parse(localStorage.getItem("nostorgraph_relays"));
    savedList.pinned.push("wss://late.example");
    localStorage.setItem("nostorgraph_relays", JSON.stringify(savedList));
    const later = refreshCircuits({ force: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (readsAfterPost !== 0 || releasePost.length !== 1) {
        throw new Error("a second refresh read before the in-flight post finished");
    }
    releasePost[0]({ ok: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (releasePost.length !== 2 || readsAfterPost !== 0) {
        throw new Error("the changed list was not posted before the read");
    }
    const sentLate = JSON.parse(postBodies[1]).relays;
    if (sentLate.indexOf("wss://late.example") === -1) {
        throw new Error(JSON.stringify(sentLate));
    }
    releasePost[1]({ ok: true });
    await later;
    await earlier;
    if (readsAfterPost !== 1) {
        throw new Error("expected one circuit read after the posts");
    }
    const afterPost = quietRow.querySelector(".relay-circuit-hop");
    if (!afterPost || afterPost.textContent !== "after-post") {
        throw new Error(afterPost && afterPost.textContent);
    }
    } catch (error) {
        if (typeof setTimeout === "function") {
            setTimeout(function () { throw error; }, 0);
        }
        throw error;
    }
})();
"##,
    );
    run_javascript("nostorgraph-home-deal", &program);
}

fn run_javascript(prefix: &str, program: &str) {
    let path = std::env::temp_dir().join(format!("{prefix}-{}.js", std::process::id()));
    std::fs::write(&path, format!("{}\n{program}", script_shim())).expect("write script fixture");
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
        .expect("node or osascript must run the script")
}

#[test]
fn the_nojs_writing_page_lists_the_cookie_and_leaves_out_configured_names() {
    let mut config = crate::config::Config::default();
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let saved = super::relays::write_relay_cookie(&super::relays::RelayCookie {
        pinned: vec!["wss://pin.example".to_string()],
        dealt: vec![
            "wss://dealt.example".to_string(),
            "wss://other.example".to_string(),
        ],
    });
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::nojs_index]),
    )
    .unwrap();
    let page = client
        .get("/nojs")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved))
        .dispatch();
    assert_eq!(page.status(), Status::Ok);
    let html = page.into_string().unwrap();
    let help = region(
        &html,
        "<div class=\"help-section\" id=\"relays\">",
        "</div>",
    );
    let sidebar = region(&html, "id=\"sidebar-relays\"", "class=\"footer\"");
    for section in [help, sidebar] {
        let pin = section.find("data-relay=\"wss://pin.example\"").unwrap();
        let dealt = section.find("data-relay=\"wss://dealt.example\"").unwrap();
        let other = section.find("data-relay=\"wss://other.example\"").unwrap();
        assert!(pin < dealt && dealt < other, "{section}");
        assert!(!section.contains("wss://config.example"), "{section}");
        assert!(!section.contains("config.example"), "{section}");
    }
    assert!(!help.contains("Config.toml"));
    assert!(help.contains("Add or remove relays with the form."));
    assert!(help.contains("stays in this browser"));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_nojs_form_adds_removes_and_reshuffles_and_the_write_sends_the_cookie() {
    struct ClearPublish;
    impl Drop for ClearPublish {
        fn drop(&mut self) {
            set_shared_nojs_accepted(None);
            note_nojs_publish_relays(&[]);
        }
    }
    let _clear = ClearPublish;

    let dir = tempfile::tempdir().unwrap();
    let _content = super::relays::RelayContentGuard::set(dir.path());
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    let rows = (0..8)
        .map(|index| crawler::Row {
            url: format!("wss://relay{index}.example"),
            first_seen: 1,
            next_try: 1,
            failures: 0,
            publish_refused: false,
            visit: Some(crawler::Visit {
                at: 1,
                socket_answered: true,
                nip11: Some(document.clone()),
                probes: vec![crawler::ProbeResult {
                    name: "30323".to_string(),
                    accepted: true,
                    detail: "OK".to_string(),
                }],
            }),
        })
        .collect();
    crawler::save(&path, &crawler::Pool { rows }).unwrap();

    let mut config = crate::config::Config::default();
    config.nostr.timeout_secs = 1;
    let client = rocket::local::blocking::Client::tracked(rocket::build().manage(config).mount(
        "/",
        routes![
            super::nojs_index,
            super::nojs_relays_update,
            super::nojs_create_post,
        ],
    ))
    .unwrap();

    let first = client.get("/nojs").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let saved = first
        .cookies()
        .get(super::relays::RELAY_LIST_COOKIE)
        .expect("a missing cookie deals three")
        .value()
        .to_string();
    let opened = super::relays::read_relay_cookie(Some(&saved));
    assert!(opened.pinned.is_empty());
    assert_eq!(opened.dealt.len(), 3);
    let html = first.into_string().unwrap();
    assert!(html.contains("action=\"/nojs/relays\""));
    assert!(html.contains("class=\"relay-count\">"));
    assert!(html.contains("class=\"relay-reshuffle\""));
    assert!(html.contains("class=\"relay-add-btn\""));
    assert!(!html.contains("relay-delete-all"));
    for url in &opened.dealt {
        assert!(html.contains(&format!("data-relay=\"{url}\"")), "{html}");
    }
    let token = csrf_value(&html);

    let added = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2Fhand.example&csrf_token={token}"
        ))
        .dispatch();
    assert_eq!(added.status(), Status::SeeOther);
    assert_eq!(added.headers().get_one("Location"), Some("/nojs"));
    drop(added);
    let html = client.get("/nojs").dispatch().into_string().unwrap();
    let at = html.find("data-relay=\"").unwrap();
    assert!(html[at..].starts_with("data-relay=\"wss://hand.example\""));
    let token = csrf_value(&html);
    let saved = client
        .cookies()
        .get(super::relays::RELAY_LIST_COOKIE)
        .expect("add stores the pin")
        .value()
        .to_string();
    let listed = super::relays::read_relay_cookie(Some(&saved));
    assert_eq!(listed.pinned, vec!["wss://hand.example".to_string()]);
    let removed = listed.dealt[0].clone();

    let gone = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "remove={}&csrf_token={token}",
            removed.replace("://", "%3A%2F%2F")
        ))
        .dispatch();
    assert_eq!(gone.status(), Status::SeeOther);
    assert_eq!(gone.headers().get_one("Location"), Some("/nojs"));
    drop(gone);
    let html = client.get("/nojs").dispatch().into_string().unwrap();
    assert!(!html.contains(&format!("data-relay=\"{removed}\"")));
    assert!(html.contains("data-relay=\"wss://hand.example\""));
    let token = csrf_value(&html);
    let before = super::relays::read_relay_cookie(Some(
        client
            .cookies()
            .get(super::relays::RELAY_LIST_COOKIE)
            .unwrap()
            .value(),
    ));

    let shuffled = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!("action=reshuffle&count=3&csrf_token={token}"))
        .dispatch();
    assert_eq!(shuffled.status(), Status::SeeOther);
    assert_eq!(shuffled.headers().get_one("Location"), Some("/nojs"));
    drop(shuffled);
    let html = client.get("/nojs").dispatch().into_string().unwrap();
    let after = super::relays::read_relay_cookie(Some(
        client
            .cookies()
            .get(super::relays::RELAY_LIST_COOKIE)
            .unwrap()
            .value(),
    ));
    assert_eq!(after.pinned, vec!["wss://hand.example".to_string()]);
    assert_ne!(after.dealt, before.dealt);
    assert!(html.contains("name=\"count\" value=\"3\""));
    for url in after.pinned.iter().chain(after.dealt.iter()) {
        assert!(html.contains(&format!("data-relay=\"{url}\"")), "{url}");
    }
    let token = csrf_value(&html);
    let expected: Vec<String> = after
        .pinned
        .iter()
        .cloned()
        .chain(after.dealt.iter().cloned())
        .collect();

    set_shared_nojs_accepted(Some(vec!["wss://hand.example".to_string()]));
    note_nojs_publish_relays(&[]);
    let published = client
        .post("/nojs/create")
        .header(ContentType::Form)
        .body(format!(
            "title=Hello&content=hi&alias=Ada&csrf_token={token}"
        ))
        .dispatch();
    assert_eq!(published.status(), Status::SeeOther);
    assert_eq!(nojs_publish_relays_seen(), expected);

    let refused = client
        .post("/nojs/relays")
        .header(ContentType::Form)
        .body(format!(
            "action=add&relay=wss%3A%2F%2F127.0.0.1&csrf_token={token}"
        ))
        .dispatch();
    assert_eq!(refused.status(), Status::SeeOther);
    let location = refused.headers().get_one("Location").unwrap().to_string();
    assert!(location.contains("error=relay_invalid"), "{location}");
    assert!(
        location.contains("typed=wss%3A%2F%2F127.0.0.1"),
        "{location}"
    );
    drop(refused);
    let page = client.get(location).dispatch().into_string().unwrap();
    assert!(page.contains("Use a public wss:// relay URL."));
    assert!(page.contains("value=\"wss://127.0.0.1\""));
    assert!(page.contains("data-relay=\"wss://hand.example\""));
    assert!(!page.contains("data-relay=\"wss://127.0.0.1\""));
}

fn csrf_value(html: &str) -> String {
    let marker = "name=\"csrf_token\" value=\"";
    let start = html.find(marker).expect("csrf field") + marker.len();
    let end = html[start..].find('"').expect("csrf value");
    html[start..start + end].to_string()
}

#[test]
fn a_fixture_pool_renders_the_deal_controls_in_the_landing_sidebar() {
    let dir = tempfile::tempdir().unwrap();
    let _content = super::relays::RelayContentGuard::set(dir.path());
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    let urls: Vec<String> = (0..4)
        .map(|index| format!("wss://relay{index}.example"))
        .collect();
    let rows = urls
        .iter()
        .map(|url| crawler::Row {
            url: url.clone(),
            first_seen: 1,
            next_try: 1,
            failures: 0,
            publish_refused: false,
            visit: Some(crawler::Visit {
                at: 1,
                socket_answered: true,
                nip11: Some(document.clone()),
                probes: vec![crawler::ProbeResult {
                    name: "30323".to_string(),
                    accepted: true,
                    detail: "OK".to_string(),
                }],
            }),
        })
        .collect();
    crawler::save(&path, &crawler::Pool { rows }).unwrap();

    let mut config = crate::config::Config::default();
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let page = home_context(&config, false, None, None);
    let sidebar = page.get("relays_sidebar").unwrap();
    let help = page.get("relays_section").unwrap();
    assert!(sidebar.contains("id=\"sidebar-relays\""));
    for url in &urls[..3] {
        let row = format!("data-relay=\"{url}\"");
        assert!(sidebar.contains(&row), "{sidebar}");
        assert!(help.contains(&row), "{help}");
    }
    assert!(!sidebar.contains("data-relay=\"wss://relay3.example\""));
    assert!(!help.contains("wss://relay3.example"));
    assert!(sidebar.contains("class=\"relay-count\">3/6</span>"));
    assert!(sidebar.contains("aria-label=\"Reshuffle\""));
    assert!(sidebar.contains("class=\"relay-add-btn\">Add"));
    assert!(!sidebar.contains("relay-delete-all"));
    assert!(sidebar.contains("id=\"relay-candidates\""));
    assert!(sidebar.contains("wss://relay3.example"));
    assert!(!help.contains("relay-add-btn"));
    assert!(!help.contains("relay-url-input"));
    assert!(!help.contains("relay-reshuffle"));
    assert!(!help.contains("relay-delete-all"));
    assert!(!help.contains("relay-candidates"));
    assert!(!sidebar.contains("wss://config.example"));
    assert!(!help.contains("wss://config.example"));
}

fn region<'a>(html: &'a str, start: &str, end: &str) -> &'a str {
    let at = html
        .find(start)
        .unwrap_or_else(|| panic!("missing {start}"));
    let rest = &html[at..];
    let stop = rest.find(end).unwrap_or_else(|| panic!("missing {end}"));
    &rest[..stop]
}

#[test]
fn the_homepage_leaves_out_configured_names_and_the_pool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay-pool.json");
    let document: serde_json::Value =
        serde_json::from_str(include_str!("nip11/open.json")).unwrap();
    crawler::save(
        &path,
        &crawler::Pool {
            rows: vec![crawler::Row {
                url: "wss://pool.example".to_string(),
                first_seen: 1_700_000_000,
                next_try: 1_700_000_100,
                failures: 0,
                publish_refused: false,
                visit: Some(crawler::Visit {
                    at: 1_700_000_100,
                    socket_answered: true,
                    nip11: Some(document),
                    probes: vec![crawler::ProbeResult {
                        name: "30323".to_string(),
                        accepted: true,
                        detail: "OK".to_string(),
                    }],
                }),
            }],
        },
    )
    .unwrap();
    let usable = crate::crawl::usable_rows(&path);
    assert_eq!(usable.len(), 1);
    assert_eq!(usable[0].url, "wss://pool.example");

    let mut config = crate::config::Config::default();
    config.nostr.relays = vec!["wss://config.example".to_string()];
    for nojs in [false, true] {
        let page = home_context(&config, nojs, None, None);
        let relays: Vec<String> = serde_json::from_str(page.get("nostr_relays").unwrap()).unwrap();
        let sidebar = page.get("relays_sidebar").unwrap();
        let help = page.get("relays_section").unwrap();
        assert!(relays.is_empty(), "{nojs}");
        assert!(!sidebar.contains("wss://config.example"));
        assert!(!help.contains("wss://config.example"));
        if nojs {
            assert!(!help.contains("Config.toml"));
        }
        assert!(!sidebar.contains("wss://pool.example"));
        assert!(!help.contains("wss://pool.example"));
    }
    assert!(public_relays(&[]).is_empty());
    assert_eq!(
        public_relays(&["wss://link.example".to_string()]),
        vec!["wss://link.example".to_string()]
    );

    let home = include_str!("../src/pages/home.rs");
    let sealed = include_str!("../src/pages/sealed.rs");
    assert!(!home.contains("usable_rows"));
    assert!(!sealed.contains("usable_rows"));
    assert!(home.contains("config.nostr.relays"));
    assert!(!sealed.contains("config.nostr.relays"));
}

#[test]
fn tab_relay_timeout_is_at_least_ten_seconds() {
    assert_eq!(tab_relay_timeout_ms(0), 10_000);
    assert_eq!(tab_relay_timeout_ms(3), 10_000);
    assert_eq!(tab_relay_timeout_ms(20), 20_000);
}
#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_trims_the_title_and_alias_and_keeps_the_body() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    let body = "\n  hi  ";
    let form = NewPost {
        title: "  Hello World  ".to_string(),
        content: body.to_string(),
        alias: "  Ada  ".to_string(),
        csrf_token: String::new(),
    };

    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://nojs-seal.invalid".to_string()];
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    let mut padded = form.clone();
    padded.title = format!(" {} ", "x".repeat(config.limits.title_max_length));
    padded.alias = "Ada".to_string();
    padded.content = "hi".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&padded, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    padded.title = "Hello".to_string();
    padded.alias = format!(" {} ", "a".repeat(config.limits.alias_max_length));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&padded, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_rejects_a_private_cookie_and_keeps_field_checks_first() {
    let _holds = HoldDir::new();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let mut form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };

    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    let private = pinned_relays(&["wss://10.0.0.1", "wss://127.0.0.1"]);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&private));
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = " \n ".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&private));
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    config.security.csrf_protection_enabled = true;
    form.content = "hi".to_string();
    form.csrf_token = "not-a-token".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&private));
    assert_eq!(href, "/nojs?error=csrf_token_invalid");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    form.csrf_token.clear();
    let public = pinned_relays(&["wss://nojs-seal.invalid"]);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&public));
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_builds_the_public_relay_list_and_fails_when_it_is_empty() {
    let saved = vec![
        "wss://pin.example".to_string(),
        "wss://127.0.0.1".to_string(),
        "wss://dealt.example".to_string(),
    ];
    assert_eq!(
        nojs_publish_relays(&saved).unwrap(),
        vec![
            "wss://pin.example".to_string(),
            "wss://dealt.example".to_string()
        ]
    );
    assert_eq!(
        nojs_publish_relays(&[
            "wss://Pin.Example/".to_string(),
            "wss://pin.example".to_string(),
            "wss://dealt.example".to_string(),
        ])
        .unwrap(),
        vec![
            "wss://pin.example".to_string(),
            "wss://dealt.example".to_string()
        ]
    );

    let mut many = Vec::new();
    for index in 0..crate::nostr::MAX_FETCH_RELAYS + 2 {
        many.push(format!("wss://saved{index}.example"));
    }
    let published = nojs_publish_relays(&many).unwrap();
    assert_eq!(published.len(), crate::nostr::MAX_FETCH_RELAYS);
    assert_eq!(published[0], "wss://saved0.example");
    assert!(!published.iter().any(|relay| relay.contains("saved6")));
    assert!(nojs_publish_relays(&["wss://127.0.0.1".to_string()]).is_none());
    assert!(nojs_publish_relays(&[]).is_none());

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let mut form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
    };
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];

    form.content = " \n ".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = "hi".to_string();
    let private = pinned_relays(&["wss://10.0.0.1"]);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&private));
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_publish_relay_list_equals_the_cookie() {
    struct ClearPublish;
    impl Drop for ClearPublish {
        fn drop(&mut self) {
            set_shared_nojs_accepted(None);
            note_nojs_publish_relays(&[]);
        }
    }
    let _clear = ClearPublish;

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://config.example".to_string()];
    let saved = super::relays::write_relay_cookie(&super::relays::RelayCookie {
        dealt: vec![
            "wss://dealt.example".to_string(),
            "wss://other.example".to_string(),
        ],
        pinned: vec!["wss://pin.example".to_string()],
    });
    let list = super::relays::saved_relay_cookie(Some(&saved)).expect("relay cookie");
    let expected: Vec<String> = list.pinned.into_iter().chain(list.dealt).collect();
    set_shared_nojs_accepted(Some(expected.clone()));
    note_nojs_publish_relays(&[]);

    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::nojs_create_post]),
    )
    .unwrap();
    let published = client
        .post("/nojs/create")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved.clone()))
        .header(ContentType::Form)
        .body("title=Hello&content=hi&alias=Ada&csrf_token=")
        .dispatch();
    assert_eq!(published.status(), Status::SeeOther);
    assert_eq!(nojs_publish_relays_seen(), expected);

    let with_extra = client
        .post("/nojs/create")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved))
        .header(ContentType::Form)
        .body("title=Hello&content=hi&alias=Ada&csrf_token=&relay=wss%3A%2F%2Fhand.example")
        .dispatch();
    assert_eq!(with_extra.status(), Status::SeeOther);
    assert_eq!(nojs_publish_relays_seen(), expected);

    note_nojs_publish_relays(&[]);
    let missed = client
        .post("/nojs/create")
        .header(ContentType::Form)
        .body("title=Hello&content=hi&alias=Ada&csrf_token=")
        .dispatch();
    assert_eq!(missed.status(), Status::Ok);
    let missed_html = missed.into_string().unwrap();
    assert!(missed_html.contains("Publishing failed. Try again."));
    let missed_help = region(
        &missed_html,
        "<div class=\"help-section\" id=\"relays\">",
        "</div>",
    );
    let missed_sidebar = region(&missed_html, "id=\"sidebar-relays\"", "class=\"footer\"");
    assert!(missed_help.contains("No relays."));
    assert!(missed_sidebar.contains("No relays."));
    assert!(!missed_help.contains("config.example"));
    assert!(!missed_sidebar.contains("config.example"));
    assert!(nojs_publish_relays_seen().is_empty());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn an_empty_saved_list_sends_no_relays() {
    let home = include_str!("../templates/home.js");
    let urls_at = home
        .find("function savedRelayUrls(")
        .expect("saved relay urls");
    let urls_end = home[urls_at..]
        .find("\nfunction paintSavedRelayLists(")
        .map(|offset| urls_at + offset)
        .expect("paint saved lists");
    let publish_at = home
        .find("globalThis.nostorgraphPublishPublicNote")
        .expect("publish helper");
    let publish_end = home[publish_at..]
        .find("\n            async function loadNostr(")
        .map(|offset| publish_at + offset)
        .expect("load nostr");
    let ready_at = home
        .find("function hasAnyPublishRelay()")
        .expect("publish gate");
    let ready_end = home[ready_at..]
        .find("\n            globalThis.nostorgraphPublishPublicNote")
        .map(|offset| ready_at + offset)
        .expect("publish helper follows the gate");
    let mut program = home[urls_at..urls_end].to_string();
    program.push_str(
        r##"
const localStorage = {
    getItem(key) {
        if (key === "nostorgraph_relays") {
            return JSON.stringify({ pinned: [], dealt: [] });
        }
        return null;
    },
    setItem() {},
};
const form = {
    dataset: {
        timeout: "10000",
        sealMax: "100",
        relays: JSON.stringify([
            "wss://relay.primal.net",
            "wss://relay.snort.social",
            "wss://offchain.pub",
        ]),
    },
    csrf_token: { value: "token" },
};
let sent = null;
async function loadNostr() {
    return {
        publishPublicNote(fields) {
            sent = fields.relays;
            return {};
        },
    };
}
"##,
    );
    program.push_str(&home[ready_at..ready_end]);
    program.push_str(&home[publish_at..publish_end]);
    program.push_str(
        r##"
(async () => {
    if (hasAnyPublishRelay()) {
        throw new Error("an empty saved list should hide publish");
    }
    await nostorgraphPublishPublicNote({ title: "Hello", author: "Ada", content: "hi" });
    if (JSON.stringify(sent) !== "[]") {
        throw new Error(JSON.stringify(sent));
    }
})();
"##,
    );
    run_javascript("nostorgraph-empty-saved", &program);

    let note = crate::nostr::sign_v2_note("0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec![
        "wss://relay.primal.net".to_string(),
        "wss://relay.snort.social".to_string(),
        "wss://offchain.pub".to_string(),
    ];
    let body = NostrPublishBody {
        event,
        relays: Vec::new(),
        seal_claim: String::new(),
        csrf_token: String::new(),
    };
    assert!(note_and_relays_for_publish(&body, &config, "").is_none());

    struct ClearPublish;
    impl Drop for ClearPublish {
        fn drop(&mut self) {
            set_shared_nojs_accepted(None);
            note_nojs_publish_relays(&[]);
        }
    }
    let _clear = ClearPublish;
    set_shared_nojs_accepted(Some(config.nostr.relays.clone()));
    note_nojs_publish_relays(&[]);
    let saved = super::relays::write_relay_cookie(&super::relays::RelayCookie {
        dealt: Vec::new(),
        pinned: Vec::new(),
    });
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::nojs_create_post]),
    )
    .unwrap();
    let published = client
        .post("/nojs/create")
        .cookie((super::relays::RELAY_LIST_COOKIE, saved))
        .header(ContentType::Form)
        .body("title=Hello&content=hi&alias=Ada&csrf_token=")
        .dispatch();
    assert_eq!(published.status(), Status::Ok);
    let html = published.into_string().unwrap();
    assert!(html.contains("Publishing failed. Try again."));
    assert!(nojs_publish_relays_seen().is_empty());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_seals_and_publishes_inside_the_blocking_task() {
    let holds = HoldDir::new();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let form = NewPost {
        title: "  Hello World  ".to_string(),
        content: "  hi  ".to_string(),
        alias: "  Ada  ".to_string(),
        csrf_token: String::new(),
    };
    let relays = pinned_relays(&["wss://nojs-seal.invalid"]);
    let published = match publish_nojs_note_with_cookie(&form, &config, Some(&relays)) {
        Ok(published) => published,
        Err(error) => panic!("seal and publish did not run: {error}"),
    };
    assert!(crate::nostr::sealed_locator(&published.locator));
    assert_ne!(published.payload, form.content);
    assert!(published.payload.len() > form.content.len());
    assert!(published.accepted.is_empty());
    assert_eq!(hold_dir_count(holds.path()), 0);
    assert!(published.k.len() > 1);
    assert!(!published.k.contains('='));
    let mut accepted = published;
    let key = accepted.k.clone();
    accepted.accepted = vec!["wss://nojs-seal.invalid".to_string()];
    assert_eq!(
        nojs_sealed_location(&accepted).as_deref(),
        Some(
            format!(
                "/{}/{}?r=wss%3A%2F%2Fnojs-seal.invalid#{}",
                accepted.locator, accepted.event_id, key
            )
            .as_str()
        )
    );
    accepted.accepted = vec!["wss://127.0.0.1".to_string()];
    assert!(nojs_sealed_location(&accepted).is_none());
    accepted.accepted = vec!["wss://nojs-seal.invalid".to_string()];
    accepted.k.clear();
    assert!(nojs_sealed_location(&accepted).is_none());
    accepted.k = key;
    accepted.accepted = vec!["wss://Relay.Primal.net/".to_string()];
    assert_eq!(
        nojs_sealed_location(&accepted).as_deref(),
        Some(
            format!(
                "/{}/{}?r=wss%3A%2F%2Frelay.primal.net#{}",
                accepted.locator, accepted.event_id, accepted.k
            )
            .as_str()
        )
    );
    accepted.accepted = vec![
        "wss://relay.primal.net".to_string(),
        "wss://nojs-seal.invalid".to_string(),
    ];
    assert_eq!(
        nojs_sealed_location(&accepted).as_deref(),
        Some(
            format!(
                "/{}/{}?r=wss%3A%2F%2Frelay.primal.net&r=wss%3A%2F%2Fnojs-seal.invalid#{}",
                accepted.locator, accepted.event_id, accepted.k
            )
            .as_str()
        )
    );

    let temp = tempfile::tempdir().unwrap();
    let href = create_location_with_cookie(&form, &config, "", Some(&relays));
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    let started = std::time::Instant::now();
    let temp = tempfile::tempdir().unwrap();
    let mut rejected = form.clone();
    rejected.content = " \n ".to_string();
    let href = create_location(&rejected, &config, "");
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "validation waited on a relay: {:?}",
        started.elapsed()
    );
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_takes_a_public_publish_slot_until_it_finishes() {
    let _holds = HoldDir::new();
    let mut readers = Vec::new();
    for _ in 0..MAX_PUBLIC_FETCHES {
        readers.push(PublicFetchSlot::acquire().expect("public fetch slot"));
    }
    drop(nojs_publish_slot().expect("publish has its own slots"));
    drop(readers);
    let mut held = Vec::new();
    for _ in 0..MAX_PUBLIC_PUBLISHES {
        held.push(PublicPublishSlot::acquire().expect("public publish slot"));
    }
    let busy = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(
            nojs_publish_slot().err(),
            Some("/nojs?error=nostr_publish_failed")
        );
        let mut config = {
            let mut config = crate::config::Config::default();
            config.security.csrf_protection_enabled = false;
            config
        };
        let form = NewPost {
            title: "Hello World".to_string(),
            content: "hi".to_string(),
            alias: "Ada".to_string(),
            csrf_token: String::new(),
        };
        config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
        let blocked = tempfile::tempdir().unwrap();
        let href = create_location(&form, &config, "");
        assert_eq!(href, "/nojs?error=nostr_publish_failed");
        assert!(std::fs::read_dir(blocked.path().join("content")).is_err());
        assert!(nojs_publish_slot().is_err());

        config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
        let relays = pinned_relays(&["wss://relay.primal.net"]);
        struct ResetAccepted;
        impl Drop for ResetAccepted {
            fn drop(&mut self) {
                set_nojs_test_accepted(None);
            }
        }
        let _reset_accepted = ResetAccepted;
        set_nojs_test_accepted(Some(vec!["wss://relay.primal.net".to_string()]));
        let href = create_location_with_cookie(&form, &config, "", Some(&relays));
        assert_eq!(href, "/nojs?error=nostr_publish_failed");
        assert!(nojs_publish_slot().is_err(), "sealing took a publish slot");

        held.pop();
        let href = create_location_with_cookie(&form, &config, "", Some(&relays));
        let link = seal_link_parts(&href);
        assert_eq!(link.locator.len(), nostorgraph_nip44::LOCATOR_LEN, "{href}");
        assert!(link.event_id.is_some(), "{href}");
        let released = nojs_publish_slot().expect("publish released the publish slot");
        drop(released);
    }));
    drop(held);
    busy.unwrap();
}
