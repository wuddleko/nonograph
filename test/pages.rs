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
        WRITEMARK_CSS_PATH,
        NOTICE_CSS_PATH,
        THEME_JS_PATH,
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
    assert!(scripts.contains("/writemark.js?v="));
    assert!(scripts.contains("/home.js?v="));
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
    assert!(loader.contains("./page/nonograph_page.js"));
    assert!(loader.contains("./page/nonograph_page_bg.wasm"));
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
                    super::nojs_add_relay,
                    super::assign_tor_circuits,
                ],
            ),
    )
    .unwrap();

    let first = client.get("/").dispatch();
    assert_eq!(first.status(), Status::Ok);
    let header = first.headers().get_one("Set-Cookie").unwrap().to_string();
    assert!(header.starts_with("nonograph_csrf="));
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
    assert!(again.headers().get_one("Set-Cookie").is_none());
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
        .post("/nojs/relay")
        .header(ContentType::Form)
        .body(format!(
            "title=Hello&content=Body&alias=&relay_box=wss://csrf-check.example&csrf_token={token}"
        ))
        .dispatch();
    assert_eq!(added.status(), Status::Ok);
    let added_body = added.into_string().unwrap();
    assert!(added_body.contains("data-relay=\"wss://csrf-check.example\""));
    assert!(!added_body.contains("The publish form expired. Try again."));

    let refused = client
        .post("/nojs/relay")
        .header(ContentType::Form)
        .body(format!(
            "title=Hello&content=Body&alias=&relay_box=wss://csrf-other.example&csrf_token={foreign}"
        ))
        .dispatch();
    assert_eq!(refused.status(), Status::Ok);
    let refused_body = refused.into_string().unwrap();
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
        .cookie(("nonograph_csrf", "not-a-token"))
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
    assert!(open_post.headers().get_one("Set-Cookie").is_none());
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
    assert!(js.contains("stored only in this browser"));
    assert!(!js.contains("class=\"nostr-publish\""));
    assert_eq!(js.matches(">Publish</button>").count(), 2);
    assert!(js.contains("type=\"submit\""));
    assert!(nojs.contains("This host seals the note and sends it to these relays over Tor."));
    assert!(nojs.contains("Add one more with the form."));
    assert!(nojs.contains("kept with this draft"));
    assert!(!nojs.contains("signs in your browser"));
    assert!(!nojs.contains("stored only in this browser"));
    assert!(nojs.contains("id=\"sidebar-relays\""));
    assert!(nojs.contains("href=\"#sidebar-relays\""));
    assert!(nojs.contains("class=\"help-link relays-open-bar\""));
    assert!(nojs.contains("class=\"nojs\""));
    assert!(nojs.contains("This host seals the note and does not keep a copy."));
    assert!(nojs.contains("The key stays in the address."));
    assert!(nojs.contains(
        "The host seals the note and does not keep a copy. The writing is ordinary markup, not a proprietary format."
    ));
    assert!(!nojs.contains("stored as plaintext markup"));
    assert!(!js.contains("stored as plaintext markup"));
    assert!(js.contains(
        "The browser seals the note and this host does not keep a copy. The writing is ordinary markup, not a proprietary format."
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
    assert!(nojs_relay.contains("name=\"relay_box\""));
    assert!(nojs_relay.contains("form=\"publishForm\""));
    assert!(!nojs_relay
        .replace("name=\"relay_box\"", "")
        .contains("name=\"relay\""));
    assert!(nojs.contains("<input type=\"hidden\" name=\"relay\" value=\"\" />"));
    let nojs_help_at = nojs
        .find("<div class=\"help-section\" id=\"relays\">")
        .unwrap();
    let nojs_help = &nojs[nojs_help_at..];
    let nojs_help = &nojs_help[..nojs_help.find("</div>").unwrap()];
    assert!(nojs_help.contains("relay.primal.net"));
    assert!(!nojs_help.contains("relay-add-btn"));
    assert!(!nojs_help.contains("relay-url-input"));
    assert!(!nojs_help.contains("signs in your browser"));
    let css = include_str!("../templates/home.css");
    let tablet = css.find("@media (max-width: 768px)").unwrap();
    let mobile = css.find("@media (max-width: 480px)").unwrap();
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

#[test]
fn nojs_publish_submits_the_relay_and_add_posts_it() {
    let config = crate::config::Config::default();
    let engine = crate::template::TemplateEngine::new("templates");
    let nojs = engine
        .render("home", &home_context(&config, true, None, None))
        .unwrap();
    assert!(nojs.contains("action=\"/nojs/create\""));
    assert_eq!(
        nojs.matches("<button type=\"submit\" form=\"publishForm\">Publish</button>")
            .count(),
        2
    );
    let relay = relay_url_input_after(&nojs, "sidebar-relays");
    assert!(relay.contains("name=\"relay_box\""));
    assert!(relay.contains("form=\"publishForm\""));
    let publish = nojs
        .find("<button type=\"submit\" form=\"publishForm\">Publish</button>")
        .unwrap();
    let input = nojs.find("class=\"relay-url-input\"").unwrap();
    assert!(publish < input);
    let first_submit = nojs.find("<button type=\"submit\"").unwrap();
    assert!(nojs[first_submit..]
        .starts_with("<button type=\"submit\" form=\"publishForm\">Publish</button>"));
    assert!(!nojs.contains("relay-add-default"));
    assert!(!nojs[first_submit..first_submit + 80].contains("formaction"));
    let add = tag_after(
        &nojs,
        "sidebar-relays",
        "<button type=\"submit\" class=\"relay-add-btn\"",
    );
    assert!(add.contains("formaction=\"/nojs/relay\""));
    assert!(add.contains("form=\"publishForm\""));
    assert!(add.contains("formnovalidate"));
    assert!(!add.contains("name="));
    assert!(publish < nojs.find("relay-add-btn").unwrap());
    assert!(!nojs.contains("<script"));
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
    assert!(js.contains("/home.js?v="));
    assert!(!js.contains("relay-add-default"));
    assert!(!include_str!("../templates/home.css").contains("relay-add-default"));
    let home_js = include_str!("../templates/home.js");
    let listener = home_js.find(".relay-add-btn").unwrap();
    let click = home_js[listener..].find("addEventListener").unwrap();
    let add_relay = home_js.find("function addRelayFromBlock(").unwrap();
    assert!(add_relay < listener + click);
}

#[test]
fn nojs_add_relay_lists_it_and_keeps_the_draft() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
    let mut form = NewPost {
        title: "Hello <World>".to_string(),
        content: "# ici\n\nhey".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
        relay: String::new(),
        relay_box: "  wss://damus.io  ".to_string(),
        relay_remove: false,
    };
    let html = render_nojs_draft(&config, &form, None, "").0;
    let form_html = publish_form(&html);
    let sidebar = &html[html.find("class=\"sidebar-relays\"").unwrap()..];
    assert!(sidebar.contains("<li data-relay=\"wss://damus.io\">"));
    assert!(sidebar.contains("<span class=\"relay-name\">damus.io</span>"));
    assert!(sidebar.contains("name=\"relay_remove\""));
    assert!(form_html.contains("<input type=\"hidden\" name=\"relay\" value=\"wss://damus.io\" />"));
    assert!(!relay_url_input_after(&html, "sidebar-relays").contains("value="));
    assert!(!sidebar.contains("relay-list-user\" hidden"));
    assert!(html.contains("Hello &lt;World&gt;"));
    assert!(html.contains("# ici"));
    assert!(html.contains("hey"));
    assert!(html.contains("value=\"Ada\""));
    assert!(html.contains("<textarea name=\"content\""));
    assert!(!html.contains("writemark-editor"));
    assert!(!html.contains("<script"));

    form.title.clear();
    form.content.clear();
    form.relay.clear();
    form.relay_box = "wss://relay.damus.io".to_string();
    let bare = render_nojs_draft(&config, &form, None, "").0;
    assert!(bare.contains("<li data-relay=\"wss://relay.damus.io\">"));
    assert!(publish_form(&bare)
        .contains("<input type=\"hidden\" name=\"relay\" value=\"wss://relay.damus.io\" />"));
    assert!(!bare.contains("A title is required."));
    assert!(!bare.contains("Write something before publishing."));

    form.title = "Hello <World>".to_string();
    form.content = "# ici\n\nhey".to_string();
    form.relay = "wss://relay.damus.io".to_string();
    form.relay_box = "wss://nos.lol".to_string();
    let replaced = render_nojs_draft(&config, &form, None, "").0;
    assert!(replaced.contains("<li data-relay=\"wss://nos.lol\">"));
    assert!(!replaced.contains("data-relay=\"wss://relay.damus.io\""));
    assert!(publish_form(&replaced)
        .contains("<input type=\"hidden\" name=\"relay\" value=\"wss://nos.lol\" />"));
    assert!(!relay_url_input_after(&replaced, "sidebar-relays").contains("value="));

    form.relay = "wss://nos.lol".to_string();
    form.relay_box = "wss://127.0.0.1".to_string();
    let rejected = render_nojs_draft(&config, &form, None, "").0;
    let rejected_sidebar = &rejected[rejected.find("class=\"sidebar-relays\"").unwrap()..];
    assert_eq!(
        relay_field_error(&rejected),
        "Use a public wss:// relay URL."
    );
    assert_eq!(form_error_text(&rejected), "");
    assert!(rejected_sidebar.contains("value=\"wss://127.0.0.1\""));
    assert!(rejected.contains("# ici"));
    assert!(rejected_sidebar.contains("<li data-relay=\"wss://nos.lol\">"));
    assert!(!rejected_sidebar.contains("<li data-relay=\"wss://127.0.0.1\">"));

    form.relay_box = "wss://relay.primal.net".to_string();
    let configured = render_nojs_draft(&config, &form, None, "").0;
    let configured_sidebar = &configured[configured.find("class=\"sidebar-relays\"").unwrap()..];
    assert_eq!(
        relay_field_error(&configured),
        "That relay is already configured on this site."
    );
    assert_eq!(form_error_text(&configured), "");
    assert!(configured_sidebar.contains("value=\"wss://relay.primal.net\""));
    assert!(configured_sidebar.contains("<li data-relay=\"wss://nos.lol\">"));

    form.relay_box = "wss://Relay.Primal.net/".to_string();
    let spelled = render_nojs_draft(&config, &form, None, "").0;
    assert_eq!(
        relay_field_error(&spelled),
        "That relay is already configured on this site."
    );
    assert_eq!(form_error_text(&spelled), "");
    assert!(spelled.contains("value=\"wss://Relay.Primal.net/\""));
    assert!(spelled.contains("<li data-relay=\"wss://nos.lol\">"));

    form.relay.clear();
    form.relay_box = "wss://Damus.io/".to_string();
    let canonical = render_nojs_draft(&config, &form, None, "").0;
    assert!(publish_form(&canonical)
        .contains("<input type=\"hidden\" name=\"relay\" value=\"wss://damus.io\" />"));
    assert!(canonical.contains("<li data-relay=\"wss://damus.io\">"));
    assert!(!relay_url_input_after(&canonical, "sidebar-relays").contains("value="));
    form.relay = "wss://nos.lol".to_string();

    form.relay_box.clear();
    let empty = render_nojs_draft(&config, &form, None, "").0;
    assert_eq!(relay_field_error(&empty), "Enter a relay URL.");
    assert_eq!(form_error_text(&empty), "");
    assert!(empty.contains("# ici"));
    assert!(empty.contains("<li data-relay=\"wss://nos.lol\">"));

    form.relay_remove = true;
    let removed = render_nojs_draft(&config, &form, None, "").0;
    let removed_sidebar = &removed[removed.find("class=\"sidebar-relays\"").unwrap()..];
    assert!(removed_sidebar.contains("relay-list-user\" hidden"));
    assert!(publish_form(&removed).contains("<input type=\"hidden\" name=\"relay\" value=\"\" />"));
    assert!(removed.contains("# ici"));
    assert!(!removed.contains("Enter a relay URL."));

    config.security.csrf_protection_enabled = true;
    form.relay = "wss://nos.lol".to_string();
    form.relay_box = "wss://damus.io".to_string();
    form.relay_remove = false;
    form.csrf_token = "not-a-token".to_string();
    let expired = render_nojs_draft(&config, &form, None, "").0;
    let expired_sidebar = &expired[expired.find("class=\"sidebar-relays\"").unwrap()..];
    assert!(expired.contains("The publish form expired. Try again."));
    assert!(expired.contains("# ici"));
    assert!(expired_sidebar.contains("<li data-relay=\"wss://nos.lol\">"));
    assert!(expired_sidebar.contains("value=\"wss://damus.io\""));
    assert!(!expired_sidebar.contains("<li data-relay=\"wss://damus.io\">"));
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

fn publish_form(html: &str) -> &str {
    let start = html.find("id=\"publishForm\"").unwrap();
    let end = start + html[start..].find("</form>").unwrap();
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
fn public_origin_is_optional_and_a_bad_one_is_fatal() {
    let engine = crate::template::TemplateEngine::new("templates");
    let config = crate::config::Config::default();
    let home = engine
        .render("home", &home_context(&config, false, None, None))
        .unwrap();
    assert!(home.contains("rel=\"canonical\" href=\"/\""));
    assert!(home.contains("property=\"og:url\" content=\"/\""));
    assert!(!home.contains("nonograph.com"));

    let mut named = crate::config::Config::default();
    named.server.public_origin = "https://example.com".to_string();
    let home = engine
        .render("home", &home_context(&named, false, None, None))
        .unwrap();
    assert!(home.contains("rel=\"canonical\" href=\"https://example.com/\""));
    assert!(home.contains("property=\"og:url\" content=\"https://example.com/\""));
    assert!(!home.contains("nonograph.com"));
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
    assert!(js.contains("<script src=\"/theme.js?v="));
    assert!(include_str!("../templates/theme.js").contains("nonograph-theme"));
    assert!(!js.contains("&lt;script"));
    let nojs = engine
        .render("home", &home_context(&config, true, None, None))
        .unwrap();
    assert!(nojs.contains("class=\"theme-toggle\""));
    assert!(!nojs.contains("theme.js"));
    assert!(!nojs.contains("<script"));

    let post = engine.render("post", &sample_post_context(false)).unwrap();
    assert!(post.contains("<script src=\"/theme.js?v="));
    let post_nojs = engine.render("post", &sample_post_context(true)).unwrap();
    assert!(post_nojs.contains("class=\"theme-toggle\""));
    assert!(!post_nojs.contains("<script"));

    let event_id = "ab".repeat(32);
    let opened = sealed_shell("{}", "0123456789abcdef0123", &event_id, &[], "", &config);
    assert!(opened.contains("<script src=\"/theme.js?v="));
    assert!(!opened.contains("</script><script"));
    let locked = sealed_shell("", "0123456789abcdef0123", &event_id, &[], "", &config);
    assert!(locked.contains("class=\"theme-toggle\""));
    assert!(!locked.contains("<script"));
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
    let event_id = "ab".repeat(32);
    let html = sealed_shell(&event, "0123456789abcdef0123", &event_id, &[], "", &config);
    assert!(html.contains("<title>Sealed note</title>"));
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
    assert!(html.contains("<div id=\"sealed-event\" hidden>"));
    assert!(!html.contains("<script type=\"application/json\""));
    assert!(html.contains("<script type=\"module\" src=\"/nostr.js?v="));
    assert!(html.contains("id=\"sealed-copy\">Copy link</button>"));
    assert!(html.contains("id=\"sealed-open\""));
    assert!(html.contains("id=\"sealed-key\""));
    assert!(html.contains(">Open</button>"));
    assert!(html.contains("The key is the part of the address after #."));
    assert!(html.contains(&format!("action=\"/s/0123456789abcdef0123/{event_id}\"")));
    assert!(!html.contains("/nojs/s/"));
    assert!(!html.contains("?k="));
    assert!(html.contains(">legal</a>"));
    assert!(html.contains(">api</a>"));
    assert!(html.contains(">source code</a>"));
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
    assert!(!draw_body.contains("location.hash"));
    assert!(!draw_body.contains("event.content"));
    assert!(PAGE_JS.contains(
        "export function render_markdown(markdown, syntax_theme, max_url_length, external_link_security)"
    ));
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
    assert!(load_body.contains("prefillSealedKey()"));
    assert!(load_body.contains("secretFromFragment(location.hash)"));
    let hidden = load_body.find("form.hidden = true").unwrap();
    let opened = load_body
        .find("openEmbeddedSeal(sealedEvent.textContent)")
        .unwrap();
    assert!(hidden < opened);
    assert!(load_body.contains("form.hidden = false"));
    assert!(load_body.contains("event.preventDefault()"));
    assert!(load_body.contains("openEmbeddedSeal(sealedEvent.textContent)"));
    assert!(!load_body.contains("linkNojsFromHash"));
    assert!(!nostr.contains("?k="));
    assert!(!nostr.contains("/nojs/s/"));
}

#[test]
fn nojs_sealed_page_says_the_note_opens_in_the_browser() {
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let key = "KkKeyValue99";
    let config = crate::config::Config::default();
    let html = wait(nojs_sealed_response(
        locator,
        &event_id,
        Some(key),
        &[],
        &config,
    ))
    .unwrap()
    .0;
    assert!(html.contains("id=\"sealed-open\""));
    assert!(html.contains("The key does not open this note."));
    assert!(html.contains(&format!("action=\"/s/{locator}/{event_id}\"")));
    assert!(!html.contains(key));
    assert!(!html.contains("<script"));
    assert!(!html.contains("sealed-event"));
    assert!(!html.contains("nostr.js"));
    assert!(!html.contains("render_markdown"));
    assert!(
        wait(nojs_sealed_response(locator, &event_id, None, &[], &config))
            .unwrap()
            .0
            .contains("The key does not open this note.")
    );
    match wait(nojs_sealed_response(
        "not-a-locator",
        &event_id,
        Some(key),
        &[],
        &config,
    )) {
        Err((status, rocket::Either::Right(page))) if status == Status::NotFound => {
            assert_eq!(page.0, not_found_html());
            assert!(!page.0.contains(key));
            assert!(!page.0.contains("does not decrypt"));
        }
        Ok(_) => panic!("a bad locator returned a page"),
        Err((status, _)) => panic!("a bad locator returned {status}"),
    }
    match wait(nojs_sealed_response(
        "0123456789ABCDEF0123",
        &event_id,
        Some(key),
        &[],
        &config,
    )) {
        Err((status, _)) if status == Status::NotFound => {}
        Ok(_) => panic!("an uppercase locator returned a page"),
        Err((status, _)) => panic!("an uppercase locator returned {status}"),
    }
}

#[test]
fn nojs_query_secret_matches_secret_from_fragment_and_a_failed_open_is_refused() {
    assert!(nojs_query_secret(None).is_none());
    assert!(nojs_query_secret(Some("")).is_none());
    assert!(nojs_query_secret(Some("AA")).is_none());
    assert!(nojs_query_secret(Some(&"A".repeat(42))).is_none());
    assert!(nojs_query_secret(Some(&"A".repeat(44))).is_none());
    assert!(nojs_query_secret(Some(&format!("{}=", "A".repeat(43)))).is_none());
    assert!(nojs_query_secret(Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA+")).is_none());
    assert!(nojs_query_secret(Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/")).is_none());
    assert!(nojs_query_secret(Some("#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")).is_none());
    assert_eq!(
        nojs_query_secret(Some(&"A".repeat(43))).unwrap(),
        [0u8; nonograph_nip44::SECRET_LEN]
    );

    let sealed = crate::seal::seal_and_sign(
        "unique title text",
        "Ada",
        "unique body text",
        128,
        32,
        256_000,
    )
    .unwrap();
    let event_id = sealed.note.id_hex();
    let event = sealed.note.event_json.clone();
    let payload = serde_json::from_str::<serde_json::Value>(&event).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    let zeros = "A".repeat(43);
    let config = crate::config::Config::default();
    let refused = nojs_opened_page(
        &sealed.locator,
        &event_id,
        &nojs_query_secret(Some(&zeros)).unwrap(),
        &event,
        &zeros,
        &[],
        &config,
    );
    assert!(refused.contains("This page does not decrypt it."));
    assert!(!refused.contains("unique title text"));
    assert!(!refused.contains("unique body text"));
    assert!(!refused.contains(&zeros));
    assert!(!refused.contains(&sealed.k));
    assert!(!refused.contains(&payload));

    let opened = nojs_opened_page(
        &sealed.locator,
        &event_id,
        &nojs_query_secret(Some(&sealed.k)).unwrap(),
        &event,
        &sealed.k,
        &[],
        &config,
    );
    assert!(opened.contains("<h1>unique title text</h1>"));
    assert!(opened.contains("unique body text"));
    assert!(!opened.contains("This page does not decrypt it."));
    assert!(!opened.contains(&payload));

    let missing = wait(nojs_sealed_response(
        &sealed.locator,
        &event_id,
        None,
        &[],
        &config,
    ))
    .unwrap()
    .0;
    assert!(missing.contains("The key does not open this note."));
    assert!(missing.contains("id=\"sealed-open\""));
    assert!(!missing.contains("unique body text"));
    assert!(!missing.contains(&payload));
    match wait(nojs_sealed_response(
        "not-a-locator",
        &event_id,
        Some(&sealed.k),
        &[],
        &config,
    )) {
        Err((status, rocket::Either::Right(page))) if status == Status::NotFound => {
            assert!(!page.0.contains("unique body text"));
            assert!(!page.0.contains(&payload));
            assert!(!page.0.contains(&sealed.k));
        }
        Ok(_) => panic!("a bad locator returned a page"),
        Err((status, _)) => panic!("a bad locator returned {status}"),
    }
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

    let refused = client
        .get(format!("/nojs/s/0123456789abcdef0123/{event_id}"))
        .dispatch();
    assert_eq!(refused.status(), Status::SeeOther);
    sealed_route_headers(&refused);
    let location = format!("/s/0123456789abcdef0123/{event_id}");
    assert_eq!(
        refused.headers().get_one("Location"),
        Some(location.as_str())
    );
    let refused_body = refused.into_string().unwrap_or_default();
    assert!(!refused_body.contains("?k="));
    assert!(!refused_body.contains("does not decrypt"));
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
fn nojs_sealed_address_redirects_to_the_one_link() {
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(crate::config::Config::default())
            .mount("/", routes![super::nojs_sealed_view]),
    )
    .unwrap();
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let key = "A".repeat(43);
    let with_key = format!("/s/{locator}/{event_id}?r=wss%3A%2F%2Fhint.example#{key}");
    let moved = client
        .get(format!(
            "/nojs/s/{locator}/{event_id}?k={key}&r=wss://hint.example"
        ))
        .dispatch();
    assert_eq!(moved.status(), Status::SeeOther);
    assert_eq!(moved.headers().get_one("Location"), Some(with_key.as_str()));
    let without_key = format!("/s/{locator}/{event_id}");
    let plain = client
        .get(format!("/nojs/s/{locator}/{event_id}?k=not-a-secret"))
        .dispatch();
    assert_eq!(plain.status(), Status::SeeOther);
    assert_eq!(
        plain.headers().get_one("Location"),
        Some(without_key.as_str())
    );
    let missing = client
        .get(format!("/nojs/s/not-a-locator/{event_id}?k={key}"))
        .dispatch();
    assert_eq!(missing.status(), Status::NotFound);
    assert!(missing.headers().get_one("Location").is_none());
}

fn wait<T>(future: impl std::future::Future<Output = T>) -> T {
    rocket::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_sealed_fetch_uses_a_slot_and_a_missing_event_is_not_found() {
    let locator = "0123456789abcdef0123";
    let event_id = "ab".repeat(32);
    let key = "A".repeat(43);
    let mut config = crate::config::Config::default();
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://nojs-seal.invalid".to_string()];

    let mut held = Vec::new();
    for _ in 0..MAX_PUBLIC_FETCHES {
        held.push(PublicFetchSlot::acquire().expect("public fetch slot"));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let refused = wait(nojs_sealed_response(
            locator,
            &event_id,
            Some("AA"),
            &[],
            &config,
        ))
        .unwrap();
        assert!(refused.0.contains("The key does not open this note."));
        assert!(refused.0.contains("id=\"sealed-open\""));
        assert!(!refused.0.contains("AA"));

        match wait(nojs_sealed_response(
            locator,
            &event_id,
            Some(&key),
            &[],
            &config,
        )) {
            Err((status, rocket::Either::Left(page))) if status == Status::ServiceUnavailable => {
                assert_eq!(page.0, "Service unavailable");
                assert!(!page.0.contains(&key));
            }
            Ok(_) => panic!("a full gate fetched the note"),
            Err((status, _)) => panic!("a full gate returned {status}"),
        }

        held.pop();
        match wait(nojs_sealed_response(
            locator,
            &event_id,
            Some(&key),
            &[],
            &config,
        )) {
            Err((status, rocket::Either::Right(page))) if status == Status::NotFound => {
                assert_eq!(page.0, not_found_html());
                assert!(!page.0.contains(&key));
                assert!(!page.0.contains("does not decrypt"));
            }
            Ok(page) => panic!("a missing event was shown: {}", page.0),
            Err((status, _)) => panic!("a missing event returned {status}"),
        }
    }));
    drop(held);
    result.unwrap();
}

#[test]
fn nojs_open_fetched_opens_the_ciphertext_with_the_secret_and_d_tag() {
    let title = "unique <script>alert(1)</script> title";
    let sealed =
        crate::seal::seal_and_sign(title, "Ada", "**unique body text**", 128, 32, 256_000).unwrap();
    let event_id = sealed.note.id_hex();
    let event = sealed.note.event_json;
    let secret = nojs_query_secret(Some(&sealed.k)).unwrap();
    let note = nojs_open_fetched(&secret, &event, &sealed.locator, &event_id).unwrap();
    assert_eq!(note.title, title);
    assert_eq!(note.author, "Ada");
    assert_eq!(note.content, "**unique body text**");
    assert_eq!(note.locator, sealed.locator);

    let config = crate::config::Config::default();
    let page = nojs_opened_page(
        &sealed.locator,
        &event_id,
        &secret,
        &event,
        &sealed.k,
        &[],
        &config,
    );
    let created = serde_json::from_str::<serde_json::Value>(&event).unwrap()["created_at"]
        .as_i64()
        .unwrap();
    let date = chrono::DateTime::from_timestamp(created, 0)
        .unwrap()
        .format("%B %d, %Y")
        .to_string();
    assert!(page.contains("<h1>unique &lt;script&gt;alert(1)&lt;/script&gt; title</h1>"));
    assert!(page.contains(&format!("Ada · {date}")));
    assert!(page.contains("<strong>unique body text</strong>"));
    assert!(page.contains("article-content"));
    assert!(page.contains("post.css"));
    assert!(page.contains("post-noscript.css"));
    assert!(!page.contains("<script"));
    assert!(!page.contains("post.js"));
    assert!(page.contains(&format!(
        "href=\"/s/{}/{}#{}\" target=\"_blank\">link</a>",
        sealed.locator, event_id, sealed.k
    )));
    assert!(page.contains(&format!(
        "property=\"og:url\" content=\"/s/{}/{}\"",
        sealed.locator, event_id
    )));
    assert!(!page.contains(&format!("href=\"/{}\"", sealed.locator)));
    assert!(!page.contains("http-equiv"));
    assert!(!page.contains("location."));
    assert_eq!(page.matches(&sealed.k).count(), 1);
    assert!(!page.contains("does not decrypt"));
    let payload = serde_json::from_str::<serde_json::Value>(&event).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!page.contains(&payload));
    assert!(!page.contains(&event));

    let zeros = nojs_query_secret(Some(&"A".repeat(43))).unwrap();
    assert!(nojs_open_fetched(&zeros, &event, &sealed.locator, &event_id).is_none());
    let refused = nojs_opened_page(
        &sealed.locator,
        &event_id,
        &zeros,
        &event,
        &sealed.k,
        &[],
        &config,
    );
    assert!(refused.contains("This page does not decrypt it."));
    assert!(!refused.contains("unique body text"));
    assert!(!refused.contains("<strong>"));
    assert!(!refused.contains(&sealed.k));
    assert!(!refused.contains(&payload));
    assert!(!refused.contains("<script"));

    let mut moved: serde_json::Value = serde_json::from_str(&event).unwrap();
    let tags = moved.get_mut("tags").unwrap().as_array_mut().unwrap();
    let d = tags
        .iter_mut()
        .find(|tag| tag[0].as_str() == Some("d"))
        .unwrap();
    d[1] = serde_json::json!("00000000000000000000");
    assert!(nojs_open_fetched(&secret, &moved.to_string(), &sealed.locator, &event_id).is_none());
    moved.as_object_mut().unwrap().remove("content");
    assert!(nojs_open_fetched(&secret, &moved.to_string(), &sealed.locator, &event_id).is_none());
    moved.as_object_mut().unwrap().remove("tags");
    assert!(nojs_open_fetched(&secret, &moved.to_string(), &sealed.locator, &event_id).is_none());
    let mut other_id: serde_json::Value = serde_json::from_str(&event).unwrap();
    other_id["id"] = serde_json::json!("ab".repeat(32));
    assert!(
        nojs_open_fetched(&secret, &other_id.to_string(), &sealed.locator, &event_id).is_none()
    );
}

#[test]
fn sealed_query_reuses_relays_for_public_fetch() {
    let hints = vec![
        "wss://relay.damus.io".to_string(),
        "wss://127.0.0.1".to_string(),
    ];
    let configured = vec!["wss://relay.primal.net".to_string()];
    assert_eq!(
        relays_for_public_fetch(&hints, &configured),
        vec![
            "wss://relay.damus.io".to_string(),
            "wss://relay.primal.net".to_string(),
        ]
    );
    let started = std::time::Instant::now();
    let event_id = "ab".repeat(32);
    assert!(fetch_sealed_by_id(
        &["wss://127.0.0.1".to_string()],
        &event_id,
        "0123456789abcdef0123",
        &["ws://10.0.0.1".to_string()],
        128,
        32,
        4,
        std::time::Duration::from_millis(20),
    )
    .is_none());
    assert!(started.elapsed() < std::time::Duration::from_millis(200));
    assert!(fetch_sealed_by_id(
        &configured,
        &event_id,
        "0123456789ABCDEF0123",
        &[],
        128,
        32,
        4,
        std::time::Duration::from_millis(20),
    )
    .is_none());
}

#[test]
fn public_fetch_asks_hint_relays_then_the_instance() {
    let hints = vec![
        "wss://hint.example".to_string(),
        "wss://relay.damus.io".to_string(),
    ];
    let fallback = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ];
    assert_eq!(
        relays_for_public_fetch(&hints, &fallback),
        vec![
            "wss://hint.example".to_string(),
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
        ]
    );
    assert_eq!(relays_for_public_fetch(&[], &fallback), fallback);
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
    let fallback = vec!["wss://nos.lol".to_string()];
    let relays = relays_for_public_fetch(&hints, &fallback);
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
    let fallback = vec!["wss://127.0.0.1".to_string()];
    let relays = relays_for_public_fetch(&hints, &fallback);
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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
    };

    set_nojs_test_accepted(Some(vec![instance.clone()]));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let (locator, event_id, key, query) = seal_link_parts(&href);
    assert!(query.is_empty(), "{href}");
    assert!(crate::nostr::sealed_locator(&locator), "{href}");
    assert!(crate::nostr::sealed_event_id(&event_id), "{href}");
    assert!(nojs_query_secret(Some(&key)).is_some(), "{href}");
    assert!(href.contains(&locator) && href.contains(&event_id) && href.contains(&key));
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
    rejected.relay = "wss://relay.damus.io".to_string();
    rejected.relay_box = "wss://127.0.0.1".to_string();
    assert_nojs_error(&rejected, &config, "relay_invalid");
    rejected.relay_box = "   ".to_string();
    assert_eq!(nojs_chosen_relay(&rejected), "wss://relay.damus.io");
    rejected.relay_box.clear();
    rejected.relay = "wss://127.0.0.1".to_string();
    assert_eq!(nojs_chosen_relay(&rejected), "");

    rejected.relay = "wss://hint.example".to_string();
    set_nojs_test_accepted(Some(vec!["wss://hint.example".to_string()]));
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&rejected, &config, "");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let (locator, event_id, key, query) = seal_link_parts(&href);
    assert!(crate::nostr::sealed_locator(&locator), "{href}");
    assert!(crate::nostr::sealed_event_id(&event_id), "{href}");
    assert!(nojs_query_secret(Some(&key)).is_some(), "{href}");
    assert_eq!(query, "r=wss%3A%2F%2Fhint.example");
    assert!(!href.contains("nostr_publish_failed"), "{href}");
}

#[test]
#[serial(nojs_sealed_fetch)]
fn opening_the_nojs_redirect_renders_the_note_or_refuses() {
    struct ClearFetch;
    impl Drop for ClearFetch {
        fn drop(&mut self) {
            set_nojs_test_accepted(None);
            set_nojs_test_fetch(NojsTestFetch::Live);
        }
    }
    let _clear = ClearFetch;

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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
    };
    set_nojs_test_accepted(Some(vec![instance]));
    set_nojs_test_fetch(NojsTestFetch::Capture);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let (locator, event_id, key, query) = seal_link_parts(&href);
    assert!(query.is_empty(), "{href}");

    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::sealed_view, super::sealed_open]),
    )
    .unwrap();

    let path = format!("/s/{locator}/{event_id}");
    let shell = client.get(&path).dispatch();
    assert_eq!(shell.status(), Status::Ok);
    let shell_body = shell.into_string().unwrap();
    assert!(shell_body.contains("id=\"sealed-open\""));
    assert!(shell_body.contains("id=\"sealed-event\""));
    assert!(!shell_body.contains("Redirect title"));
    assert!(!shell_body.contains("opened note body"));

    let opened = post_open(&client, &path, &format!("k={key}"));
    assert_eq!(opened.0, Status::Ok);
    assert!(opened.1.contains("<h1>Redirect title</h1>"));
    assert!(opened.1.contains("opened note body"));
    assert!(!opened.1.contains("<script"));
    assert!(opened
        .1
        .contains(&format!("href=\"/s/{locator}/{event_id}#{key}\"")));
    assert!(!opened.1.contains("does not decrypt"));

    let mut bad_key = key.clone();
    bad_key.replace_range(0..1, if key.starts_with('A') { "B" } else { "A" });
    let refused = post_open(&client, &path, &format!("k={bad_key}"));
    assert_eq!(refused.0, Status::Ok);
    assert!(refused.1.contains("The key does not open this note."));
    assert!(refused.1.contains("id=\"sealed-open\""));
    assert!(!refused.1.contains("Redirect title"));
    assert!(!refused.1.contains("opened note body"));
    assert!(!refused.1.contains(&key));

    let missing_locator = post_open(
        &client,
        &format!("/s/not-a-locator/{event_id}"),
        &format!("k={key}"),
    );
    assert_eq!(missing_locator.0, Status::NotFound);
    assert_eq!(missing_locator.1, not_found_html());
    assert!(!missing_locator.1.contains("Redirect title"));
    assert!(!missing_locator.1.contains(&key));
    assert!(!missing_locator.1.contains("does not decrypt"));

    set_nojs_test_fetch(NojsTestFetch::Missing);
    let missing_event = post_open(&client, &path, &format!("k={key}"));
    assert_eq!(missing_event.0, Status::NotFound);
    assert_eq!(missing_event.1, not_found_html());
    assert!(!missing_event.1.contains("Redirect title"));
    assert!(!missing_event.1.contains("opened note body"));
    assert!(!missing_event.1.contains(&key));
    assert!(!missing_event.1.contains("does not decrypt"));
}

