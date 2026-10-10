use std::collections::HashMap;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rocket::http::{ContentType, Header, Status};
use rocket::response::content;
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::csrf;
use crate::publish;
use crate::template;
use crate::{
    is_nostr_identifier, is_valid_post_id, parse_frontmatter, render_options,
    yaml_frontmatter_field, Post, PostStorage,
};

pub const PAGE_JS_PATH: &str = "/page/nonograph_page.js";
pub const PAGE_WASM_PATH: &str = "/page/nonograph_page_bg.wasm";
pub const HOME_CSS_PATH: &str = "/home.css";
pub const HOME_JS_PATH: &str = "/home.js";
pub const NOSTR_JS_PATH: &str = "/nostr.js";
pub const SECP256K1_JS_PATH: &str = "/secp256k1.js";
pub const POST_CSS_PATH: &str = "/post.css";
pub const POST_JS_PATH: &str = "/post.js";
pub const POST_NOSCRIPT_CSS_PATH: &str = "/post-noscript.css";
pub const WRITEMARK_JS_PATH: &str = "/writemark.js";

const PAGE_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/nonograph_page.js"));
const PAGE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nonograph_page_bg.wasm"));

const LONG_CACHE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "public, no-cache";

const STATIC_PAGES: &[&str] = &["about", "legal", "markup", "api"];

pub fn cache_control_for_path(path: &str) -> &'static str {
    if path.starts_with("/nojs/s/") {
        return "no-store";
    }
    match path {
        PAGE_JS_PATH | PAGE_WASM_PATH => REVALIDATE,
        HOME_CSS_PATH
        | HOME_JS_PATH
        | NOSTR_JS_PATH
        | SECP256K1_JS_PATH
        | POST_CSS_PATH
        | POST_JS_PATH
        | POST_NOSCRIPT_CSS_PATH
        | WRITEMARK_JS_PATH => LONG_CACHE,
        _ => "no-store, max-age=0",
    }
}

struct PageAssetTags {
    js: String,
    wasm: String,
}

fn page_asset_tags() -> &'static PageAssetTags {
    static TAGS: OnceLock<PageAssetTags> = OnceLock::new();
    TAGS.get_or_init(|| PageAssetTags {
        js: entity_tag(PAGE_JS.as_bytes()),
        wasm: entity_tag(PAGE_WASM),
    })
}

fn short_hex(digest: &[u8]) -> String {
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn entity_tag(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("\"{}\"", short_hex(&hasher.finalize()))
}

pub fn entity_tag_for_path(path: &str) -> Option<&'static str> {
    let tags = page_asset_tags();
    match path {
        PAGE_JS_PATH => Some(tags.js.as_str()),
        PAGE_WASM_PATH => Some(tags.wasm.as_str()),
        _ => None,
    }
}

pub fn if_none_match_matches(header: &str, etag: &str) -> bool {
    header.split(',').any(|part| {
        let part = part.trim();
        if part == "*" {
            return true;
        }
        let part = part.strip_prefix("W/").unwrap_or(part).trim();
        part == etag
    })
}

struct Assets {
    version: String,
    home_css: String,
    home_js: String,
    nostr_js: String,
    secp256k1_js: String,
    post_css: String,
    post_js: String,
    post_noscript_css: String,
    writemark_js: String,
}

pub fn warm() {
    nonograph_parser::warm();
    let _ = assets();
    let _ = page_asset_tags();
    if let Err(error) = template::shared().preload(&["home", "post"]) {
        eprintln!("Nonograph: {error}");
    }
}

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let read = |name: &str| -> String {
            std::fs::read_to_string(format!("templates/{name}")).unwrap_or_else(|error| {
                panic!("Nonograph: failed to read templates/{name}: {error}")
            })
        };
        let home_css = read("home.css");
        let home_js = read("home.js");
        let nostr_js = read("nostr.js");
        let secp256k1_js = read("secp256k1.js");
        let post_css = read("post.css");
        let post_js = read("post.js");
        let post_noscript_css = read("post-noscript.css");
        let writemark_js = read("writemark.js");
        let version = asset_version(&[
            &home_css,
            &home_js,
            &nostr_js,
            &secp256k1_js,
            &post_css,
            &post_js,
            &post_noscript_css,
            &writemark_js,
        ]);
        Assets {
            version,
            home_css,
            home_js,
            nostr_js,
            secp256k1_js,
            post_css,
            post_js,
            post_noscript_css,
            writemark_js,
        }
    })
}

fn asset_version(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    short_hex(&hasher.finalize())
}

fn home_scripts(version: &str) -> String {
    format!(
        "<script type=\"module\" src=\"/writemark.js?v={version}\"></script>\n<script type=\"module\" src=\"/home.js?v={version}\"></script>"
    )
}

fn home_content_field(nojs: bool, max_length: &str, content: &str) -> String {
    if nojs {
        format!(
            "<textarea name=\"content\" placeholder=\"Write something with markdown...\" maxlength=\"{max_length}\" title=\"Mind your opsec, be careful what you share.\" required>{}</textarea>",
            nonograph_parser::html_escape(content)
        )
    } else {
        format!(
            "<writemark-editor name=\"content\" mode=\"live\" placeholder=\"Write something with markdown or type / for formatting options...\" maxlength=\"{max_length}\" title=\"Mind your opsec, be careful what you share.\" required></writemark-editor>"
        )
    }
}

fn split_home_error(error: Option<&str>) -> (Option<&str>, &str) {
    match error {
        Some(code @ ("relay_required" | "relay_invalid" | "relay_configured")) => (None, code),
        other => (other, ""),
    }
}

fn home_error_message(error: Option<&str>) -> String {
    match error.unwrap_or("") {
        "" => String::new(),
        "csrf_token_invalid" => "The publish form expired. Try again.".to_string(),
        "title_required" => "A title is required.".to_string(),
        "content_required" => "Write something before publishing.".to_string(),
        "title_too_long" => "The title is too long.".to_string(),
        "content_too_long" => "The post is too long.".to_string(),
        "alias_too_long" => "The alias is too long.".to_string(),
        "relay_required" => "Enter a relay URL.".to_string(),
        "relay_invalid" => "Use a public wss:// relay URL.".to_string(),
        "relay_configured" => "That relay is already configured on this site.".to_string(),
        "nostr_publish_failed" => "Publishing failed. Try again.".to_string(),
        "save_failed" => "Saving the post failed. Try again.".to_string(),
        "no_available_slots" => "Could not pick an address for this post. Try again.".to_string(),
        "javascript_required" => "This page needs JavaScript. Use the no-JS page.".to_string(),
        _ => "Something went wrong. Try again.".to_string(),
    }
}

fn post_scripts(version: &str) -> String {
    format!("<script src=\"/post.js?v={version}\"></script>")
}

fn post_fallback_css(nojs: bool, version: &str) -> String {
    if nojs {
        format!("<link rel=\"stylesheet\" href=\"/post-noscript.css?v={version}\" />")
    } else {
        String::new()
    }
}

fn mode_href(nojs: bool, public_id: &str) -> String {
    if nojs {
        format!("/{public_id}")
    } else {
        format!("/nojs/{public_id}")
    }
}

fn mode_label(nojs: bool) -> &'static str {
    if nojs {
        "js"
    } else {
        "nojs"
    }
}

pub(crate) const MIN_TAB_RELAY_TIMEOUT_MS: u64 = 10_000;

pub(crate) fn tab_relay_timeout_ms(timeout_secs: u64) -> u64 {
    timeout_secs
        .max(1)
        .saturating_mul(1000)
        .max(MIN_TAB_RELAY_TIMEOUT_MS)
}

fn relay_timeout(config: &Config) -> Duration {
    Duration::from_secs(config.nostr.timeout_secs.max(1))
}

fn format_count(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped.chars().rev().collect()
}

fn home_context(config: &Config, nojs: bool, error: Option<&str>) -> HashMap<String, String> {
    home_form_context(config, nojs, error, "", "", "", "", "", false, false)
}

