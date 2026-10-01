use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rocket::http::{ContentType, Status};
use rocket::response::content;
use rocket::State;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::csrf::{self, CsrfProtected};
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
    match path {
        PAGE_JS_PATH | PAGE_WASM_PATH => REVALIDATE,
        HOME_CSS_PATH
        | HOME_JS_PATH
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
        let post_css = read("post.css");
        let post_js = read("post.js");
        let post_noscript_css = read("post-noscript.css");
        let writemark_js = read("writemark.js");
        let version = asset_version(&[
            &home_css,
            &home_js,
            &post_css,
            &post_js,
            &post_noscript_css,
            &writemark_js,
        ]);
        Assets {
            version,
            home_css,
            home_js,
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

fn home_content_field(nojs: bool, max_length: &str) -> String {
    if nojs {
        format!(
            "<textarea name=\"content\" placeholder=\"Write something with markdown...\" maxlength=\"{max_length}\" title=\"Mind your opsec, be careful what you share.\" required></textarea>"
        )
    } else {
        format!(
            "<writemark-editor name=\"content\" mode=\"live\" placeholder=\"Write something with markdown or type / for formatting options...\" maxlength=\"{max_length}\" title=\"Mind your opsec, be careful what you share.\" required></writemark-editor>"
        )
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
        "nostr_publish_failed" => "Publishing failed. Try again.".to_string(),
        "save_failed" => "Saving the post failed. Try again.".to_string(),
        "no_available_slots" => "Could not pick an address for this post. Try again.".to_string(),
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
        "content_field".to_string(),
        home_content_field(nojs, &max_length),
    );
    context.insert("error".to_string(), home_error_message(error));
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
    context
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

#[derive(FromForm)]
pub(crate) struct NewPost {
    title: String,
    content: String,
    alias: String,
    csrf_token: String,
}

pub(crate) fn create_location_in_dir(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
    base_dir: &str,
) -> String {
    let home = if nojs { "/nojs" } else { "/" };
    if config.security.csrf_protection_enabled && !csrf::is_valid_csrf_token(&form.csrf_token) {
        return format!("{home}?error=csrf_token_invalid");
    }

    let alias = if form.alias.trim().is_empty() {
        None
    } else {
        Some(form.alias.as_str())
    };
    if let Err(error) = config.validate_post(&form.title, &form.content, alias) {
        return format!("{home}?error={error}");
    }

    let rendered_content =
        nonograph_parser::render_markdown_with_config(&form.content, &render_options(config));
    match publish::publish_note_in_dir(
        storage,
        &form.title,
        &form.alias,
        &rendered_content,
        &form.content,
        base_dir,
    ) {
        Ok(post_id) => publish::published_href(nojs, &post_id),
        Err(failure) => publish::publish_failure_href(nojs, failure),
    }
}

async fn handle_create(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
) -> rocket::response::Redirect {
    let storage = storage.clone();
    let config = config.clone();
    let form = NewPost {
        title: form.title.clone(),
        content: form.content.clone(),
        alias: form.alias.clone(),
        csrf_token: form.csrf_token.clone(),
    };
    match rocket::tokio::task::spawn_blocking(move || {
        create_location_in_dir(nojs, &form, &storage, &config, ".")
    })
    .await
    {
        Ok(location) => rocket::response::Redirect::to(location),
        Err(_) => publish::publish_failure_redirect(
            nojs,
            publish::PublishFailure::Save("publishing was interrupted".to_string()),
        ),
    }
}

#[post("/create", data = "<form>")]
pub async fn create_post(
    _csrf: CsrfProtected,
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<rocket::response::Redirect, content::RawHtml<String>> {
    Ok(handle_create(false, &form, storage, config).await)
}

#[post("/nojs/create", data = "<form>")]
pub async fn nojs_create_post(
    _csrf: CsrfProtected,
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<rocket::response::Redirect, content::RawHtml<String>> {
    Ok(handle_create(true, &form, storage, config).await)
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

enum BuiltPage {
    Ready(String),
    Invalid(String),
    Missing,
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
        BuiltPage::Missing => Err((
            Status::NotFound,
            content::RawHtml(NOT_FOUND_HTML.to_string()),
        )),
    }
}

fn build_static_page(page_name: &str, config: &Config, nojs: bool) -> BuiltPage {
    let Ok(file_content) = std::fs::read_to_string(format!("content/{page_name}.md")) else {
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
    let mut context = HashMap::new();
    context.insert("title".to_string(), title);
    context.insert("content".to_string(), rendered_content);
    context.insert(
        "created_at".to_string(),
        created_at.format("%B %d, %Y").to_string(),
    );
    context.insert("author".to_string(), author);
    context.insert("author_display".to_string(), String::new());
    context.insert(
        "created_at_iso".to_string(),
        created_at.format("%Y-%m-%dT00:00:00+00:00").to_string(),
    );
    context.insert("url".to_string(), format!("/{page_name}"));
    context.insert("description".to_string(), String::new());
    fill_page_chrome(&mut context, nojs, page_name);
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

#[get("/<post_id>")]
pub async fn view_post(
    post_id: &str,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
    render_post(post_id, storage, config, false).await
}

#[get("/nojs/<post_id>")]
pub async fn nojs_view_post(
    post_id: &str,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
    render_post(post_id, storage, config, true).await
}

pub(crate) fn leftover_wrap_file_id(post_id: &str) -> Option<String> {
    crate::nostr::decode_nevent(post_id).map(|nevent| nevent.event_id_hex)
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
    if !crate::nostr::public_relay_url(relay) {
        return;
    }
    if relays.iter().any(|have| have == relay) {
        return;
    }
    relays.push(relay.to_string());
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

pub(crate) fn may_fetch_public_kind(kind: Option<u32>) -> bool {
    matches!(kind, None | Some(crate::nostr::KIND_LONG_FORM))
}

pub(crate) fn is_gift_wrap(kind: Option<u32>) -> bool {
    kind == Some(crate::nostr::KIND_GIFT_WRAP)
}

pub(crate) fn refuse_unopened_wrap(
    kind: Option<u32>,
    has_local_file: bool,
) -> Option<&'static str> {
    if has_local_file || !is_gift_wrap(kind) {
        None
    } else {
        Some(WRAP_REFUSED_HTML)
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
    let timeout = Duration::from_secs(config.nostr.timeout_secs.max(1));
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
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
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
            rocket::Either::Right(content::RawHtml(NOT_FOUND_HTML.to_string())),
        ));
    }

    if is_raw_request {
        return match crate::save::read_post_file_in_dir(file_id, ".") {
            Some(raw_bytes) => Ok(rocket::Either::Right(content::RawText(raw_bytes))),
            None => Err((
                Status::NotFound,
                rocket::Either::Left(content::RawText("Page not found".to_string())),
            )),
        };
    }

    if !is_nostr_path && is_static_page(file_id) {
        return match serve_static_page(file_id, config, nojs) {
            Ok(html) => Ok(rocket::Either::Left(html)),
            Err((status, body)) => Err((status, rocket::Either::Right(body))),
        };
    }

    let public_id = if is_nostr_path { post_id } else { file_id };
    // Try to load from memory first with minimal lock time
    let cached = {
        let posts = storage.read().unwrap();
        posts.lookup(file_id, nojs, public_id)
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
            } else if let Some(html) = hit.html {
                return Ok(rocket::Either::Left(content::RawHtml(html.to_string())));
            }
            return serve_article(storage, file_id, &post, nojs, public_id);
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
            rocket::Either::Right(content::RawHtml(html.to_string())),
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
        Some(post) => serve_article(storage, file_id, &post, nojs, public_id),
        None => Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(NOT_FOUND_HTML.to_string())),
        )),
    }
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
    let mut context = HashMap::new();
    context.insert("title".to_string(), post.title.clone());
    context.insert("content".to_string(), post.content.clone());
    let author = if post.author.is_empty() {
        "Anonymous".to_string()
    } else {
        post.author.clone()
    };
    context.insert("author".to_string(), author);
    let author_display = if post.author.is_empty() {
        "Anonymous · ".to_string()
    } else {
        format!("{} · ", post.author)
    };
    context.insert("author_display".to_string(), author_display);
    context.insert(
        "created_at".to_string(),
        post.created_at.format("%B %d, %Y").to_string(),
    );
    context.insert(
        "created_at_iso".to_string(),
        post.created_at
            .format("%Y-%m-%dT00:00:00+00:00")
            .to_string(),
    );
    // OpenGraph variables
    let nostr_id = post
        .nostr_id
        .as_deref()
        .filter(|id| is_nostr_identifier(id))
        .unwrap_or("");
    let url_id = if nostr_id.is_empty() {
        public_id
    } else {
        nostr_id
    };
    context.insert("url".to_string(), format!("/{url_id}"));
    context.insert(
        "description".to_string(),
        post_description(&post.raw_content),
    );
    fill_page_chrome(&mut context, nojs, public_id);
    let nostr_link = if nostr_id.is_empty() {
        String::new()
    } else {
        let id = nonograph_parser::html_attr_escape(nostr_id);
        format!("<a href=\"/{id}\" class=\"nostr-id\">{id}</a>")
    };
    context.insert("nostr_link".to_string(), nostr_link);
    template::shared().render("post", &context)
}

pub(crate) const WRAP_REFUSED_HTML: &str = r#"<!doctype html>
<html>
<head>
    <title>Open this in the extension</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
        body {
            max-width: 720px;
            margin: 0 auto;
            padding: 40px 20px;
            text-align: center;
            color: #333;
        }
        h1 { font-weight: 300; margin-bottom: 16px; }
        a { color: #333; }
    </style>
</head>
<body>
    <h1>Open this in the extension</h1>
    <p>This is a private note. This site does not decrypt it.</p>
    <p><a href="/">Write Your Own</a></p>
</body>
</html>"#;

const NOT_FOUND_HTML: &str = r#"<!doctype html>
<html>
<head>
    <title>404 - Page not found</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
        body {
            max-width: 720px;
            margin: 0 auto;
            padding: 40px 20px;
            text-align: center;
            color: #333;
        }
        h1 { font-weight: 300; margin-bottom: 16px; }
        a { color: #333; }
    </style>
</head>
<body>
    <h1>Page Not Found</h1>
    <p><a href="/">Write Your Own</a></p>
</body>
</html>"#;

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
