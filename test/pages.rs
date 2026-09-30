use super::*;
use std::collections::HashMap;

#[test]
fn static_assets_are_cacheable_and_html_is_not() {
    for path in [
        HOME_CSS_PATH,
        HOME_JS_PATH,
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
    assert!(js.contains("<p class=\"form-error\"></p>"));
    assert!(js.contains("0 / 128,000"));
    assert!(js.contains("class=\"\""));
    assert!(!js.contains("class=\"nojs\""));
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
    let BuiltPage::Ready(nojs) = build_static_page("about", &config, true) else {
        panic!("about page did not render");
    };
    assert!(!nojs.contains("<script"));
    assert!(nojs.contains("href=\"/about\""));
    assert!(nojs.contains(">js<"));

    let BuiltPage::Ready(js) = build_static_page("about", &config, false) else {
        panic!("about page did not render");
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