fn home_form_context(
    config: &Config,
    nojs: bool,
    error: Option<&str>,
    title: &str,
    alias: &str,
    content: &str,
    saved: &str,
    typed: &str,
    list_relay: bool,
    surface_relay_error: bool,
) -> HashMap<String, String> {
    let mut context = HashMap::new();
    let max_length = config.limits.content_max_length.to_string();
    context.insert(
        "content_limit".to_string(),
        format_count(config.limits.content_max_length),
    );
    context.insert(
        "body_class".to_string(),
        if nojs { "nojs" } else { "" }.to_string(),
    );
    context.insert(
        "about_durability".to_string(),
        about_durability(nojs).to_string(),
    );
    context.insert("title_value".to_string(), title.to_string());
    context.insert("alias_value".to_string(), alias.to_string());
    context.insert(
        "content_field".to_string(),
        home_content_field(nojs, &max_length, content),
    );
    context.insert(
        "nostr_relays".to_string(),
        serde_json::to_string(&relays_for_public_fetch(&[], &config.nostr.relays))
            .unwrap_or_else(|_| "[]".to_string()),
    );
    context.insert(
        "nostr_timeout_ms".to_string(),
        tab_relay_timeout_ms(config.nostr.timeout_secs).to_string(),
    );
    context.insert(
        "seal_max".to_string(),
        crate::seal::sealed_payload_max(
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
        )
        .to_string(),
    );
    let (mut form_error, relay_error) = split_home_error(error);
    if surface_relay_error && !relay_error.is_empty() {
        form_error = Some(relay_error);
    }
    context.insert("error".to_string(), home_error_message(form_error));
    let relay_message = if relay_error.is_empty() {
        String::new()
    } else {
        home_error_message(Some(relay_error))
    };
    let csrf_token = if config.security.csrf_protection_enabled {
        csrf::generate_csrf_token_with_timestamp()
    } else {
        String::new()
    };
    context.insert("csrf_token".to_string(), csrf_token);
    // Update form action to point to /nojs/create
    context.insert(
        "form_action".to_string(),
        if nojs { "/nojs/create" } else { "/create" }.to_string(),
    );
    let version = assets().version.clone();
    context.insert("asset_version".to_string(), version.clone());
    context.insert(
        "scripts".to_string(),
        if nojs {
            String::new()
        } else {
            home_scripts(&version)
        },
    );
    let relays = relays_for_public_fetch(&[], &config.nostr.relays);
    context.insert(
        "relays_sidebar".to_string(),
        crate::relays::render_sidebar_field(
            &relays,
            nojs,
            saved,
            typed,
            list_relay,
            &relay_message,
        ),
    );
    context.insert(
        "relays_section".to_string(),
        crate::relays::render_about_section(&relays, nojs),
    );
    context.insert(
        "relay_field".to_string(),
        if nojs {
            crate::relays::render_saved_relay_input(saved)
        } else {
            String::new()
        },
    );
    context
}

fn about_durability(nojs: bool) -> &'static str {
    if nojs {
        "The host seals the note and does not keep a copy. The writing is ordinary markup, not a proprietary format."
    } else {
        "Your pages are stored as plaintext markup, no proprietary formats."
    }
}

fn render_home(config: &Config, nojs: bool, error: Option<&str>) -> content::RawHtml<String> {
    let context = home_context(config, nojs, error);
    match template::shared().render("home", &context) {
        Ok(html) => content::RawHtml(html),
        Err(error) => content::RawHtml(format!("Template error: {error}")),
    }
}

#[get("/?<error>")]
pub fn index(error: Option<&str>, config: &State<Config>) -> content::RawHtml<String> {
    render_home(config, false, error)
}

#[get("/nojs?<error>")]
pub fn nojs_index(error: Option<&str>, config: &State<Config>) -> content::RawHtml<String> {
    render_home(config, true, error)
}

#[derive(Clone, FromForm)]
pub(crate) struct NewPost {
    title: String,
    content: String,
    alias: String,
    csrf_token: String,
    #[field(default = String::new())]
    relay: String,
    #[field(default = String::new())]
    relay_box: String,
    #[field(default = false)]
    relay_remove: bool,
    #[field(default = false)]
    javascript_off: bool,
}

fn csrf_failure(config: &Config, token: &str) -> Option<&'static str> {
    if config.security.csrf_protection_enabled && !csrf::is_valid_csrf_token(token) {
        Some("csrf_token_invalid")
    } else {
        None
    }
}

pub(crate) fn create_location_in_dir(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> String {
    let home = if nojs { "/nojs" } else { "/" };
    if !nojs && form.javascript_off {
        return format!("{home}?error=javascript_required");
    }
    if let Some(error) = csrf_failure(config, &form.csrf_token) {
        return format!("{home}?error={error}");
    }

    let title = if nojs {
        form.title.trim()
    } else {
        form.title.as_str()
    };
    let alias = if nojs {
        form.alias.trim()
    } else {
        form.alias.as_str()
    };
    let alias_check = if alias.trim().is_empty() {
        None
    } else {
        Some(alias)
    };
    if let Err(error) = config.validate_post(title, &form.content, alias_check) {
        return format!("{home}?error={error}");
    }
    if nojs {
        let relay = nojs_chosen_relay(form);
        if !relay.is_empty() && !crate::nostr::public_relay_url(relay) {
            return format!("{home}?error=relay_invalid");
        }
        if nojs_publish_relays(relay, &config.nostr.relays).is_none() {
            return format!("{home}?error=nostr_publish_failed");
        }
        return match publish_nojs_note(form, config) {
            Ok(published) => nojs_sealed_location(&published, &config.nostr.relays)
                .unwrap_or_else(|| format!("{home}?error=nostr_publish_failed")),
            Err(error) => format!("{home}?error={error}"),
        };
    }

    let rendered_content =
        nonograph_parser::render_markdown_with_config(&form.content, &render_options(config));
    match publish::publish_note_in_dir(
        storage,
        title,
        alias,
        &rendered_content,
        &form.content,
        base_dir,
    ) {
        Ok(post_id) => publish::published_href(nojs, &post_id),
        Err(failure) => publish::publish_failure_href(nojs, failure),
    }
}

struct NojsPublished {
    accepted: Vec<String>,
    locator: String,
    event_id: String,
    payload: String,
    k: String,
}

fn publish_nojs_note(form: &NewPost, config: &Config) -> Result<NojsPublished, &'static str> {
    let relays = nojs_publish_relays(nojs_chosen_relay(form), &config.nostr.relays)
        .ok_or("nostr_publish_failed")?;
    let sealed = match crate::seal::seal_and_sign(
        form.title.trim(),
        form.alias.trim(),
        &form.content,
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    ) {
        Ok(sealed) => sealed,
        Err(crate::seal::SealPlaintextError::TooLong) => return Err("content_too_long"),
        Err(_) => return Err("nostr_publish_failed"),
    };
    #[cfg(test)]
    remember_nojs_test_event(&sealed.note.event_json);
    let timeout = relay_timeout(config);
    let accepted = {
        let _slot = nojs_publish_slot().map_err(|_| "nostr_publish_failed")?;
        #[cfg(test)]
        {
            if let Some(accepted) = nojs_test_accepted() {
                accepted
            } else {
                crate::nostr::publish_to_relays(&relays, &sealed.note, timeout)
            }
        }
        #[cfg(not(test))]
        {
            crate::nostr::publish_to_relays(&relays, &sealed.note, timeout)
        }
    };
    let event: serde_json::Value =
        serde_json::from_str(&sealed.note.event_json).map_err(|_| "nostr_publish_failed")?;
    let payload = event
        .get("content")
        .and_then(|content| content.as_str())
        .ok_or("nostr_publish_failed")?
        .to_string();
    Ok(NojsPublished {
        accepted,
        locator: sealed.locator,
        event_id: sealed.note.id_hex(),
        k: sealed.k,
        payload,
    })
}

#[cfg(test)]
thread_local! {
    static NOJS_TEST_ACCEPTED: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn nojs_test_accepted() -> Option<Vec<String>> {
    NOJS_TEST_ACCEPTED.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
fn set_nojs_test_accepted(accepted: Option<Vec<String>>) {
    NOJS_TEST_ACCEPTED.with(|slot| *slot.borrow_mut() = accepted);
}

#[cfg(test)]
enum NojsTestFetch {
    Live,
    Missing,
    Capture,
    Event(String),
}

#[cfg(test)]
static NOJS_TEST_FETCH: Mutex<NojsTestFetch> = Mutex::new(NojsTestFetch::Live);

#[cfg(test)]
thread_local! {
    static NOJS_TEST_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
static NOJS_FETCH_HINTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(test)]
static NOJS_RECORD_HINTS: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
fn note_nojs_fetch_hints(hints: &[String]) {
    if NOJS_RECORD_HINTS.load(Ordering::Relaxed) {
        *NOJS_FETCH_HINTS.lock().expect("nojs fetch hints") = hints.to_vec();
    }
}

#[cfg(test)]
fn nojs_fetch_hints_seen() -> Vec<String> {
    NOJS_FETCH_HINTS.lock().expect("nojs fetch hints").clone()
}

#[cfg(test)]
fn set_nojs_test_fetch(next: NojsTestFetch) {
    let capture = matches!(next, NojsTestFetch::Capture);
    NOJS_TEST_CAPTURE.with(|flag| flag.set(capture));
    *NOJS_TEST_FETCH.lock().expect("nojs test fetch") = next;
}

#[cfg(test)]
fn remember_nojs_test_event(event_json: &str) {
    if !NOJS_TEST_CAPTURE.with(|flag| flag.get()) {
        return;
    }
    let mut slot = NOJS_TEST_FETCH.lock().expect("nojs test fetch");
    if matches!(*slot, NojsTestFetch::Capture) {
        *slot = NojsTestFetch::Event(event_json.to_string());
    }
}

#[cfg(test)]
fn canned_nojs_event() -> Option<Option<String>> {
    match &*NOJS_TEST_FETCH.lock().expect("nojs test fetch") {
        NojsTestFetch::Live | NojsTestFetch::Capture => None,
        NojsTestFetch::Missing => Some(None),
        NojsTestFetch::Event(event) => Some(Some(event.clone())),
    }
}

fn query_escape(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0xf) as usize] as char);
            }
        }
    }
    out
}