fn seal_link_parts(href: &str) -> (String, String, String, String) {
    let (before, key) = href.split_once('#').unwrap_or_else(|| panic!("{href}"));
    assert!(!before.contains("?k=") && !before.contains("&k="), "{href}");
    let (path, query) = before.split_once('?').unwrap_or((before, ""));
    let rest = path.strip_prefix("/s/").unwrap_or_else(|| panic!("{href}"));
    let (locator, event_id) = rest.split_once('/').unwrap_or_else(|| panic!("{href}"));
    (
        locator.to_string(),
        event_id.to_string(),
        key.to_string(),
        query.to_string(),
    )
}

fn post_open(client: &rocket::local::blocking::Client, path: &str, body: &str) -> (Status, String) {
    let response = client
        .post(path)
        .header(ContentType::Form)
        .body(body)
        .dispatch();
    let status = response.status();
    sealed_route_headers(&response);
    (status, response.into_string().unwrap())
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

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://instance.example".to_string()];
    let form = NewPost {
        title: "Added relay title".to_string(),
        content: "added relay body".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
        relay: "wss://hint.example".to_string(),
        relay_box: String::new(),
        relay_remove: false,
    };
    set_nojs_test_accepted(Some(vec!["wss://hint.example".to_string()]));
    set_nojs_test_fetch(NojsTestFetch::Capture);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    let (locator, event_id, key, query) = seal_link_parts(&href);
    assert_eq!(query, "r=wss%3A%2F%2Fhint.example");
    assert!(nojs_query_secret(Some(&key)).is_some(), "{href}");
    let js_href = format!("/s/{locator}/{event_id}?r=wss%3A%2F%2Fhint.example#{key}");

    NOJS_RECORD_HINTS.store(true, std::sync::atomic::Ordering::Relaxed);
    let client = rocket::local::blocking::Client::tracked(
        rocket::build()
            .manage(config)
            .mount("/", routes![super::sealed_view, super::sealed_open]),
    )
    .unwrap();

    let opened = post_open(
        &client,
        &format!("/s/{locator}/{event_id}"),
        &format!("k={key}&r=wss://hint.example"),
    );
    assert_eq!(opened.0, Status::Ok);
    assert!(opened.1.contains("<h1>Added relay title</h1>"));
    assert!(opened.1.contains("added relay body"));
    assert!(opened.1.contains(&js_href));
    assert!(!opened.1.contains("Publishing failed"));
    assert_eq!(
        nojs_fetch_hints_seen(),
        vec!["wss://hint.example".to_string()]
    );

    let bare = client.get("/s/not-a-locator").dispatch();
    assert_eq!(bare.status(), Status::NotFound);

    let js = client
        .get(format!(
            "/s/{locator}/{event_id}?r=wss%3A%2F%2Fhint.example"
        ))
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
        relay: "  wss://damus.io  ".to_string(),
        relay_box: String::new(),
        relay_remove: false,
    };

    let expired = nojs_publish_failure_page(&config, &form, "csrf_token_invalid", None).0;
    assert!(expired.contains("The publish form expired. Try again."));
    assert!(expired.contains("Hello &lt;World&gt;"));
    assert!(expired.contains("  # ici  "));
    assert!(expired.contains("value=\"Ada\""));
    assert!(expired.contains("<input type=\"hidden\" name=\"relay\" value=\"wss://damus.io\" />"));
    assert!(expired.contains("<li data-relay=\"wss://damus.io\">"));
    assert!(expired.contains("<span class=\"relay-name\">damus.io</span>"));
    assert!(!relay_url_input_after(&expired, "sidebar-relays").contains("value="));
    assert!(!expired.contains("<script"));
    assert!(!expired.contains("not-a-token"));

    let mut blank = form.clone();
    blank.relay.clear();
    blank.relay_box.clear();
    blank.content = " \n ".to_string();
    let empty = nojs_publish_failure_page(&config, &blank, "content_required", None).0;
    assert!(empty.contains("Write something before publishing."));
    assert!(empty.contains("Hello &lt;World&gt;"));
    assert!(empty.contains(" \n "));
    assert!(empty.contains("relay-list-user\" hidden"));

    let mut local = form.clone();
    local.relay.clear();
    local.relay_box = "wss://127.0.0.1".to_string();
    let rejected = nojs_publish_failure_page(&config, &local, "relay_invalid", None).0;
    assert_eq!(
        relay_field_error(&rejected),
        "Use a public wss:// relay URL."
    );
    assert_eq!(form_error_text(&rejected), "Use a public wss:// relay URL.");
    assert!(
        relay_url_input_after(&rejected, "sidebar-relays").contains("value=\"wss://127.0.0.1\"")
    );
    assert!(rejected.contains("<input type=\"hidden\" name=\"relay\" value=\"\" />"));
    assert!(rejected.contains("relay-list-user\" hidden"));
    assert!(!rejected.contains("<li data-relay=\"wss://127.0.0.1\">"));

    let mut configured = form.clone();
    configured.relay.clear();
    configured.relay_box = "wss://relay.primal.net".to_string();
    let known = nojs_publish_failure_page(&config, &configured, "nostr_publish_failed", None).0;
    assert!(known.contains("Publishing failed. Try again."));
    assert!(relay_url_input_after(&known, "sidebar-relays")
        .contains("value=\"wss://relay.primal.net\""));
    assert!(known.contains("relay-list-user\" hidden"));

    let mut both = form.clone();
    both.relay = "wss://relay.damus.io".to_string();
    both.relay_box = "wss://127.0.0.1".to_string();
    let kept = nojs_publish_failure_page(&config, &both, "relay_invalid", None).0;
    assert!(
        kept.contains("<input type=\"hidden\" name=\"relay\" value=\"wss://relay.damus.io\" />")
    );
    assert!(relay_url_input_after(&kept, "sidebar-relays").contains("value=\"wss://127.0.0.1\""));
    assert!(kept.contains("<li data-relay=\"wss://relay.damus.io\">"));
    assert!(!kept.contains("<li data-relay=\"wss://127.0.0.1\">"));
    assert_eq!(form_error_text(&kept), "Use a public wss:// relay URL.");

    let mut poisoned = form.clone();
    poisoned.relay = "wss://127.0.0.1".to_string();
    poisoned.relay_box = "   ".to_string();
    let dropped = nojs_publish_failure_page(&config, &poisoned, "title_required", None).0;
    assert!(dropped.contains("A title is required."));
    assert_eq!(relay_field_error(&dropped), "");
    assert!(dropped.contains("<input type=\"hidden\" name=\"relay\" value=\"\" />"));
    assert!(dropped.contains("relay-list-user\" hidden"));
    assert!(!relay_url_input_after(&dropped, "sidebar-relays").contains("value="));

    assert_eq!(
        nojs_error_code("/nojs?error=content_required"),
        Some("content_required")
    );
    assert!(nojs_error_code("/nojs/s/abc?k=secret").is_none());
    assert!(nojs_error_code("/nojs?error=").is_none());
    assert!(nojs_error_code("/?error=title_required").is_none());

    let too_long = nojs_publish_failure_page(&config, &form, "content_too_long", None).0;
    assert!(too_long.contains("The post is too long."));
    assert!(too_long.contains("Hello &lt;World&gt;"));
    assert!(too_long.contains("  # ici  "));
}

