use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rocket::http::{ContentType, Status};
use rocket::response::content;
use rocket::State;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::csrf::{self, CsrfProtected};
use crate::publish::{self, publish_failure_redirect, publish_note};
use crate::template;
use crate::{
    is_valid_post_id, parse_legacy_frontmatter, parse_yaml_frontmatter, render_options, Post,
    PostStorage,
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
// Stable renderer URLs have no content hash. Revalidate with an ETag so a new
// build is picked up, and a matching tag answers 304 instead of resending the wasm.
const REVALIDATE: &str = "public, no-cache";

const STATIC_PAGES: &[&str] = &["about", "legal", "markup", "api"];

pub fn cache_control_for_path(path: &str) -> &'static str {
    match path {
        PAGE_JS_PATH | PAGE_WASM_PATH => REVALIDATE,
        // CSS and site scripts are content-hashed in the URL, so those can be immutable.
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

async fn handle_create(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
) -> rocket::response::Redirect {
    let home = if nojs { "/nojs" } else { "/" };
    if config.security.csrf_protection_enabled && !csrf::is_valid_csrf_token(&form.csrf_token) {
        return rocket::response::Redirect::to(format!("{home}?error=csrf_token_invalid"));
    }

    let alias = if form.alias.trim().is_empty() {
        None
    } else {
        Some(form.alias.as_str())
    };
    if let Err(error) = config.validate_post(&form.title, &form.content, alias) {
        return rocket::response::Redirect::to(format!("{home}?error={error}"));
    }

    let rendered_content =
        nonograph_parser::render_markdown_with_config(&form.content, &render_options(config));
    let storage = storage.clone();
    let title = form.title.clone();
    let author = form.alias.clone();
    let raw = form.content.clone();
    let published = rocket::tokio::task::spawn_blocking(move || {
        publish_note(&storage, &title, &author, &rendered_content, &raw)
    })
    .await;
    let published = match published {
        Ok(result) => result,
        Err(_) => Err(publish::PublishFailure::Save(
            "publishing was interrupted".to_string(),
        )),
    };
    match published {
        Ok(post_id) => rocket::response::Redirect::to(publish::published_href(nojs, &post_id)),
        Err(failure) => publish_failure_redirect(nojs, failure),
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
    let parsed = if file_content.starts_with("---\n") {
        parse_yaml_frontmatter(&file_content)
    } else {
        parse_legacy_frontmatter(&file_content)
    };
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
    // Stop at the 161st character. A longer post must not be walked to the end.
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
}

#[get("/<post_id>")]
pub fn view_post(
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
    render_post(post_id, storage, config, false)
}

#[get("/nojs/<post_id>")]
pub fn nojs_view_post(
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
    render_post(post_id, storage, config, true)
}

/// Old wrap links (`nevent1…`) still name the markdown file by event id.
pub(crate) fn leftover_wrap_file_id(post_id: &str) -> Option<String> {
    crate::nostr::decode_nevent(post_id).map(|nevent| nevent.event_id_hex)
}

fn render_post(
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
    let leftover_id = leftover_wrap_file_id(post_id);
    let is_raw_request = leftover_id.is_none() && post_id.ends_with(".md");
    let file_id = match &leftover_id {
        Some(id) => id.as_str(),
        None => post_id.strip_suffix(".md").unwrap_or(post_id),
    };

    if !is_valid_post_id(file_id) {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(NOT_FOUND_HTML.to_string())),
        ));
    }

    if is_raw_request {
        let file_path = format!("content/{file_id}.md");
        return match std::fs::read_to_string(&file_path) {
            Ok(raw_bytes) => Ok(rocket::Either::Right(content::RawText(raw_bytes))),
            Err(_) => Err((
                Status::NotFound,
                rocket::Either::Left(content::RawText("Page not found".to_string())),
            )),
        };
    }

    if leftover_id.is_none() && is_static_page(file_id) {
        return match serve_static_page(file_id, config, nojs) {
            Ok(html) => Ok(rocket::Either::Left(html)),
            Err((status, body)) => Err((status, rocket::Either::Right(body))),
        };
    }

    let public_id = if leftover_id.is_some() {
        post_id
    } else {
        file_id
    };
    let cached = {
        let posts = storage.read().unwrap();
        posts.lookup(file_id, nojs, public_id)
    };
    if let Some(hit) = cached {
        hit.note_access();
        if let Some(html) = hit.html {
            return Ok(rocket::Either::Left(content::RawHtml(html.to_string())));
        }
        return serve_article(storage, file_id, &hit.post, nojs, public_id);
    }

    let post = if crate::save::post_file_exists(file_id) {
        load_post_from_disk(file_id, storage, config)
    } else {
        None
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
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Option<Arc<Post>> {
    let file_content = std::fs::read_to_string(format!("content/{file_id}.md")).ok()?;
    let parsed = if file_content.starts_with("---\n") {
        parse_yaml_frontmatter(&file_content)
    } else {
        parse_legacy_frontmatter(&file_content)
    };
    let (title, author, created_at, raw_content) = parsed?;
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
    });
    storage
        .write()
        .unwrap()
        .insert(file_id.to_string(), Arc::clone(&post));
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
    context.insert("url".to_string(), format!("/{public_id}"));
    context.insert(
        "description".to_string(),
        post_description(&post.raw_content),
    );
    fill_page_chrome(&mut context, nojs, public_id);
    template::shared().render("post", &context)
}

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