fn nojs_sealed_location(published: &NojsPublished, configured: &[String]) -> Option<String> {
    if published.k.is_empty()
        || published.locator.is_empty()
        || !crate::nostr::sealed_event_id(&published.event_id)
    {
        return None;
    }
    let mut kept = Vec::new();
    for relay in &published.accepted {
        push_public_relay(&mut kept, relay);
    }
    if kept.is_empty() {
        return None;
    }
    let instance = relays_for_public_fetch(&[], configured);
    let hints = kept
        .into_iter()
        .filter(|relay| !instance.iter().any(|have| have == relay))
        .collect::<Vec<_>>();
    Some(js_seal_href(
        &published.locator,
        &published.event_id,
        &published.k,
        &hints,
    ))
}

async fn handle_create(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
) -> rocket::response::Redirect {
    let storage = storage.clone();
    let config = config.clone();
    let form = form.clone();
    match rocket::tokio::task::spawn_blocking(move || {
        create_location_in_dir(nojs, &form, &storage, &config, ".")
    })
    .await
    {
        Ok(location) => rocket::response::Redirect::to(location),
        Err(_) if nojs => rocket::response::Redirect::to("/nojs?error=nostr_publish_failed"),
        Err(_) => publish::publish_failure_redirect(
            nojs,
            publish::PublishFailure::Save("publishing was interrupted".to_string()),
        ),
    }
}

#[post("/create", data = "<form>")]
pub async fn create_post(
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<rocket::response::Redirect, content::RawHtml<String>> {
    Ok(handle_create(false, &form, storage, config).await)
}

fn nojs_chosen_relay(form: &NewPost) -> &str {
    let typed = form.relay_box.trim();
    if !typed.is_empty() {
        return typed;
    }
    let saved = form.relay.trim();
    if crate::nostr::public_relay_url(saved) {
        return saved;
    }
    ""
}

fn nojs_publish_relays(relay: &str, configured: &[String]) -> Option<Vec<String>> {
    let mut relays = relays_for_public_fetch(&[], configured);
    if let Some(relay) = crate::nostr::canonical_public_relay(relay) {
        if !relays.iter().any(|have| have == &relay) {
            relays.insert(0, relay);
        }
    }
    if relays.is_empty() {
        None
    } else {
        Some(relays)
    }
}

fn nojs_publish_slot() -> Result<PublicFetchSlot, &'static str> {
    PublicFetchSlot::acquire().ok_or("/nojs?error=nostr_publish_failed")
}

fn nojs_relay_add_status(
    relay: &str,
    csrf_token: &str,
    config: &Config,
) -> (Option<&'static str>, bool) {
    if let Some(error) = csrf_failure(config, csrf_token) {
        return (Some(error), false);
    }
    if relay.is_empty() {
        return (Some("relay_required"), false);
    }
    if crate::nostr::canonical_public_relay(relay).is_none() {
        return (Some("relay_invalid"), false);
    }
    if relays_for_public_fetch(&[], &config.nostr.relays)
        .iter()
        .any(|have| crate::nostr::same_public_relay(have, relay))
    {
        return (Some("relay_configured"), false);
    }
    (None, true)
}

fn render_nojs_form(
    config: &Config,
    form: &NewPost,
    error: Option<&str>,
    list_relay: bool,
    surface_relay_error: bool,
) -> content::RawHtml<String> {
    let saved = crate::nostr::canonical_public_relay(form.relay.trim()).unwrap_or_default();
    let context = home_form_context(
        config,
        true,
        error,
        &form.title,
        &form.alias,
        &form.content,
        &saved,
        form.relay_box.trim(),
        list_relay,
        surface_relay_error,
    );
    match template::shared().render("home", &context) {
        Ok(html) => content::RawHtml(html),
        Err(error) => content::RawHtml(format!("Template error: {error}")),
    }
}

fn render_nojs_draft(config: &Config, form: &NewPost) -> content::RawHtml<String> {
    let mut form = form.clone();
    if let Some(error) = csrf_failure(config, &form.csrf_token) {
        return render_nojs_form(
            config,
            &form,
            Some(error),
            nojs_lists_extra_relay(form.relay.trim(), config),
            false,
        );
    }
    if form.relay_remove {
        form.relay.clear();
        return render_nojs_form(config, &form, None, false, false);
    }
    form.relay_box = form.relay_box.trim().to_string();
    let (error, accept) = nojs_relay_add_status(&form.relay_box, &form.csrf_token, config);
    if accept {
        form.relay = crate::nostr::canonical_public_relay(&form.relay_box)
            .unwrap_or_else(|| form.relay_box.clone());
        form.relay_box.clear();
        return render_nojs_form(config, &form, None, true, false);
    }
    render_nojs_form(
        config,
        &form,
        error,
        nojs_lists_extra_relay(form.relay.trim(), config),
        false,
    )
}

fn nojs_error_code(location: &str) -> Option<&str> {
    let error = location.strip_prefix("/nojs?error=")?;
    if error.is_empty() || error.contains(['&', '#', '?']) {
        None
    } else {
        Some(error)
    }
}

fn nojs_lists_extra_relay(relay: &str, config: &Config) -> bool {
    crate::nostr::canonical_public_relay(relay).is_some_and(|relay| {
        !relays_for_public_fetch(&[], &config.nostr.relays)
            .iter()
            .any(|have| have == &relay)
    })
}

fn nojs_publish_failure_page(
    config: &Config,
    form: &NewPost,
    error: &str,
) -> content::RawHtml<String> {
    render_nojs_form(
        config,
        form,
        Some(error),
        nojs_lists_extra_relay(form.relay.trim(), config),
        true,
    )
}

#[post("/nojs/relay", data = "<form>")]
pub fn nojs_add_relay(
    form: rocket::form::Form<NewPost>,
    config: &State<Config>,
) -> content::RawHtml<String> {
    render_nojs_draft(config, &form)
}

#[post("/nojs/create", data = "<form>")]
pub async fn nojs_create_post(
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> rocket::Either<rocket::response::Redirect, content::RawHtml<String>> {
    let form = form.into_inner();
    let storage = storage.inner().clone();
    let config_owned = config.inner().clone();
    let moved = form.clone();
    let location = match rocket::tokio::task::spawn_blocking(move || {
        create_location_in_dir(true, &moved, &storage, &config_owned, ".")
    })
    .await
    {
        Ok(location) => location,
        Err(_) => {
            return rocket::Either::Right(nojs_publish_failure_page(
                config,
                &form,
                "nostr_publish_failed",
            ));
        }
    };
    if let Some(error) = nojs_error_code(&location) {
        rocket::Either::Right(nojs_publish_failure_page(config, &form, error))
    } else {
        rocket::Either::Left(rocket::response::Redirect::to(location))
    }
}

#[derive(Deserialize)]
pub struct NostrPublishBody {
    event: serde_json::Value,
    #[serde(default)]
    relays: Vec<String>,
    #[serde(default)]
    csrf_token: String,
}

#[derive(Serialize)]
pub struct NostrPublishResult {
    locator: String,
    accepted: Vec<String>,
}

pub(crate) fn note_and_relays_for_publish(
    body: &NostrPublishBody,
    config: &Config,
) -> Option<(crate::nostr::SignedNote, Vec<String>, String)> {
    if csrf_failure(config, &body.csrf_token).is_some() {
        return None;
    }
    let (note, locator) = crate::nostr::sealed_note_from_json(
        &body.event,
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
    )?;
    let relays = relays_for_public_publish(&body.relays, &config.nostr.relays);
    if relays.is_empty() {
        return None;
    }
    Some((note, relays, locator))
}

fn publish_result(locator: String, accepted: Vec<String>) -> NostrPublishResult {
    let locator = if accepted
        .iter()
        .any(|relay| crate::nostr::canonical_public_relay(relay).is_some())
    {
        locator
    } else {
        String::new()
    };
    NostrPublishResult { locator, accepted }
}

#[get("/tor-circuits")]
pub fn tor_circuits() -> Json<serde_json::Value> {
    let (tracking, relays) = crate::tor_circuits::view(crate::nostr::relays_use_proxy());
    Json(serde_json::json!({
        "tracking": tracking,
        "relays": relays,
    }))
}

#[derive(Deserialize)]
pub(crate) struct TorCircuitsBody {
    #[serde(default)]
    relays: Vec<String>,
    #[serde(default)]
    csrf_token: String,
}

pub(crate) fn relays_for_circuit_pass(
    body: &TorCircuitsBody,
    config: &Config,
) -> Option<Vec<String>> {
    if csrf_failure(config, &body.csrf_token).is_some() {
        return None;
    }
    Some(body.relays.clone())
}

#[post("/tor-circuits", data = "<body>")]
pub fn assign_tor_circuits(body: Json<TorCircuitsBody>, config: &State<Config>) -> Status {
    let config = config.inner();
    let Some(relays) = relays_for_circuit_pass(&body, config) else {
        return Status::BadRequest;
    };
    let timeout = relay_timeout(config);
    crate::nostr::schedule_relay_assignments(relays, config.nostr.relays.clone(), timeout);
    Status::NoContent
}

#[post("/nostr/publish", data = "<body>")]
pub async fn nostr_publish(
    body: Json<NostrPublishBody>,
    config: &State<Config>,
) -> Result<Json<NostrPublishResult>, Status> {
    let config = config.inner().clone();
    let body = body.into_inner();
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err(Status::ServiceUnavailable);
    };
    match rocket::tokio::task::spawn_blocking(move || {
        let (note, relays, locator) = note_and_relays_for_publish(&body, &config)?;
        let timeout = relay_timeout(&config);
        let accepted = crate::nostr::publish_to_relays(&relays, &note, timeout);
        Some(publish_result(locator, accepted))
    })
    .await
    {
        Ok(Some(result)) => Ok(Json(result)),
        Ok(None) => Err(Status::BadRequest),
        Err(_) => Err(Status::ServiceUnavailable),
    }
}