#[test]
#[serial(nojs_sealed_fetch)]
fn a_note_at_the_plaintext_cap_seals_and_an_over_cap_payload_keeps_the_error() {
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
        relay: "wss://nojs-seal.invalid".to_string(),
        relay_box: String::new(),
        relay_remove: false,
    };
    set_nojs_test_accepted(Some(vec!["wss://relay.primal.net".to_string()]));
    let published = publish_nojs_note(&form, &config).unwrap();
    let cap = crate::seal::sealed_payload_max(
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    );
    assert!(published.payload.len() > form.content.len());
    assert!(published.payload.len() <= cap);
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
    set_nojs_test_accepted(None);
    assert!(href.starts_with("/s/"), "{href}");
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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
    };
    match publish_nojs_note(&over, &config) {
        Err("content_too_long") => {}
        Err(error) => panic!("{error}"),
        Ok(_) => panic!("expected content_too_long"),
    }
    let page = nojs_publish_failure_page(&config, &over, "content_too_long", None).0;
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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
    };
    assert!(controls.content.len() <= wide.limits.content_max_length);
    assert_nojs_error(&controls, &wide, "content_too_long");
    let kept = nojs_publish_failure_page(&wide, &controls, "content_too_long", None).0;
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
    let note = crate::nostr::sign_sealed_note("ciphertext", "0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.damus.io".to_string()];
    let body = NostrPublishBody {
        event,
        relays: vec!["wss://127.0.0.1".to_string()],
        csrf_token: String::new(),
    };
    let (parsed, relays, locator) = note_and_relays_for_publish(&body, &config, "").unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(relays, vec!["wss://relay.damus.io".to_string()]);
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
    assert_eq!(relays, vec!["wss://relay.damus.io".to_string()]);
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
        csrf_token: String::new(),
    };
    assert!(note_and_relays_for_publish(&body, &config, "").is_none());
}