fn static_page_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_static_page(name: &str) -> bool {
    STATIC_PAGES.contains(&name)
}

#[get("/markup")]
pub fn markup_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("markup", config, false)
}

#[get("/legal")]
pub fn legal_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("legal", config, false)
}

#[get("/about")]
pub fn about_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("about", config, false)
}

#[get("/api")]
pub fn api_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("api", config, false)
}

#[derive(Debug)]
enum BuiltPage {
    Ready(String),
    Invalid(String),
    Missing,
}

fn read_static_page_markdown(page_name: &str) -> Option<String> {
    for dir in ["content", "pages"] {
        if let Ok(file_content) = std::fs::read_to_string(format!("{dir}/{page_name}.md")) {
            return Some(file_content);
        }
    }
    None
}

fn serve_static_page(
    page_name: &str,
    config: &Config,
    nojs: bool,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    let key = if nojs {
        format!("nojs:{page_name}")
    } else {
        page_name.to_string()
    };
    if let Some(html) = static_page_cache().lock().unwrap().get(&key).cloned() {
        return Ok(content::RawHtml(html));
    }
    match build_static_page(page_name, config, nojs) {
        BuiltPage::Ready(html) => {
            static_page_cache()
                .lock()
                .unwrap()
                .insert(key, html.clone());
            Ok(content::RawHtml(html))
        }
        BuiltPage::Invalid(html) => Ok(content::RawHtml(html)),
        BuiltPage::Missing => Err((Status::NotFound, content::RawHtml(not_found_html()))),
    }
}

fn build_static_page(page_name: &str, config: &Config, nojs: bool) -> BuiltPage {
    let Some(file_content) = read_static_page_markdown(page_name) else {
        return BuiltPage::Missing;
    };
    let parsed = parse_frontmatter(&file_content);
    let Some((title, author, created_at, raw_content)) = parsed else {
        return BuiltPage::Invalid(format!(
            "<h1>Error</h1><p>Invalid file format for {page_name}</p>"
        ));
    };
    let rendered_content =
        nonograph_parser::render_markdown_with_config(&raw_content, &render_options(config));
    let mut context = article_context(
        &title,
        &author,
        Some(created_at),
        &format!("/{page_name}"),
        "",
        nojs,
        page_name,
    );
    context.insert("content".to_string(), rendered_content);
    match template::shared().render("post", &context) {
        Ok(html) => BuiltPage::Ready(html),
        Err(error) => BuiltPage::Invalid(format!("Template error: {error}")),
    }
}

pub(crate) fn post_description(raw: &str) -> String {
    let Some((end, _)) = raw.char_indices().nth(160) else {
        return raw.to_string();
    };
    let mut description = raw[..end].to_string();
    description.push_str("...");
    description
}

fn article_context(
    title: &str,
    author: &str,
    created_at: Option<DateTime<Utc>>,
    url: &str,
    description: &str,
    nojs: bool,
    public_id: &str,
) -> HashMap<String, String> {
    let author = if author.is_empty() {
        "Anonymous".to_string()
    } else {
        author.to_string()
    };
    let mut context = HashMap::new();
    context.insert("title".to_string(), title.to_string());
    context.insert("author".to_string(), author.clone());
    context.insert("author_display".to_string(), format!("{author} · "));
    context.insert(
        "created_at".to_string(),
        created_at
            .map(|time| time.format("%B %d, %Y").to_string())
            .unwrap_or_default(),
    );
    context.insert(
        "created_at_iso".to_string(),
        created_at
            .map(|time| time.format("%Y-%m-%dT00:00:00+00:00").to_string())
            .unwrap_or_default(),
    );
    context.insert("url".to_string(), url.to_string());
    context.insert("description".to_string(), description.to_string());
    fill_page_chrome(&mut context, nojs, public_id);
    context
}

fn fill_page_chrome(context: &mut HashMap<String, String>, nojs: bool, public_id: &str) {
    let version = assets().version.clone();
    context.insert("asset_version".to_string(), version.clone());
    context.insert(
        "scripts".to_string(),
        if nojs {
            String::new()
        } else {
            post_scripts(&version)
        },
    );
    context.insert("mode_href".to_string(), mode_href(nojs, public_id));
    context.insert("mode_label".to_string(), mode_label(nojs).to_string());
    context.insert(
        "fallback_css".to_string(),
        post_fallback_css(nojs, &version),
    );
    context.insert("nostr_link".to_string(), String::new());
}

#[derive(Responder)]
pub enum ViewPage {
    Page(rocket::Either<content::RawHtml<String>, content::RawText<String>>),
    Redirect(rocket::response::Redirect),
}

type ViewErr = (
    Status,
    rocket::Either<content::RawText<String>, content::RawHtml<String>>,
);

const SEALED_TITLE: &str = "Sealed note";
const SEALED_PREVIEW: &str = "A note sealed in the browser.";

fn sealed_hint_inputs(hints: &[String]) -> String {
    let mut html = String::new();
    for hint in hints {
        let Some(relay) = crate::nostr::canonical_public_relay(hint) else {
            continue;
        };
        html.push_str("<input type=\"hidden\" name=\"r\" value=\"");
        html.push_str(&nonograph_parser::html_attr_escape(&relay));
        html.push_str("\" />");
    }
    html
}

fn sealed_open_form(locator: &str, event_id: &str, hints: &[String], key_error: &str) -> String {
    format!(
        "<form id=\"sealed-open\" method=\"post\" action=\"/s/{locator}/{event_id}\">\
<p id=\"sealed-key-error\">{}</p>\
<input id=\"sealed-key\" name=\"k\" type=\"text\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"The key is the part of the address after #.\" required />\
<button type=\"submit\">Open</button>\
{}</form>",
        nonograph_parser::html_escape(key_error),
        sealed_hint_inputs(hints),
        locator = nonograph_parser::html_attr_escape(locator),
        event_id = nonograph_parser::html_attr_escape(event_id),
    )
}

pub(crate) fn sealed_shell(
    event_json: &str,
    locator: &str,
    event_id: &str,
    hints: &[String],
    key_error: &str,
    config: &Config,
) -> String {
    let version = assets().version.clone();
    let options = render_options(config);
    let theme = nonograph_parser::html_attr_escape(&options.syntax_theme);
    let external = if options.external_link_security {
        "true"
    } else {
        "false"
    };
    let event = if event_json.is_empty() {
        String::new()
    } else {
        format!(
            "<script type=\"application/json\" id=\"sealed-event\">{}</script>\n    <script type=\"module\" src=\"/nostr.js?v={version}\"></script>",
            event_json.replace('<', "\\u003c"),
        )
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <link rel="icon" href="data:," />
    <meta property="og:title" content="{title}" />
    <meta property="og:type" content="website" />
    <meta property="og:description" content="{preview}" />
    <meta name="description" content="{preview}" />
    <link rel="stylesheet" href="/post.css?v={version}" />
</head>
<body>
    <div class="container">
        <div class="content">
            <h1 id="sealed-title">{title}</h1>
            <svg class="wave" viewBox="0 0 640 16" preserveAspectRatio="none" aria-hidden="true"><path d="M0 8 L16 3 L32 13 L48 3 L64 13 L80 3 L96 13 L112 3 L128 13 L144 3 L160 13 L176 3 L192 13 L208 3 L224 13 L240 3 L256 13 L272 3 L288 13 L304 3 L320 13 L336 3 L352 13 L368 3 L384 13 L400 3 L416 13 L432 3 L448 13 L464 3 L480 13 L496 3 L512 13 L528 3 L544 13 L560 3 L576 13 L592 3 L608 13 L624 3 L640 8"></path></svg>
            <div class="article-meta" id="sealed-author" hidden></div>
            {form}
            <div
                class="article-content"
                id="CONTENT"
                data-syntax-theme="{theme}"
                data-max-url-length="{max_url}"
                data-external-links="{external}"
            ></div>
        </div>
    </div>
    <div class="footer">
        <button type="button" id="sealed-copy">Copy link</button>
        <a href="/">write your own</a>
        <a href="/legal">legal</a>
        <a href="/api">api</a>
        <a href="https://github.com/wuddleko/nonograph">source code</a>
    </div>
    {event}
</body>
</html>"#,
        title = SEALED_TITLE,
        preview = SEALED_PREVIEW,
        form = sealed_open_form(locator, event_id, hints, key_error),
        event = event,
        version = version,
        theme = theme,
        max_url = options.max_url_length,
        external = external,
    )
}

#[derive(FromForm)]
pub(crate) struct SealHints {
    #[field(name = "r", default = Vec::<String>::new())]
    r: Vec<String>,
}

#[get("/s/<locator>/<event_id>?<hints..>")]
pub async fn sealed_view(
    locator: &str,
    event_id: &str,
    hints: SealHints,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(not_found_html())),
        ));
    }
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err((
            Status::ServiceUnavailable,
            rocket::Either::Left(content::RawText("Service unavailable".to_string())),
        ));
    };
    let config = config.inner().clone();
    let shell = config.clone();
    let locator = locator.to_string();
    let event_id = event_id.to_string();
    let query = locator.clone();
    let query_id = event_id.clone();
    let hints = hints.r;
    let event = match rocket::tokio::task::spawn_blocking({
        let hints = hints.clone();
        move || {
            #[cfg(test)]
            note_nojs_fetch_hints(&hints);
            #[cfg(test)]
            if let Some(canned) = canned_nojs_event() {
                return canned;
            }
            let timeout = relay_timeout(&config);
            fetch_sealed_by_id(
                &hints,
                &query_id,
                &query,
                &config.nostr.relays,
                config.limits.title_max_length,
                config.limits.alias_max_length,
                config.limits.content_max_length,
                timeout,
            )
        }
    })
    .await
    {
        Ok(event) => event,
        Err(_) => None,
    };
    let Some(event) = event else {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(not_found_html())),
        ));
    };
    Ok(content::RawHtml(sealed_shell(
        &event, &locator, &event_id, &hints, "", &shell,
    )))
}

pub(crate) fn nojs_query_secret(k: Option<&str>) -> Option<[u8; nonograph_nip44::SECRET_LEN]> {
    let text = k?;
    if text.is_empty()
        || !text
            .bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
    {
        return None;
    }
    let bytes = decode_base64url(text)?;
    if bytes.len() != nonograph_nip44::SECRET_LEN {
        return None;
    }
    let mut secret = [0u8; nonograph_nip44::SECRET_LEN];
    secret.copy_from_slice(&bytes);
    Some(secret)
}

fn decode_base64url(text: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        buf = (buf << 6) | u32::from(value(byte)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn nojs_sealed_refusal(locator: &str, event_id: &str) -> String {
    centered_notice(
        "The note opens in the browser",
        "The note opens in the browser",
        &format!(
            "<p>This page does not decrypt it.</p>\n    <p><a href=\"/s/{locator}/{event_id}\">js</a></p>\n    <p><a href=\"/\">Write Your Own</a></p>"
        ),
    )
}

fn nojs_not_found() -> ViewErr {
    (
        Status::NotFound,
        rocket::Either::Right(content::RawHtml(not_found_html())),
    )
}

fn event_d_tag(value: &serde_json::Value) -> Option<String> {
    value.get("tags")?.as_array()?.iter().find_map(|tag| {
        let tag = tag.as_array()?;
        if tag.first()?.as_str()? != "d" {
            return None;
        }
        tag.get(1)?.as_str().map(str::to_string)
    })
}

pub(crate) fn nojs_open_fetched(
    secret: &[u8; nonograph_nip44::SECRET_LEN],
    event_json: &str,
    locator: &str,
    event_id: &str,
) -> Option<nonograph_nip44::Note> {
    let value: serde_json::Value = serde_json::from_str(event_json).ok()?;
    if value.get("id")?.as_str()? != event_id {
        return None;
    }
    let payload = value.get("content")?.as_str()?;
    let d_tag = event_d_tag(&value)?;
    if d_tag != locator {
        return None;
    }
    let note = nonograph_nip44::open(payload, secret, &d_tag).ok()?;
    (note.locator == locator).then_some(note)
}

pub(crate) fn nojs_opened_page(
    locator: &str,
    event_id: &str,
    secret: &[u8; nonograph_nip44::SECRET_LEN],
    event_json: &str,
    k: &str,
    hints: &[String],
    config: &Config,
) -> String {
    let Some(note) = nojs_open_fetched(secret, event_json, locator, event_id) else {
        return nojs_sealed_refusal(locator, event_id);
    };
    nojs_note_html(locator, event_id, &note, event_json, k, hints, config)
        .unwrap_or_else(|error| format!("Template error: {error}"))
}

fn event_created_at(event_json: &str) -> Option<DateTime<Utc>> {
    let value: serde_json::Value = serde_json::from_str(event_json).ok()?;
    DateTime::from_timestamp(value.get("created_at")?.as_i64()?, 0)
}

fn nojs_note_html(
    locator: &str,
    event_id: &str,
    note: &nonograph_nip44::Note,
    event_json: &str,
    k: &str,
    hints: &[String],
    config: &Config,
) -> Result<String, String> {
    let content =
        nonograph_parser::render_markdown_with_config(&note.content, &render_options(config));
    let share = js_seal_href(locator, event_id, k, hints);
    let page = share.split_once('#').map(|(page, _)| page).unwrap_or(&share);
    let mut context = article_context(
        &note.title,
        &note.author,
        event_created_at(event_json),
        page,
        &post_description(&note.content),
        true,
        locator,
    );
    context.insert("content".to_string(), content);
    context.insert("mode_href".to_string(), share);
    context.insert("mode_label".to_string(), "link".to_string());
    template::shared().render("post", &context)
}

fn js_seal_href(locator: &str, event_id: &str, k: &str, hints: &[String]) -> String {
    let mut href = format!("/s/{locator}/{event_id}");
    let mut first = true;
    for relay in hints {
        let Some(relay) = crate::nostr::canonical_public_relay(relay) else {
            continue;
        };
        href.push(if first { '?' } else { '&' });
        first = false;
        href.push_str("r=");
        href.push_str(&query_escape(&relay));
    }
    href.push('#');
    href.push_str(k);
    href
}

pub(crate) async fn nojs_sealed_response(
    locator: &str,
    event_id: &str,
    k: Option<&str>,
    hints: &[String],
    config: &Config,
) -> Result<content::RawHtml<String>, ViewErr> {
    if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
        return Err(nojs_not_found());
    }
    let Some(k) = k else {
        return Ok(content::RawHtml(sealed_shell(
            "",
            locator,
            event_id,
            hints,
            "The key does not open this note.",
            config,
        )));
    };
    let Some(secret) = nojs_query_secret(Some(k)) else {
        return Ok(content::RawHtml(sealed_shell(
            "",
            locator,
            event_id,
            hints,
            "The key does not open this note.",
            config,
        )));
    };
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err((
            Status::ServiceUnavailable,
            rocket::Either::Left(content::RawText("Service unavailable".to_string())),
        ));
    };
    let config = config.clone();
    let query = locator.to_string();
    let query_id = event_id.to_string();
    let locator = locator.to_string();
    let event_id = event_id.to_string();
    let hints = hints.to_vec();
    let event = match rocket::tokio::task::spawn_blocking({
        let config = config.clone();
        let hints = hints.clone();
        move || {
            #[cfg(test)]
            note_nojs_fetch_hints(&hints);
            #[cfg(test)]
            if let Some(canned) = canned_nojs_event() {
                return canned;
            }
            let timeout = relay_timeout(&config);
            fetch_sealed_by_id(
                &hints,
                &query_id,
                &query,
                &config.nostr.relays,
                config.limits.title_max_length,
                config.limits.alias_max_length,
                config.limits.content_max_length,
                timeout,
            )
        }
    })
    .await
    {
        Ok(event) => event,
        Err(_) => None,
    };
    let Some(event) = event else {
        return Err(nojs_not_found());
    };
    let Some(note) = nojs_open_fetched(&secret, &event, &locator, &event_id) else {
        return Ok(content::RawHtml(sealed_shell(
            &event,
            &locator,
            &event_id,
            &hints,
            "The key does not open this note.",
            &config,
        )));
    };
    Ok(content::RawHtml(
        nojs_note_html(&locator, &event_id, &note, &event, k, &hints, &config)
            .unwrap_or_else(|error| format!("Template error: {error}")),
    ))
}