#[test]
fn nostr_publish_accepts_a_sealed_note_that_passes_the_stamp() {
    let note = crate::nostr::sign_sealed_note("ciphertext", "0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://relay.damus.io".to_string()];
    let body = NostrPublishBody {
        event,
        relays: vec!["wss://127.0.0.1".to_string()],
        csrf_token: String::new(),
    };
    let (parsed, relays, locator) = note_and_relays_for_publish(&body, &config, "").unwrap();
    assert_eq!(parsed.id, note.id);
    assert_eq!(relays, vec!["wss://relay.damus.io".to_string()]);
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
fn nostr_publish_sends_site_relays_and_the_added_relays_that_fit() {
    let note = crate::nostr::sign_sealed_note("ciphertext", "0123456789abcdef0123", 1_700_000_000);
    let event: serde_json::Value = serde_json::from_str(&note.event_json).unwrap();
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec![
        "wss://relay.primal.net".to_string(),
        "wss://relay.snort.social".to_string(),
        "wss://offchain.pub".to_string(),
    ];
    let mut relays = config.nostr.relays.clone();
    for n in 1..=4 {
        relays.push(format!("wss://extra{n}.example"));
    }
    let body = NostrPublishBody {
        event,
        relays,
        csrf_token: String::new(),
    };
    let (_, chosen, _) = note_and_relays_for_publish(&body, &config, "").unwrap();
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
    let add = home.find("async function addRelayFromBlock").unwrap();
    let add_listener = home
        .find("document.querySelectorAll(\"[data-relay-add]\")")
        .unwrap();
    let remove = home.find("closest(\".relay-remove\")").unwrap();
    let listed = home
        .find("renderExtraRelayLists();\n            let relayListSending")
        .unwrap();
    let send = home.find("async function sendRelayList()").unwrap();
    let refresh = home.find("async function refreshRelayCircuits()").unwrap();
    let get = home
        .find("fetch(\"/tor-circuits\", { cache: \"no-store\" })")
        .unwrap();
    let paint = home[get..].find("paintRelayCircuits()").unwrap();
    let publish = home.find("async function publishOnNostr()").unwrap();
    assert!(home[add..add_listener].contains("void sendRelayList()"));
    assert!(home[remove..listed].contains("void sendRelayList()"));
    assert!(home[send..refresh].contains("method: \"POST\""));
    assert!(home[send..refresh].contains("relays: mergePublishRelays()"));
    assert!(home[send..refresh].contains("csrf_token:"));
    assert!(refresh < get);
    assert!(home[refresh..get].contains("await sendRelayList()"));
    assert!(!home[get..get + paint].contains("method: \"POST\""));
    assert!(home.contains("relay-spinner"));
    assert!(home.contains("everyRelayHasPath()"));
    let waiting = home.find("function everyRelayHasPath()").unwrap();
    let sync = home.find("function syncNostrButtons()").unwrap();
    assert!(home[waiting..sync].contains("if (!circuitsTracked)"));
    assert!(home[waiting..sync].contains("relaysForPublicPublish(mergePublishRelays())"));
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
    assert!(nostr.contains(&format!("SECRET_LEN = {}", nonograph_nip44::SECRET_LEN)));
    assert!(nostr.contains(&format!("LOCATOR_LEN = {}", nonograph_nip44::LOCATOR_LEN)));
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
    let secret = [4u8; nonograph_nip44::SECRET_LEN];
    let nonce = [5u8; 32];
    let payload = nonograph_nip44::seal("Title", "Ada", content, locator, &secret, &nonce).unwrap();
    let opened = nonograph_nip44::open(&payload, &secret, locator).unwrap();
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
    assert!(loader.contains("./page/nonograph_page.js"));
    assert!(loader.contains("./page/nonograph_page_bg.wasm"));
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
    assert!(nostr.contains(&format!("POW_BITS = {}", nonograph_nip44::POW_BITS)));
    assert!(nostr.contains(&format!("KIND_SEAL = {}", nonograph_nip44::KIND)));
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
            nonograph_nip44::POW_BITS.to_string(),
        ],
    ];
    let mut rebuilt = String::from("[0,");
    nonograph_nip44::push_json_string(&mut rebuilt, PUBKEY);
    rebuilt.push(',');
    rebuilt.push_str(&CREATED_AT.to_string());
    rebuilt.push(',');
    rebuilt.push_str(&nonograph_nip44::KIND.to_string());
    rebuilt.push(',');
    nonograph_nip44::push_tags(&mut rebuilt, &tags);
    rebuilt.push(',');
    nonograph_nip44::push_json_string(&mut rebuilt, CONTENT);
    rebuilt.push(']');
    assert_eq!(rebuilt, CANONICAL);
    let id = nonograph_nip44::event_id(PUBKEY, CREATED_AT, nonograph_nip44::KIND, &tags, CONTENT);
    let hex: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, ID);
    assert!(nonograph_nip44::check_stamp(
        &id,
        PUBKEY,
        CREATED_AT,
        nonograph_nip44::KIND,
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
    let payload_at = body.find("JSON.stringify({").unwrap();
    let payload_end = body[payload_at..]
        .find("})")
        .map(|offset| payload_at + offset)
        .unwrap();
    let payload = &body[payload_at..payload_end];
    assert!(payload.contains("event,"));
    assert!(payload.contains("relays: relaysForPublicPublish(relays)"));
    assert!(payload.contains("csrf_token: csrfToken || \"\""));
    assert!(!payload.contains("secret"));
    assert!(!payload.contains("key"));
    assert!(!payload.contains("locator"));
    assert!(!payload.contains("sealed"));

    let home = include_str!("../templates/home.js");
    assert!(home.contains("contentMax: Number(form.dataset.sealMax)"));
    assert!(home.contains("The post is too long."));
    assert!(!home.contains("contentMax: contentLimit"));
    assert!(home.contains("location.assign(openedSealHref(result))"));
    let href = home.find("function openedSealHref(").unwrap();
    let href_end = home[href..]
        .find("\n            async function publishOnNostr")
        .map(|offset| href + offset)
        .unwrap();
    let href_body = &home[href..href_end];
    assert!(href_body.contains("\"/s/\""));
    assert!(href_body.contains("result.locator"));
    assert!(href_body.contains("result.event.id"));
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
    assert!(nostr.contains(&format!("KIND_SEAL = {}", nonograph_nip44::KIND)));
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
    let helper = home.find("nonographPublishPublicNote").unwrap();
    let imported = home.find("await import").unwrap();
    assert!(helper < imported);
    assert!(home.contains("./nostr.js"));
    assert!(home.contains("publishPublicNote"));
    assert!(home.contains("csrfToken"));
    assert!(home.contains("location.assign(openedSealHref(result))"));
    let href = home.find("function openedSealHref(").unwrap();
    let href_end = home[href..]
        .find("\n            async function publishOnNostr")
        .map(|offset| href + offset)
        .unwrap();
    let href_body = &home[href..href_end];
    assert!(href_body.contains("\"/s/\""));
    assert!(href_body.contains("result.locator"));
    assert!(href_body.contains("result.event.id"));
    assert!(href_body.contains("\"#\""));
    assert!(href_body.contains("result.key"));
    assert!(href_body.contains("encodeURIComponent(relay)"));
    assert!(home.contains("button[type=\"submit\"][form=\"publishForm\"]"));
    assert!(home.contains("e.preventDefault()"));
    assert!(home.contains("publishOnNostr()"));
    assert!(home.contains("Publishing failed. Try again."));
    assert!(home.contains("A title is required."));
    assert!(home.contains("if (publishing || !everyRelayHasPath())"));
    assert!(home.contains("leaveBusy = true"));
    assert!(home.contains("if (!leaveBusy)"));
    assert!(home.contains("syncNostrButtons"));
    assert!(home.contains("nonograph_extra_relays"));
    assert!(home.contains("nostr.MAX_FETCH_RELAYS"));
    assert!(home.contains("function extraRelayLimit()"));
    assert!(home.contains("stored.slice(0, extraRelayLimit())"));
    assert!(!home.contains("MAX_EXTRA_RELAYS"));
    assert!(home.contains("instance.has(key)"));
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
    assert_eq!(html.matches(">Publish</button>").count(), 2);
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
fn homepage_js_gets_the_instance_public_relays() {
    let config = crate::config::Config::default();
    let js = home_context(&config, false, None, None);
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
        &nonograph_nip44::MAX_PAYLOAD_CHARS.to_string()
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
    assert!(nojs.get("nostr_relays").is_some());
}

#[test]
fn the_homepage_and_sealed_fetch_stay_on_the_config_relays() {
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
        assert_eq!(relays, config.nostr.relays);
        let sidebar = page.get("relays_sidebar").unwrap();
        assert!(sidebar.contains("wss://config.example"));
        assert!(!sidebar.contains("wss://pool.example"));
    }
    assert_eq!(
        relays_for_public_fetch(&[], &config.nostr.relays),
        config.nostr.relays
    );

    let home = include_str!("../src/pages/home.rs");
    let sealed = include_str!("../src/pages/sealed.rs");
    assert!(!home.contains("usable_rows"));
    assert!(!sealed.contains("usable_rows"));
    assert!(home.contains("config.nostr.relays"));
    assert!(sealed.contains("config.nostr.relays"));
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
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
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
fn nojs_create_rejects_a_relay_public_relay_url_rejects() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let mut form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
        relay: "wss://127.0.0.1".to_string(),
        relay_box: String::new(),
        relay_remove: false,
    };
    assert_eq!(nojs_chosen_relay(&form), "");

    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "   ".to_string();
    assert_eq!(nojs_chosen_relay(&form), "");
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "wss://relay.damus.io".to_string();
    form.relay_box = "   ".to_string();
    assert_eq!(nojs_chosen_relay(&form), "wss://relay.damus.io");
    form.relay_box = "wss://127.0.0.1".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=relay_invalid");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = " \n ".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    config.security.csrf_protection_enabled = true;
    form.content = "hi".to_string();
    form.csrf_token = "not-a-token".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=csrf_token_invalid");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://nojs-seal.invalid".to_string()];
    form.csrf_token.clear();
    form.relay.clear();
    form.relay_box.clear();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "wss://nojs-seal.invalid".to_string();
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "wss://relay.damus.io".to_string();
    form.relay_box = "wss://127.0.0.1".to_string();
    assert_eq!(nojs_chosen_relay(&form), "wss://127.0.0.1");
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=relay_invalid");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "wss://127.0.0.1".to_string();
    form.relay_box = "  wss://relay.damus.io  ".to_string();
    assert_eq!(nojs_chosen_relay(&form), "wss://relay.damus.io");
    form.relay = "wss://relay.damus.io".to_string();
    form.relay_box = "   ".to_string();
    assert_eq!(nojs_chosen_relay(&form), "wss://relay.damus.io");
    form.relay = "wss://127.0.0.1".to_string();
    assert_eq!(nojs_chosen_relay(&form), "");
    form.relay_box.clear();
    form.relay = "   ".to_string();
    assert_eq!(nojs_chosen_relay(&form), "");

    form.relay = "wss://127.0.0.1".to_string();
    form.relay_box.clear();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_builds_the_public_relay_list_and_fails_when_it_is_empty() {
    let configured = vec![
        "wss://relay.primal.net".to_string(),
        "wss://127.0.0.1".to_string(),
    ];
    assert_eq!(
        nojs_publish_relays("", &configured).as_deref(),
        Some(relays_for_public_fetch(&[], &configured).as_slice())
    );
    let base = relays_for_public_fetch(&[], &configured);
    let hinted = nojs_publish_relays("wss://relay.damus.io", &configured).unwrap();
    assert_eq!(hinted[0], "wss://relay.damus.io");
    assert_eq!(hinted.len(), base.len() + 1);
    for relay in &base {
        assert!(hinted.contains(relay), "{relay}");
    }
    assert!(!hinted.iter().any(|relay| relay.contains("127.0.0.1")));
    let same = nojs_publish_relays("wss://relay.primal.net", &configured).unwrap();
    assert_eq!(same, base);
    assert_eq!(
        nojs_publish_relays("wss://Relay.Primal.net/", &configured).unwrap(),
        base
    );
    let cased = nojs_publish_relays("wss://Relay.Damus.io:443/", &configured).unwrap();
    assert_eq!(cased[0], "wss://relay.damus.io");
    assert_eq!(cased.len(), base.len() + 1);

    let mut many = Vec::new();
    for index in 0..crate::nostr::MAX_FETCH_RELAYS {
        many.push(format!("wss://configured{index}.example"));
    }
    let extra = "wss://extra.example";
    let published = nojs_publish_relays(extra, &many).unwrap();
    let kept = relays_for_public_fetch(&[], &many);
    assert_eq!(published[0], extra);
    assert_eq!(published.len(), kept.len() + 1);
    for relay in &kept {
        assert!(published.contains(relay), "{relay}");
    }
    assert_eq!(
        relays_for_public_fetch(&[extra.to_string()], &many).len(),
        crate::nostr::MAX_FETCH_RELAYS
    );
    assert!(nojs_publish_relays("", &["wss://127.0.0.1".to_string()]).is_none());
    assert_eq!(
        nojs_publish_relays("wss://relay.damus.io", &["wss://127.0.0.1".to_string()]).unwrap(),
        vec!["wss://relay.damus.io".to_string()]
    );

    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let mut form = NewPost {
        title: "Hello World".to_string(),
        content: "hi".to_string(),
        alias: "Ada".to_string(),
        csrf_token: String::new(),
        relay: String::new(),
        relay_box: String::new(),
        relay_remove: false,
    };
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = " \n ".to_string();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=content_required");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.content = "hi".to_string();
    form.relay = "wss://10.0.0.1".to_string();
    form.relay_box.clear();
    assert_eq!(nojs_chosen_relay(&form), "");
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay = "wss://nojs-seal.invalid".to_string();
    config.nostr.timeout_secs = 1;
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());

    form.relay.clear();
    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
    assert_eq!(href, "/nojs?error=nostr_publish_failed");
    assert!(std::fs::read_dir(temp.path().join("content")).is_err());
}