pub struct NojsSealedHeaders<R>(R);

impl<'r, 'o: 'r, R: Responder<'r, 'o>> Responder<'r, 'o> for NojsSealedHeaders<R> {
    fn respond_to(self, request: &'r rocket::Request<'_>) -> rocket::response::Result<'o> {
        let mut response = self.0.respond_to(request)?;
        response.set_header(Header::new("Referrer-Policy", "no-referrer"));
        response.set_header(Header::new("Cache-Control", "no-store"));
        Ok(response)
    }
}

#[derive(FromForm)]
pub(crate) struct NojsSealQuery {
    k: Option<String>,
    #[field(name = "r", default = Vec::<String>::new())]
    r: Vec<String>,
}

pub(crate) struct FragmentRedirect(String);

impl<'r> Responder<'r, 'static> for FragmentRedirect {
    fn respond_to(self, _: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        rocket::Response::build()
            .status(Status::SeeOther)
            .header(Header::new("Location", self.0))
            .ok()
    }
}

fn sealed_reader_href(locator: &str, event_id: &str, k: Option<&str>, hints: &[String]) -> String {
    match k.filter(|value| nojs_query_secret(Some(value)).is_some()) {
        Some(k) => js_seal_href(locator, event_id, k, hints),
        None => {
            let href = js_seal_href(locator, event_id, "", hints);
            href.strip_suffix('#').unwrap_or(&href).to_string()
        }
    }
}

#[get("/nojs/s/<locator>/<event_id>?<seal..>")]
pub fn nojs_sealed_view(
    locator: &str,
    event_id: &str,
    seal: NojsSealQuery,
) -> NojsSealedHeaders<Result<FragmentRedirect, ViewErr>> {
    if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
        return NojsSealedHeaders(Err(nojs_not_found()));
    }
    NojsSealedHeaders(Ok(FragmentRedirect(sealed_reader_href(
        locator,
        event_id,
        seal.k.as_deref(),
        &seal.r,
    ))))
}

#[derive(FromForm)]
pub(crate) struct OpenSealForm {
    #[field(default = String::new())]
    k: String,
    #[field(name = "r", default = Vec::<String>::new())]
    r: Vec<String>,
}

#[post("/s/<locator>/<event_id>", data = "<form>")]
pub async fn sealed_open(
    locator: &str,
    event_id: &str,
    form: rocket::form::Form<OpenSealForm>,
    config: &State<Config>,
) -> NojsSealedHeaders<Result<content::RawHtml<String>, ViewErr>> {
    NojsSealedHeaders(
        nojs_sealed_response(
            locator,
            event_id,
            Some(form.k.as_str()),
            &form.r,
            config.inner(),
        )
        .await,
    )
}