#[test]
#[serial(nojs_sealed_fetch)]
fn nojs_create_seals_and_publishes_inside_the_blocking_task() {
    let mut config = crate::config::Config::default();
    config.security.csrf_protection_enabled = false;
    config.nostr.timeout_secs = 1;
    config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
    let form = NewPost {
        title: "  Hello World  ".to_string(),
        content: "  hi  ".to_string(),
        alias: "  Ada  ".to_string(),
        csrf_token: String::new(),
        relay: "wss://nojs-seal.invalid".to_string(),
        relay_box: String::new(),
        relay_remove: false,
    };
    let published = match publish_nojs_note(&form, &config) {
        Ok(published) => published,
        Err(error) => panic!("seal and publish did not run: {error}"),
    };
    assert!(crate::nostr::sealed_locator(&published.locator));
    assert_ne!(published.payload, form.content);
    assert!(published.payload.len() > form.content.len());
    assert!(published.accepted.is_empty());
    assert!(published.k.len() > 1);
    assert!(!published.k.contains('='));
    let configured = vec!["wss://relay.primal.net".to_string()];
    let mut accepted = published;
    let key = accepted.k.clone();
    accepted.accepted = vec!["wss://nojs-seal.invalid".to_string()];
    assert_eq!(
        nojs_sealed_location(&accepted, &configured).as_deref(),
        Some(
            format!(
                "/s/{}/{}?r=wss%3A%2F%2Fnojs-seal.invalid#{}",
                accepted.locator, accepted.event_id, key
            )
            .as_str()
        )
    );
    accepted.accepted = vec!["wss://127.0.0.1".to_string()];
    assert!(nojs_sealed_location(&accepted, &configured).is_none());
    accepted.accepted = vec!["wss://nojs-seal.invalid".to_string()];
    accepted.k.clear();
    assert!(nojs_sealed_location(&accepted, &configured).is_none());
    accepted.k = key;
    accepted.accepted = vec!["wss://Relay.Primal.net/".to_string()];
    assert_eq!(
        nojs_sealed_location(&accepted, &configured).as_deref(),
        Some(
            format!(
                "/s/{}/{}#{}",
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
        nojs_sealed_location(&accepted, &configured).as_deref(),
        Some(
            format!(
                "/s/{}/{}?r=wss%3A%2F%2Fnojs-seal.invalid#{}",
                accepted.locator, accepted.event_id, accepted.k
            )
            .as_str()
        )
    );

    let temp = tempfile::tempdir().unwrap();
    let href = create_location(&form, &config, "");
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
fn nojs_create_takes_a_public_fetch_slot_until_it_finishes() {
    let mut held = Vec::new();
    for _ in 0..MAX_PUBLIC_FETCHES {
        held.push(PublicFetchSlot::acquire().expect("public fetch slot"));
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
            relay: String::new(),
            relay_box: String::new(),
            relay_remove: false,
        };
        config.nostr.relays = vec!["wss://127.0.0.1".to_string()];
        let blocked = tempfile::tempdir().unwrap();
        let href = create_location(&form, &config, "");
        assert_eq!(href, "/nojs?error=nostr_publish_failed");
        assert!(std::fs::read_dir(blocked.path().join("content")).is_err());
        assert!(nojs_publish_slot().is_err());

        config.nostr.relays = vec!["wss://relay.primal.net".to_string()];
        struct ResetAccepted;
        impl Drop for ResetAccepted {
            fn drop(&mut self) {
                set_nojs_test_accepted(None);
            }
        }
        let _reset_accepted = ResetAccepted;
        set_nojs_test_accepted(Some(vec!["wss://relay.primal.net".to_string()]));
        let href = create_location(&form, &config, "");
        assert_eq!(href, "/nojs?error=nostr_publish_failed");
        assert!(nojs_publish_slot().is_err(), "sealing took a reader slot");

        held.pop();
        let href = create_location(&form, &config, "");
        assert!(href.starts_with("/s/"), "{href}");
        let released = nojs_publish_slot().expect("publish released the reader slot");
        drop(released);
    }));
    drop(held);
    busy.unwrap();
}