#[get("/<post_id>")]
pub async fn view_post(
    post_id: &str,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<ViewPage, ViewErr> {
    render_post(post_id, storage, config, false).await
}

#[get("/nojs/<post_id>")]
pub async fn nojs_view_post(
    post_id: &str,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<ViewPage, ViewErr> {
    render_post(post_id, storage, config, true).await
}

pub(crate) fn leftover_wrap_file_id(post_id: &str) -> Option<String> {
    crate::nostr::decode_nevent(post_id).map(|nevent| nevent.event_id_hex)
}

pub(crate) fn short_view_href(nojs: bool, post_id: &str, requested: &str) -> Option<String> {
    if post_id == requested || !is_valid_post_id(post_id) {
        None
    } else {
        Some(publish::published_href(nojs, post_id))
    }
}

pub(crate) fn leftover_wrap_post_in_dir(
    post_id: &str,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> Option<Arc<Post>> {
    let file_id = leftover_wrap_file_id(post_id)?;
    if !is_valid_post_id(&file_id) {
        return None;
    }
    load_post_from_disk(&file_id, storage, config, base_dir)
}

fn push_public_relay(relays: &mut Vec<String>, relay: &str) {
    if relays.len() == crate::nostr::MAX_FETCH_RELAYS {
        return;
    }
    let Some(relay) = crate::nostr::canonical_public_relay(relay) else {
        return;
    };
    if relays.iter().any(|have| have == &relay) {
        return;
    }
    relays.push(relay);
}

pub(crate) fn relays_for_public_publish(requested: &[String], configured: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for relay in configured {
        push_public_relay(&mut relays, relay);
    }
    for relay in requested {
        push_public_relay(&mut relays, relay);
    }
    relays
}

pub(crate) fn fetch_sealed_by_id(
    hints: &[String],
    event_id: &str,
    locator: &str,
    configured: &[String],
    title_max: usize,
    alias_max: usize,
    content_max: usize,
    timeout: Duration,
) -> Option<String> {
    let relays = relays_for_public_fetch(hints, configured);
    crate::nostr::fetch_sealed(
        &relays,
        event_id,
        locator,
        title_max,
        alias_max,
        content_max,
        timeout,
    )
}

pub(crate) fn relays_for_public_fetch(hints: &[String], fallback: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    let keep_one_for_instance = fallback
        .iter()
        .any(|relay| crate::nostr::public_relay_url(relay));
    let hint_limit = if keep_one_for_instance {
        crate::nostr::MAX_FETCH_RELAYS.saturating_sub(1)
    } else {
        crate::nostr::MAX_FETCH_RELAYS
    };
    for relay in hints {
        if relays.len() == hint_limit {
            break;
        }
        push_public_relay(&mut relays, relay);
    }
    for relay in fallback {
        push_public_relay(&mut relays, relay);
    }
    relays
}

pub(crate) fn relays_to_assign(page_relays: &[String], configured: &[String]) -> Vec<String> {
    let mut relays = Vec::new();
    for relay in page_relays {
        push_public_relay(&mut relays, relay);
    }
    for relay in configured {
        push_public_relay(&mut relays, relay);
    }
    relays
}

pub(crate) fn may_fetch_public_kind(kind: Option<u32>) -> bool {
    matches!(kind, None | Some(crate::nostr::KIND_LONG_FORM))
}

pub(crate) fn is_gift_wrap(kind: Option<u32>) -> bool {
    kind == Some(crate::nostr::KIND_GIFT_WRAP)
}

pub(crate) fn refuse_unopened_wrap(kind: Option<u32>, has_local_file: bool) -> Option<String> {
    if has_local_file || !is_gift_wrap(kind) {
        None
    } else {
        Some(wrap_refused_html())
    }
}

pub(crate) fn post_from_public_note(
    fetched: crate::nostr::FetchedNote,
    storage: &PostStorage,
    config: &Config,
    cache_id: &str,
    nostr_id: &str,
    base_dir: &str,
) -> Option<Arc<Post>> {
    if fetched.content.len() > config.limits.content_max_length {
        return None;
    }
    if !is_valid_post_id(cache_id) || !is_valid_post_id(&fetched.id_hex) {
        return None;
    }
    let keys = public_lookup_ids(cache_id, &fetched);
    if keys.is_empty() {
        return None;
    }
    if let Some(existing) = load_any_post(&keys, storage, config, base_dir) {
        return Some(ensure_short_link(
            existing, &keys, nostr_id, storage, config, base_dir,
        ));
    }
    let title = nonograph_parser::sanitize_text(&fetched.title);
    let title = if title.is_empty() {
        "Untitled".to_string()
    } else {
        title
    };
    let author = nonograph_parser::sanitize_text(&fetched.author);
    if title.len() > config.limits.title_max_length {
        return None;
    }
    if author.len() > config.limits.alias_max_length {
        return None;
    }
    let created_at = DateTime::from_timestamp(fetched.created_at, 0).unwrap_or_else(Utc::now);
    let rendered_content =
        nonograph_parser::render_markdown_with_config(&fetched.content, &render_options(config));
    let stored_nostr_id = is_nostr_identifier(nostr_id).then(|| nostr_id.to_string());

    let _persist = public_note_files().lock().unwrap();
    if let Some(existing) = load_any_post(&keys, storage, config, base_dir) {
        return Some(shorten_and_store(
            with_nostr_id(existing, nostr_id),
            &keys,
            storage,
            base_dir,
        ));
    }
    let post = Arc::new(Post {
        id: next_short_id(&title, &keys, storage, base_dir)
            .unwrap_or_else(|| fetched.id_hex.clone()),
        title,
        author,
        content: rendered_content,
        raw_content: fetched.content,
        created_at,
        nostr_id: stored_nostr_id,
    });
    store_public_note(post, &keys, storage, base_dir)
        .or_else(|| load_any_post(&keys, storage, config, base_dir))
}

fn public_note_files() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn public_lookup_ids(cache_id: &str, fetched: &crate::nostr::FetchedNote) -> Vec<String> {
    let mut ids = Vec::new();
    push_lookup_id(&mut ids, cache_id);
    push_lookup_id(&mut ids, &fetched.id_hex);
    if !fetched.identifier.is_empty() {
        push_lookup_id(
            &mut ids,
            &crate::nostr::naddr_cache_id(&crate::nostr::Naddr {
                identifier: fetched.identifier.clone(),
                pubkey: fetched.pubkey,
                kind: crate::nostr::KIND_LONG_FORM,
                relays: Vec::new(),
            }),
        );
    }
    ids
}

fn push_lookup_id(ids: &mut Vec<String>, id: &str) {
    if is_valid_post_id(id) && ids.iter().all(|have| have != id) {
        ids.push(id.to_string());
    }
}

fn load_any_post(
    ids: &[String],
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> Option<Arc<Post>> {
    ids.iter()
        .find_map(|id| load_post_from_disk(id, storage, config, base_dir))
}

fn next_short_id(
    title: &str,
    keys: &[String],
    storage: &PostStorage,
    base_dir: &str,
) -> Option<String> {
    for key in keys {
        if let Some(old) = crate::save::alias_target_in_dir(key, base_dir) {
            return Some(old);
        }
    }
    crate::generate_post_id_in_dir(
        title,
        storage,
        base_dir,
        &crate::generate_unguessable_segment(),
    )
    .ok()
}

fn persist_pointer(key: &str, short_id: &str, base_dir: &str) -> Result<(), ()> {
    if key == short_id {
        return Ok(());
    }
    match crate::save::alias_target_in_dir(key, base_dir) {
        Some(target) if target == short_id => Ok(()),
        Some(target) if crate::save::post_file_exists_in_dir(&target, base_dir) => Err(()),
        _ => {
            let replace = crate::save::post_file_exists_in_dir(key, base_dir);
            crate::save::write_alias_pointer(key, short_id, base_dir, replace).map_err(|_| ())
        }
    }
}

fn persist_public_note_files(post: &Post, keys: &[String], base_dir: &str) -> Result<(), ()> {
    let created = match crate::save::save_post_to_file_in_dir(post, base_dir) {
        Ok(()) => true,
        Err(crate::save::SaveError::AlreadyExists) => false,
        Err(_) => return Err(()),
    };
    for key in keys {
        if persist_pointer(key, &post.id, base_dir).is_err() {
            if created {
                crate::save::remove_post_file_in_dir(&post.id, base_dir);
            }
            return Err(());
        }
    }
    Ok(())
}

fn with_nostr_id(post: Arc<Post>, nostr_id: &str) -> Arc<Post> {
    if post.nostr_id.as_deref().is_some_and(is_nostr_identifier) {
        return post;
    }
    if !is_nostr_identifier(nostr_id) {
        return post;
    }
    let mut copy = (*post).clone();
    copy.nostr_id = Some(nostr_id.to_string());
    Arc::new(copy)
}

fn needs_persist(post: &Post, keys: &[String], base_dir: &str) -> bool {
    keys.iter().any(|key| {
        crate::save::alias_target_in_dir(key, base_dir).as_deref() != Some(post.id.as_str())
    })
}

fn ensure_short_link(
    post: Arc<Post>,
    keys: &[String],
    nostr_id: &str,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> Arc<Post> {
    let post = with_nostr_id(post, nostr_id);
    if !needs_persist(&post, keys, base_dir) {
        return post;
    }
    let _persist = public_note_files().lock().unwrap();
    let post = match load_any_post(keys, storage, config, base_dir) {
        Some(existing) => with_nostr_id(existing, nostr_id),
        None => post,
    };
    shorten_and_store(post, keys, storage, base_dir)
}

fn shorten_and_store(
    post: Arc<Post>,
    keys: &[String],
    storage: &PostStorage,
    base_dir: &str,
) -> Arc<Post> {
    let post = if keys.iter().any(|key| key == &post.id) {
        match next_short_id(&post.title, keys, storage, base_dir).filter(|id| !keys.contains(id)) {
            Some(short_id) => {
                let mut upgraded = (*post).clone();
                upgraded.id = short_id;
                Arc::new(upgraded)
            }
            None => post,
        }
    } else {
        post
    };
    store_public_note(Arc::clone(&post), keys, storage, base_dir).unwrap_or(post)
}

fn store_public_note(
    post: Arc<Post>,
    keys: &[String],
    storage: &PostStorage,
    base_dir: &str,
) -> Option<Arc<Post>> {
    persist_public_note_files(&post, keys, base_dir).ok()?;
    remember_post(storage, Arc::clone(&post), keys);
    Some(post)
}

fn remember_post(storage: &PostStorage, post: Arc<Post>, keys: &[String]) {
    let mut cache = storage.write().unwrap();
    cache.insert(post.id.clone(), Arc::clone(&post));
    for key in keys {
        if key != &post.id {
            cache.insert(key.clone(), Arc::clone(&post));
        }
    }
}

const MAX_PUBLIC_FETCHES: usize = 8;
static PUBLIC_FETCHES: AtomicUsize = AtomicUsize::new(0);

struct PublicFetchSlot;

impl PublicFetchSlot {
    fn acquire() -> Option<Self> {
        if try_acquire_public_fetch(&PUBLIC_FETCHES, MAX_PUBLIC_FETCHES) {
            Some(Self)
        } else {
            None
        }
    }
}

impl Drop for PublicFetchSlot {
    fn drop(&mut self) {
        release_public_fetch(&PUBLIC_FETCHES);
    }
}

fn try_acquire_public_fetch(counter: &AtomicUsize, max: usize) -> bool {
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current >= max {
            return false;
        }
        if counter
            .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
    }
}

fn release_public_fetch(counter: &AtomicUsize) {
    counter.fetch_sub(1, Ordering::Release);
}

struct PublicFetchBusy;

async fn fetch_public(
    hints: &[String],
    kind: Option<u32>,
    cache_id: Option<String>,
    nostr_id: &str,
    storage: &PostStorage,
    config: &Config,
    fetch: impl FnOnce(Vec<String>, Duration) -> Option<crate::nostr::FetchedNote> + Send + 'static,
) -> Result<Option<Arc<Post>>, PublicFetchBusy> {
    if !may_fetch_public_kind(kind) {
        return Ok(None);
    }
    let relays = relays_for_public_fetch(hints, &config.nostr.relays);
    if relays.is_empty() {
        return Ok(None);
    }
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err(PublicFetchBusy);
    };
    let timeout = relay_timeout(config);
    let fetched = match rocket::tokio::task::spawn_blocking(move || fetch(relays, timeout)).await {
        Ok(note) => note,
        Err(_) => return Ok(None),
    };
    Ok(fetched.and_then(|note| {
        let cache_id = cache_id.unwrap_or_else(|| note.id_hex.clone());
        post_from_public_note(note, storage, config, &cache_id, nostr_id, ".")
    }))
}

async fn render_post(
    post_id: &str,
    storage: &State<PostStorage>,
    config: &State<Config>,
    nojs: bool,
) -> Result<ViewPage, ViewErr> {
    let decoded = crate::nostr::decode_nevent(post_id);
    let naddr = if decoded.is_none() {
        crate::nostr::decode_naddr(post_id)
    } else {
        None
    };
    let leftover_id = decoded.as_ref().map(|nevent| nevent.event_id_hex.clone());
    let naddr_id = naddr.as_ref().map(crate::nostr::naddr_cache_id);
    let is_nostr_path = leftover_id.is_some() || naddr_id.is_some();
    let is_raw_request = !is_nostr_path && post_id.ends_with(".md");
    let file_id = leftover_id
        .as_deref()
        .or(naddr_id.as_deref())
        .unwrap_or_else(|| post_id.strip_suffix(".md").unwrap_or(post_id));

    // Reject identifiers that could escape the content directory before any
    // filesystem access takes place. See `is_valid_post_id`.
    if !is_valid_post_id(file_id) {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(not_found_html())),
        ));
    }

    if is_raw_request {
        return match crate::save::read_post_file_in_dir(file_id, ".") {
            Some(raw_bytes) => Ok(ViewPage::Page(rocket::Either::Right(content::RawText(
                raw_bytes,
            )))),
            None => Err((
                Status::NotFound,
                rocket::Either::Left(content::RawText("Page not found".to_string())),
            )),
        };
    }

    if !is_nostr_path && is_static_page(file_id) {
        return match serve_static_page(file_id, config, nojs) {
            Ok(html) => Ok(ViewPage::Page(rocket::Either::Left(html))),
            Err((status, body)) => Err((status, rocket::Either::Right(body))),
        };
    }

    let cached = {
        let posts = storage.read().unwrap();
        posts.lookup(file_id, nojs, file_id)
    };
    if let Some(hit) = cached {
        let live = crate::save::post_file_exists_in_dir(&hit.post.id, ".");
        let leftover = is_nostr_path && hit.post.id == file_id;
        if live && !leftover {
            hit.note_access();
            let post = if is_nostr_path {
                with_nostr_id(Arc::clone(&hit.post), post_id)
            } else {
                Arc::clone(&hit.post)
            };
            if post.nostr_id != hit.post.nostr_id {
                remember_post(storage, Arc::clone(&post), &[]);
            }
            if let Some(href) = short_view_href(nojs, &post.id, post_id) {
                return Ok(ViewPage::Redirect(rocket::response::Redirect::to(href)));
            }
            if post.nostr_id == hit.post.nostr_id {
                if let Some(html) = hit.html {
                    return Ok(ViewPage::Page(rocket::Either::Left(content::RawHtml(
                        html.to_string(),
                    ))));
                }
            }
            return view_post_page(storage, file_id, &post, nojs, post_id);
        }
    }

    let kind = decoded
        .as_ref()
        .and_then(|nevent| nevent.kind)
        .or_else(|| naddr.as_ref().map(|naddr| naddr.kind));
    let local = load_post_from_disk(file_id, storage, config, ".");
    if let Some(html) = refuse_unopened_wrap(kind, local.is_some()) {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(html)),
        ));
    }
    let fetched = if let Some(post) = local {
        let post = if is_nostr_path && may_fetch_public_kind(kind) {
            ensure_short_link(post, &[file_id.to_string()], post_id, storage, config, ".")
        } else {
            post
        };
        Ok(Some(post))
    } else if let Some(nevent) = &decoded {
        let event_id = nevent.event_id_hex.clone();
        fetch_public(
            &nevent.relays,
            nevent.kind,
            None,
            post_id,
            storage,
            config,
            move |relays, timeout| crate::nostr::fetch_public_note(&relays, &event_id, timeout),
        )
        .await
    } else if let Some(naddr) = &naddr {
        let hints = naddr.relays.clone();
        let kind = Some(naddr.kind);
        let cache_id = crate::nostr::naddr_cache_id(naddr);
        let naddr = naddr.clone();
        fetch_public(
            &hints,
            kind,
            Some(cache_id),
            post_id,
            storage,
            config,
            move |relays, timeout| crate::nostr::fetch_public_addr(&relays, &naddr, timeout),
        )
        .await
    } else {
        Ok(None)
    };
    let post = match fetched {
        Ok(post) => post,
        Err(PublicFetchBusy) => {
            return Err((
                Status::ServiceUnavailable,
                rocket::Either::Left(content::RawText("Service unavailable".to_string())),
            ));
        }
    };

    match post {
        Some(post) => view_post_page(storage, file_id, &post, nojs, post_id),
        None => Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(not_found_html())),
        )),
    }
}

fn view_post_page(
    storage: &State<PostStorage>,
    file_id: &str,
    post: &Post,
    nojs: bool,
    requested: &str,
) -> Result<ViewPage, ViewErr> {
    if let Some(href) = short_view_href(nojs, &post.id, requested) {
        return Ok(ViewPage::Redirect(rocket::response::Redirect::to(href)));
    }
    serve_article(storage, file_id, post, nojs, &post.id).map(ViewPage::Page)
}

fn load_post_from_disk(
    file_id: &str,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> Option<Arc<Post>> {
    load_post_from_disk_at(file_id, storage, config, base_dir, 0)
}

fn load_post_from_disk_at(
    file_id: &str,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
    depth: u8,
) -> Option<Arc<Post>> {
    let file_content = std::fs::read_to_string(crate::save::post_path(file_id, base_dir)).ok()?;
    if depth == 0 {
        if let Some(alias) = crate::save::alias_target(&file_content, file_id) {
            let post = load_post_from_disk_at(&alias, storage, config, base_dir, 1)?;
            remember_post(storage, Arc::clone(&post), &[file_id.to_string()]);
            return Some(post);
        }
    }
    let (title, author, created_at, raw_content) = parse_frontmatter(&file_content)?;
    let nostr_id =
        yaml_frontmatter_field(&file_content, "nostr:").filter(|id| is_nostr_identifier(id));
    let post = Arc::new(Post {
        id: file_id.to_string(),
        title,
        author,
        content: nonograph_parser::render_markdown_with_config(
            &raw_content,
            &render_options(config),
        ),
        raw_content,
        created_at,
        nostr_id,
    });
    remember_post(storage, Arc::clone(&post), &[file_id.to_string()]);
    Some(post)
}

fn serve_article(
    storage: &State<PostStorage>,
    file_id: &str,
    post: &Post,
    nojs: bool,
    public_id: &str,
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
    match article_html(post, nojs, public_id) {
        Ok(html) => {
            storage.write().unwrap().remember_html(
                file_id,
                nojs,
                public_id,
                Arc::from(html.as_str()),
            );
            Ok(rocket::Either::Left(content::RawHtml(html)))
        }
        Err(error) => Ok(rocket::Either::Left(content::RawHtml(format!(
            "Template error: {error}"
        )))),
    }
}

fn article_html(post: &Post, nojs: bool, public_id: &str) -> Result<String, String> {
    let nostr_id = post
        .nostr_id
        .as_deref()
        .filter(|id| is_nostr_identifier(id))
        .unwrap_or("");
    let mut context = article_context(
        &post.title,
        &post.author,
        Some(post.created_at),
        &format!("/{public_id}"),
        &post_description(&post.raw_content),
        nojs,
        public_id,
    );
    context.insert("content".to_string(), post.content.clone());
    let nostr_link = if nostr_id.is_empty() {
        String::new()
    } else {
        let id = nonograph_parser::html_attr_escape(nostr_id);
        format!("<a href=\"/{id}\" class=\"nostr-id\">{id}</a>")
    };
    context.insert("nostr_link".to_string(), nostr_link);
    template::shared().render("post", &context)
}

fn centered_notice(document_title: &str, heading: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{document_title}</title>
    <link rel="icon" href="data:," />
    <style>
        body {{
            max-width: 720px;
            margin: 0 auto;
            padding: 40px 20px;
            text-align: center;
            color: #333;
        }}
        h1 {{ font-weight: 300; margin-bottom: 16px; }}
        a {{ color: #333; }}
    </style>
</head>
<body>
    <h1>{heading}</h1>
    {body}
</body>
</html>"#
    )
}

pub(crate) fn wrap_refused_html() -> String {
    centered_notice(
        "Open this in the extension",
        "Open this in the extension",
        "<p>This is a private note. This site does not decrypt it.</p>\n    <p><a href=\"/\">Write Your Own</a></p>",
    )
}

fn not_found_html() -> String {
    centered_notice(
        "404 - Page not found",
        "Page Not Found",
        "<p><a href=\"/\">Write Your Own</a></p>",
    )
}

#[get("/page/nonograph_page.js")]
pub fn parser_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, PAGE_JS)
}

#[get("/page/nonograph_page_bg.wasm")]
pub fn parser_wasm() -> (ContentType, &'static [u8]) {
    (ContentType::new("application", "wasm"), PAGE_WASM)
}

#[get("/home.css")]
pub fn home_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().home_css.as_str())
}

#[get("/home.js")]
pub fn home_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().home_js.as_str())
}

#[get("/nostr.js")]
pub fn nostr_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().nostr_js.as_str())
}

#[get("/secp256k1.js")]
pub fn secp256k1_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().secp256k1_js.as_str())
}

#[get("/post.css")]
pub fn post_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().post_css.as_str())
}

#[get("/post.js")]
pub fn post_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().post_js.as_str())
}

#[get("/post-noscript.css")]
pub fn post_noscript_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().post_noscript_css.as_str())
}

#[get("/writemark.js")]
pub fn writemark_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().writemark_js.as_str())
}

const ROBOTS_TXT: &str = include_str!("../robots.txt");

#[get("/robots.txt")]
pub fn robots_txt() -> content::RawText<&'static str> {
    content::RawText(ROBOTS_TXT)
}

#[cfg(test)]
#[path = "../test/pages.rs"]
mod tests;
