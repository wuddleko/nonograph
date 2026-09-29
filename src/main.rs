#[macro_use]
extern crate rocket;

mod archiver;
mod config;
mod nip44;
mod nojs;
mod nostr;
mod save;
mod template;

use config::Config;
use nonograph_parser as parser;
use std::thread;

use chrono::{DateTime, Utc};
#[cfg(test)]
use deunicode::deunicode;
#[cfg(test)]
use rand::{thread_rng, Rng};
use rocket::{
    fairing::{Fairing, Info, Kind},
    http::{ContentType, Header, Status},
    request::{FromRequest, Outcome},
    response::content,
    Request, Response, State,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use std::sync::{Arc, Mutex};
use template::TemplateEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Post {
    id: String,
    title: String,
    author: String,
    content: String,
    raw_content: String,
    created_at: DateTime<Utc>,
}

impl Post {
    fn memory_size(&self) -> usize {
        self.id.len()
            + self.title.len()
            + self.author.len()
            + self.content.len()
            + self.raw_content.len()
            + 64 // Rough estimate for DateTime and struct overhead
    }
}

#[derive(Debug)]
struct CacheEntry {
    post: Post,
    last_accessed: DateTime<Utc>,
}

#[derive(Debug)]
struct PostCache {
    entries: HashMap<String, CacheEntry>,
    total_size: usize,
    max_size: usize, // 128 MB = 128 * 1024 * 1024
}

impl PostCache {
    fn new(max_size_mb: usize) -> Self {
        PostCache {
            entries: HashMap::new(),
            total_size: 0,
            max_size: max_size_mb * 1024 * 1024,
        }
    }

    // Add a non-cloning get for read-only access
    fn get_ref(&mut self, post_id: &str) -> Option<&Post> {
        if let Some(entry) = self.entries.get_mut(post_id) {
            entry.last_accessed = Utc::now();
            Some(&entry.post)
        } else {
            None
        }
    }

    #[cfg(test)]
    fn contains_key(&self, post_id: &str) -> bool {
        self.entries.contains_key(post_id)
    }

    fn insert(&mut self, post_id: String, post: Post) {
        let post_size = post.memory_size();

        // Remove existing entry if it exists
        if let Some(old_entry) = self.entries.remove(&post_id) {
            self.total_size -= old_entry.post.memory_size();
            println!("Nonograph: Cache UPDATE for post: {}", post_id);
        } else {
            println!("Nonograph: Cache INSERT for post: {}", post_id);
        }

        // Add new entry size
        self.total_size += post_size;

        // Evict oldest entries if over limit
        let mut evicted_count = 0;
        while self.total_size > self.max_size && !self.entries.is_empty() {
            self.evict_oldest();
            evicted_count += 1;
        }

        if evicted_count > 0 {
            println!(
                "Nonograph: Cache EVICT {} old posts to stay under 128MB limit",
                evicted_count
            );
        }

        // Insert new entry
        let entry = CacheEntry {
            post,
            last_accessed: Utc::now(),
        };

        self.entries.insert(post_id.clone(), entry);
        let (size_val, size_unit) = match self.total_size {
            b if b < 1_024 => (b as f64, "B"),
            b if b < 1_024 * 1_024 => (b as f64 / 1_024.0, "KB"),
            b if b < 1_024 * 1_024 * 1_024 => (b as f64 / (1_024.0 * 1_024.0), "MB"),
            b => (b as f64 / (1_024.0 * 1_024.0 * 1_024.0), "GB"),
        };
        println!(
            "Nonograph: Cache now contains {} posts, total size: {:.2} {}",
            self.entries.len(),
            size_val,
            size_unit
        );
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_id) = self.find_oldest_entry() {
            if let Some(old_entry) = self.entries.remove(&oldest_id) {
                self.total_size -= old_entry.post.memory_size();
                println!(
                    "Nonograph: Cache EVICT for post: {} (freed: {} KB)",
                    oldest_id,
                    old_entry.post.memory_size() / 1024
                );
            }
        }
    }

    fn find_oldest_entry(&self) -> Option<String> {
        self.entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(id, _)| id.clone())
    }

    fn purge_deleted(&mut self) {
        let stale: Vec<String> = self
            .entries
            .keys()
            .filter(|id| !std::path::Path::new(&format!("content/{}.md", id)).exists())
            .cloned()
            .collect();

        for id in stale {
            if let Some(entry) = self.entries.remove(&id) {
                self.total_size -= entry.post.memory_size();
                println!("Nonograph: Cache EVICT for post: {}.md", id);
            }
        }
    }
}

type PostStorage = Arc<Mutex<PostCache>>;

#[get("/")]
fn index(config: &State<Config>) -> content::RawHtml<String> {
    let engine = TemplateEngine::new("templates");
    let mut context = HashMap::new();
    context.insert("error".to_string(), "".to_string());
    context.insert("success".to_string(), "".to_string());
    context.insert(
        "title_max_length".to_string(),
        config.limits.title_max_length.to_string(),
    );
    context.insert(
        "alias_max_length".to_string(),
        config.limits.alias_max_length.to_string(),
    );
    context.insert(
        "content_max_length".to_string(),
        config.limits.content_max_length.to_string(),
    );

    let csrf_token = if config.security.csrf_protection_enabled {
        generate_csrf_token_with_timestamp()
    } else {
        String::new()
    };
    context.insert("csrf_token".to_string(), csrf_token);

    match engine.render_with_defaults("home", &context) {
        Ok(html) => content::RawHtml(html),
        Err(e) => content::RawHtml(format!("Template error: {}", e)),
    }
}

#[derive(FromForm)]
struct NewPost {
    title: String,
    content: String,
    alias: String,
    csrf_token: String,
}

struct OnionLocationFairing {
    onion_url: String,
}

#[rocket::async_trait]
impl Fairing for OnionLocationFairing {
    fn info(&self) -> Info {
        Info {
            name: "Onion-Location header",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        if !response.status().class().is_success() {
            return;
        }
        let is_html = response
            .content_type()
            .map(|ct| ct.is_html())
            .unwrap_or(false);
        if !is_html {
            return;
        }

        let host_is_onion = request
            .host()
            .map(|h| h.domain().as_str().ends_with(".onion"))
            .unwrap_or(false);
        let forwarded_https = request
            .headers()
            .get_one("X-Forwarded-Proto")
            .map(|p| p.eq_ignore_ascii_case("https"))
            .unwrap_or(false);
        if !host_is_onion && !forwarded_https {
            return;
        }

        response.set_header(Header::new("Onion-Location", self.onion_url.clone()));
    }
}

// Add security headers to every response.
struct SecurityHeadersFairing;

#[rocket::async_trait]
impl Fairing for SecurityHeadersFairing {
    fn info(&self) -> Info {
        Info {
            name: "Security headers (CSP et al.)",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        // TODO: Refactor HTML and remove unsafe-inline.
        response.set_header(Header::new(
            "Content-Security-Policy",
            "default-src 'self'; \
             base-uri 'self'; \
             form-action 'self'; \
             frame-ancestors 'self'; \
             img-src 'self' https: http:; \
             media-src 'self' https: http:; \
             object-src 'none'; \
             script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; \
             script-src-attr 'none'; \
             style-src 'self' https: 'unsafe-inline'",
        ));
        response.set_header(Header::new("Cross-Origin-Opener-Policy", "same-origin"));
        response.set_header(Header::new("Cross-Origin-Resource-Policy", "same-origin"));
        response.set_header(Header::new("Origin-Agent-Cluster", "?1"));
        response.set_header(Header::new("Referrer-Policy", "no-referrer"));
        response.set_header(Header::new(
            "Strict-Transport-Security",
            "max-age=15552000; includeSubDomains",
        ));
        response.set_header(Header::new("X-Content-Type-Options", "nosniff"));
        response.set_header(Header::new("X-DNS-Prefetch-Control", "off"));
        response.set_header(Header::new("X-Download-Options", "noopen"));
        response.set_header(Header::new("X-Frame-Options", "SAMEORIGIN"));
        response.set_header(Header::new("X-Permitted-Cross-Domain-Policies", "none"));
        response.set_header(Header::new("X-XSS-Protection", "0"));
        response.set_header(Header::new(
            "Cache-Control",
            cache_control_for_path(request.uri().path().as_str()),
        ));
    }
}

const PAGE_JS_PATH: &str = "/page/nonograph_page.js";
const PAGE_WASM_PATH: &str = "/page/nonograph_page_bg.wasm";

fn cache_control_for_path(path: &str) -> &'static str {
    if path == PAGE_JS_PATH || path == PAGE_WASM_PATH {
        "public, max-age=31536000, immutable"
    } else {
        "no-store, max-age=0"
    }
}

struct CsrfProtected;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for CsrfProtected {
    type Error = ();

    async fn from_request(_request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(CsrfProtected)
    }
}

/// Maximum length of a post identifier, matching typical filesystem limits on
/// a single path component.
const MAX_POST_ID_LEN: usize = 255;

/// Returns `true` if `id` is a well-formed post identifier.
///
/// A valid identifier is a non-empty, length-bounded slug composed only of
/// ASCII letters, digits, hyphens, and underscores. Event ids, older
/// slug-date links, static pages, and Telegraph slugs all fit.
///
/// This is the trust boundary for untrusted path input. Because `.`, `/`, and
/// `\` are all rejected, a value that passes this check cannot express a
/// path-traversal sequence such as `../`, so it can be safely interpolated
/// into a `content/{id}.md` path.
fn is_valid_post_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_POST_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 16 random bytes, hex-encoded. Sits between the slug and the date so the
/// address cannot be rebuilt from the title.
#[cfg(test)]
fn generate_unguessable_segment() -> String {
    let mut rng = thread_rng();
    (0..16)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

#[cfg(test)]
fn assemble_post_id(slug: &str, random: &str, date: &str, index: usize) -> String {
    if index == 0 {
        format!("{slug}-{random}-{date}")
    } else {
        format!("{slug}-{random}-{date}-{index}")
    }
}

#[cfg(test)]
fn generate_post_id(title: &str, storage: &PostStorage) -> Result<String, String> {
    generate_post_id_with_segment(title, storage, &generate_unguessable_segment())
}

#[cfg(test)]
fn generate_post_id_with_segment(
    title: &str,
    storage: &PostStorage,
    random: &str,
) -> Result<String, String> {
    let now = Utc::now();
    let date_str = now.format("%m-%d-%Y").to_string();

    // Transliterate ALL characters to ASCII equivalents (safe for all input)
    let transliterated_title = deunicode(title);

    // Create URL-safe slug from transliterated title
    let title_slug: String = transliterated_title
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c)
            } else if c.is_whitespace() || c == '-' || c == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-");

    // Apply character limit with truncation if needed.
    // "-{32 hex}" is 33 characters, then "-{date}".
    let max_slug_length = 250 - date_str.len() - 1 - 33;
    let final_slug = if title_slug.len() > max_slug_length {
        let truncate_to = max_slug_length.saturating_sub(4); // Reserve space for "-etc"
        if truncate_to > 0 {
            // Find the last complete word that fits
            let truncated = &title_slug[..truncate_to];
            let last_dash = truncated.rfind('-').unwrap_or(truncated.len());
            format!("{}-etc", &title_slug[..last_dash])
        } else {
            "etc".to_string()
        }
    } else {
        title_slug
    };

    if final_slug.is_empty() {
        // Use "na-" + 4 random characters only for completely empty titles
        let mut rng = thread_rng();
        let chars: String = (0..4)
            .map(|_| {
                let chars = b"abcdefghijklmnopqrstuvwxyz0123456789";
                chars[rng.gen_range(0..chars.len())] as char
            })
            .collect();

        let fallback_slug = format!("na-{}", chars);
        let posts = storage.lock().unwrap();

        for i in 0..1000 {
            let post_id = assemble_post_id(&fallback_slug, random, &date_str, i);

            if !posts.contains_key(&post_id) {
                return Ok(post_id);
            }
        }

        return Err(
            "All slots for this title and date are taken. Please choose another title.".to_string(),
        );
    }

    let posts = storage.lock().unwrap();

    // Try to find an available slot (0-999)
    for i in 0..1000 {
        let post_id = assemble_post_id(&final_slug, random, &date_str, i);

        if !posts.contains_key(&post_id) {
            return Ok(post_id);
        }
    }

    Err("All slots for this title and date are taken. Please choose another title.".to_string())
}

fn generate_csrf_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect::<String>()
}

fn generate_csrf_token_with_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random_part = generate_csrf_token();
    let combined = format!("{}:{}", timestamp, random_part);

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    combined.hash(&mut hasher);
    let hash = hasher.finish();

    format!("{}.{:x}", combined, hash)
}

fn is_valid_csrf_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    // Split token into data and hash parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 2 {
        return false;
    }

    let data = parts[0];
    let provided_hash = parts[1];

    // Recreate hash from data
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let expected_hash = format!("{:x}", hasher.finish());

    // Verify hash matches
    if provided_hash != expected_hash {
        return false;
    }

    // Check timestamp (token expires after 1 hour)
    let data_parts: Vec<&str> = data.split(':').collect();
    if data_parts.len() != 2 {
        return false;
    }

    if let Ok(timestamp) = data_parts[0].parse::<u64>() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token is valid for 24 hours
        current_time - timestamp < 86400
    } else {
        false
    }
}

fn insert_local_parser(context: &mut HashMap<String, String>, raw_content: &str, config: &Config) {
    context.insert(
        "raw_post_json".to_string(),
        json_for_script(&parser::markdown_for_page(raw_content)),
    );
    context.insert(
        "parser_asset_version".to_string(),
        PARSER_ASSET_VERSION.trim().to_string(),
    );
    context.insert("parser_js_path".to_string(), PAGE_JS_PATH.to_string());
    context.insert("parser_wasm_path".to_string(), PAGE_WASM_PATH.to_string());
    context.insert(
        "syntax_theme".to_string(),
        config.theme.syntax_highlighting.clone(),
    );
    context.insert(
        "max_url_length".to_string(),
        config.security.max_url_length.to_string(),
    );
    context.insert(
        "external_link_security".to_string(),
        config.security.external_link_security.to_string(),
    );
}

fn json_for_script(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

fn render_options(config: &Config) -> parser::RenderOptions {
    parser::RenderOptions {
        max_url_length: config.security.max_url_length,
        external_link_security: config.security.external_link_security,
        syntax_theme: config.theme.syntax_highlighting.clone(),
    }
}

fn publish_failure_redirect(nojs: bool, failure: PublishFailure) -> rocket::response::Redirect {
    let error = match failure {
        PublishFailure::Relays => "nostr_publish_failed",
        PublishFailure::Save(message) => {
            eprintln!("Nonograph: Failed to save post: {message}");
            "save_failed"
        }
    };
    let url = if nojs {
        format!("/nojs/?error={error}")
    } else {
        format!("/?error={error}")
    };
    rocket::response::Redirect::to(url)
}

enum PublishFailure {
    Relays,
    Save(String),
}

fn publish_note(
    storage: &PostStorage,
    config: &Config,
    title: &str,
    author: &str,
    rendered_content: &str,
    raw_content: &str,
) -> Result<String, PublishFailure> {
    let created_at = Utc::now();
    let wrapped = nostr::wrap_note(title, author, raw_content, created_at.timestamp())
        .map_err(|_| PublishFailure::Relays)?;
    let timeout = std::time::Duration::from_secs(config.nostr.timeout_secs.max(1));
    let accepted = nostr::publish_to_relays(&config.nostr.relays, &wrapped.signed_note(), timeout);
    if accepted.is_empty() {
        return Err(PublishFailure::Relays);
    }
    let nevent = nostr::encode_nevent(
        &wrapped.id,
        &accepted,
        &wrapped.pubkey,
        nostr::KIND_GIFT_WRAP,
    );
    let nsec = nostr::encode_nsec(&wrapped.recipient_secret);
    let post = Post {
        id: wrapped.id_hex(),
        title: parser::sanitize_text(title),
        author: parser::sanitize_text(author),
        content: rendered_content.to_string(),
        raw_content: raw_content.to_string(),
        created_at,
    };
    if let Err(error) = save::save_post_to_file_in_dir(&post, ".") {
        return Err(PublishFailure::Save(error.to_string()));
    }
    storage.lock().unwrap().insert(post.id.clone(), post);
    Ok(format!("{nevent}?nsec={nsec}"))
}

fn handle_create(
    nojs: bool,
    form: &NewPost,
    storage: &PostStorage,
    config: &Config,
) -> rocket::response::Redirect {
    let home = if nojs { "/nojs/" } else { "/" };
    if config.security.csrf_protection_enabled && !is_valid_csrf_token(&form.csrf_token) {
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
        parser::render_markdown_with_config(&form.content, &render_options(config));
    match publish_note(
        storage,
        config,
        &form.title,
        &form.alias,
        &rendered_content,
        &form.content,
    ) {
        Ok(nevent) => {
            let prefix = if nojs { "/nojs" } else { "" };
            rocket::response::Redirect::to(format!("{prefix}/{nevent}"))
        }
        Err(failure) => publish_failure_redirect(nojs, failure),
    }
}

#[post("/create", data = "<form>")]
fn create_post(
    _csrf: CsrfProtected,
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<rocket::response::Redirect, content::RawHtml<String>> {
    Ok(handle_create(false, &form, storage, config))
}

fn parse_yaml_frontmatter(file_content: &str) -> Option<(String, String, DateTime<Utc>, String)> {
    let after_open = file_content.strip_prefix("---\n")?;

    let closing_pos = after_open.find("\n---\n")?;
    let frontmatter_block = &after_open[..closing_pos];
    let after_closing = &after_open[(closing_pos + 5)..]; // skip "\n---\n"
    let raw_content = after_closing.strip_prefix('\n').unwrap_or(after_closing);

    let mut title = String::from("Untitled");
    let mut author = String::new();
    let mut date_str = String::new();

    for line in frontmatter_block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("title:") {
            title = parser::sanitize_text(value.trim());
        } else if let Some(value) = line.strip_prefix("date:") {
            date_str = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("author:") {
            author = parser::sanitize_text(value.trim());
        }
    }

    let created_at = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|datetime| DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc))
        .unwrap_or_else(|| Utc::now());

    Some((title, author, created_at, raw_content.to_string()))
}

fn parse_legacy_frontmatter(file_content: &str) -> Option<(String, String, DateTime<Utc>, String)> {
    let lines: Vec<&str> = file_content.splitn(4, '\n').collect();
    if lines.len() < 4 {
        return None;
    }

    let (date_str, author) = if let Some(pipe_pos) = lines[0].find(" | ") {
        (
            lines[0][..pipe_pos].to_string(),
            parser::sanitize_text(&lines[0][(pipe_pos + 3)..]),
        )
    } else {
        (lines[0].to_string(), "".to_string())
    };

    let created_at = chrono::NaiveDate::parse_from_str(&date_str, "%B %d, %Y")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|datetime| DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc))
        .unwrap_or_else(|| Utc::now());

    let title = parser::sanitize_text(lines[2].strip_prefix("# ").unwrap_or("Untitled"));
    let raw_content = lines[3].to_string();

    Some((title, author, created_at, raw_content))
}

fn fetch_missing_note(
    nevent: &nostr::Nevent,
    nsec: Option<&str>,
    storage: &PostStorage,
    config: &Config,
) -> Option<Post> {
    let timeout = std::time::Duration::from_secs(config.nostr.timeout_secs.max(1));
    let secret = nsec.and_then(nostr::decode_nsec);
    let fetched = nostr::fetch_note(&nevent.relays, &nevent.event_id_hex, timeout, secret)?;
    if fetched.content.len() > config.limits.content_max_length {
        return None;
    }
    let created_at = DateTime::from_timestamp(fetched.created_at, 0).unwrap_or_else(Utc::now);
    let post = Post {
        id: fetched.id_hex,
        title: parser::sanitize_text(&fetched.title),
        author: parser::sanitize_text(&fetched.author),
        content: parser::render_markdown_with_config(&fetched.content, &render_options(config)),
        raw_content: fetched.content,
        created_at,
    };
    if let Err(error) = save::save_post_to_file_in_dir(&post, ".") {
        if !matches!(error, save::SaveError::AlreadyExists) {
            eprintln!("Nonograph: Failed to save fetched post: {error}");
        }
    }
    storage
        .lock()
        .unwrap()
        .insert(post.id.clone(), post.clone());
    Some(post)
}

#[get("/<post_id>?<nsec>")]
fn view_post(
    post_id: &str,
    nsec: Option<&str>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
    let decoded = nostr::decode_nevent(post_id);
    let is_raw_request = decoded.is_none() && post_id.ends_with(".md");
    let file_id = match &decoded {
        Some(nevent) => nevent.event_id_hex.as_str(),
        None => post_id.strip_suffix(".md").unwrap_or(post_id),
    };

    // Reject identifiers that could escape the content directory before any
    // filesystem access takes place. See `is_valid_post_id`.
    if !is_valid_post_id(file_id) {
        return Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(NOT_FOUND_HTML.to_string())),
        ));
    }

    if is_raw_request {
        let file_path = format!("content/{}.md", file_id);
        return match std::fs::read_to_string(&file_path) {
            Ok(raw_bytes) => Ok(rocket::Either::Right(content::RawText(raw_bytes))),
            Err(_) => Err((
                Status::NotFound,
                rocket::Either::Left(content::RawText("Page not found".to_string())),
            )),
        };
    }

    // Try to load from memory first with minimal lock time
    let post_from_memory = {
        let mut posts = storage.lock().unwrap();
        // Use the non-cloning get_ref for better performance
        if let Some(post_ref) = posts.get_ref(file_id) {
            Some(post_ref.clone()) // Only clone when we actually found it
        } else {
            None
        }
    };

    let post = match post_from_memory {
        Some(post) => Some(post),
        None => {
            if save::post_file_exists(file_id) {
                if let Ok(file_content) = std::fs::read_to_string(format!("content/{}.md", file_id))
                {
                    let parsed = if file_content.starts_with("---\n") {
                        parse_yaml_frontmatter(&file_content)
                    } else {
                        parse_legacy_frontmatter(&file_content)
                    };

                    if let Some((title, author, created_at, raw_content)) = parsed {
                        let new_post = Post {
                            id: file_id.to_string(),
                            title,
                            author,
                            content: parser::render_markdown_with_config(
                                &raw_content,
                                &render_options(config),
                            ),
                            raw_content,
                            created_at,
                        };

                        {
                            let mut posts_write = storage.lock().unwrap();
                            posts_write.insert(file_id.to_string(), new_post.clone());
                        }

                        Some(new_post)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if let Some(nevent) = &decoded {
                fetch_missing_note(nevent, nsec, storage, config)
            } else {
                None
            }
        }
    };

    match post {
        Some(post) => {
            let engine = TemplateEngine::new("templates");
            let mut context = HashMap::new();

            let rendered_content = post.content.clone();

            context.insert("title".to_string(), post.title.clone());
            context.insert("content".to_string(), rendered_content);
            context.insert("raw_content".to_string(), post.raw_content.clone());
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
            let public_id = if decoded.is_some() { post_id } else { file_id };
            context.insert("post_id".to_string(), public_id.to_string());
            insert_local_parser(&mut context, &post.raw_content, config);

            // OpenGraph variables
            context.insert("url".to_string(), format!("/{}", public_id));

            let description = if post.raw_content.chars().count() > 160 {
                let truncated: String = post.raw_content.chars().take(160).collect();
                format!("{}...", parser::html_attr_escape(&truncated))
            } else {
                post.raw_content.clone()
            };
            context.insert("description".to_string(), description);

            match engine.render("post", &context) {
                Ok(html) => Ok(rocket::Either::Left(content::RawHtml(html))),
                Err(e) => Ok(rocket::Either::Left(content::RawHtml(format!(
                    "Template error: {}",
                    e
                )))),
            }
        }
        None => Err((
            Status::NotFound,
            rocket::Either::Right(content::RawHtml(NOT_FOUND_HTML.to_string())),
        )),
    }
}

#[get("/markup")]
fn markup_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("markup", config)
}

#[get("/legal")]
fn legal_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("legal", config)
}

#[get("/about")]
fn about_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("about", config)
}

#[get("/api")]
fn api_page(
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    serve_static_page("api", config)
}

const PAGE_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/nonograph_page.js"));
const PAGE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nonograph_page_bg.wasm"));
const PARSER_ASSET_VERSION: &str =
    include_str!(concat!(env!("OUT_DIR"), "/parser_asset_version.txt"));

#[get("/page/nonograph_page.js")]
fn parser_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, PAGE_JS)
}

#[get("/page/nonograph_page_bg.wasm")]
fn parser_wasm() -> (ContentType, &'static [u8]) {
    (ContentType::new("application", "wasm"), PAGE_WASM)
}

#[get("/robots.txt")]
fn robots_txt() -> content::RawText<&'static str> {
    content::RawText(
        "User-agent: *\n\
         Disallow: /\n\
         \n\
         # Allow specific paths\n\
         Allow: /api\n\
         Allow: /legal\n\
         Allow: /about\n\
         Allow: /markup\n",
    )
}

#[get("/nojs")]
fn nojs_index(config: &State<Config>) -> content::RawHtml<String> {
    let html = index(config).0;
    let clean_html = nojs::strip_javascript(&html);
    // Update form action to point to /nojs/create
    let nojs_html = clean_html.replace(r#"action="/create""#, r#"action="/nojs/create""#);
    content::RawHtml(nojs_html)
}

#[get("/nojs/<post_id>?<nsec>")]
fn nojs_view_post(
    post_id: &str,
    nsec: Option<&str>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<
    rocket::Either<content::RawHtml<String>, content::RawText<String>>,
    (
        Status,
        rocket::Either<content::RawText<String>, content::RawHtml<String>>,
    ),
> {
    match view_post(post_id, nsec, storage, config) {
        Ok(rocket::Either::Left(content::RawHtml(html))) => {
            let clean_html = nojs::strip_javascript(&html);
            let fixed_html = clean_html
                .replace(
                    &format!(r#"href="/nojs/{}"#, post_id),
                    &format!(r#"href="/{}"#, post_id),
                )
                .replace(r#"target="_blank">nojs</a>"#, r#"target="_blank">js</a>"#);
            Ok(rocket::Either::Left(content::RawHtml(fixed_html)))
        }
        Ok(rocket::Either::Right(raw_text)) => Ok(rocket::Either::Right(raw_text)),
        Err(error) => Err(error),
    }
}

#[post("/nojs/create", data = "<form>")]
fn nojs_create_post(
    _csrf: CsrfProtected,
    form: rocket::form::Form<NewPost>,
    storage: &State<PostStorage>,
    config: &State<Config>,
) -> Result<rocket::response::Redirect, content::RawHtml<String>> {
    Ok(handle_create(true, &form, storage, config))
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

fn serve_static_page(
    page_name: &str,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, (Status, content::RawHtml<String>)> {
    let file_path = format!("content/{}.md", page_name);

    match std::fs::read_to_string(&file_path) {
        Ok(file_content) => {
            let parsed = if file_content.starts_with("---\n") {
                parse_yaml_frontmatter(&file_content)
            } else {
                parse_legacy_frontmatter(&file_content)
            };

            if let Some((title, author, created_at, raw_content)) = parsed {
                let rendered_content =
                    parser::render_markdown_with_config(&raw_content, &render_options(config));

                let engine = TemplateEngine::new("templates");
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
                context.insert("url".to_string(), format!("/{}", page_name));
                context.insert("description".to_string(), String::new());
                context.insert("post_id".to_string(), page_name.to_string());
                insert_local_parser(&mut context, &raw_content, config);

                match engine.render("post", &context) {
                    Ok(html) => Ok(content::RawHtml(html)),
                    Err(e) => Ok(content::RawHtml(format!("Template error: {}", e))),
                }
            } else {
                Ok(content::RawHtml(format!(
                    "<h1>Error</h1><p>Invalid file format for {}</p>",
                    page_name
                )))
            }
        }
        Err(_) => Err((
            Status::NotFound,
            content::RawHtml(NOT_FOUND_HTML.to_string()),
        )),
    }
}

fn start_cache_purge_worker(storage: PostStorage, interval_mins: u64) {
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_secs(interval_mins * 60));
        let mut cache = storage.lock().unwrap();
        cache.purge_deleted();
    });
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "archive" {
        if args.len() < 3 {
            eprintln!("Nonograph: Usage: cargo run archive <telegraph_url>");
            std::process::exit(1);
        }

        let url = &args[2];
        let archiver = archiver::TelegraphArchiver::new();

        match archiver.archive_url(url).await {
            Ok(nonograph_url) => {
                println!("Nonograph: Successfully archived Telegraph page!");
                println!("Nonograph: View at: http://localhost:8009{}", nonograph_url);
            }
            Err(e) => {
                eprintln!("Nonograph: Error archiving page: {}", e);
                std::process::exit(1);
            }
        }

        return Ok(());
    }

    // Default behavior - launch web server
    let _rocket = rocket().launch().await?;
    Ok(())
}

fn rocket() -> rocket::Rocket<rocket::Build> {
    use rocket::data::{Limits, ToByteUnit};

    let config = Config::load_with_logging();

    let limits = Limits::default()
        .limit("form", config.form_data_limit_bytes().bytes())
        .limit("data-form", config.form_data_limit_bytes().bytes())
        .limit("string", config.form_data_limit_bytes().bytes());

    let storage = Arc::new(Mutex::new(PostCache::new(config.cache.max_cache_size_mb)));
    start_cache_purge_worker(Arc::clone(&storage), config.cache.cache_purge_interval_mins);

    let onion_url = config.resolve_onion_url();
    match &onion_url {
        Some(url) => println!("Nonograph: Onion-Location advertising enabled: {}", url),
        None => {
            println!("Nonograph: Onion-Location disabled (no onion URL configured or detected)")
        }
    }

    let mut rocket = rocket::build()
        .configure(rocket::Config {
            limits,
            port: config.server.port,
            address: config
                .server
                .address
                .parse()
                .unwrap_or("127.0.0.1".parse().unwrap()),
            ..rocket::Config::default()
        })
        .attach(SecurityHeadersFairing)
        .manage(storage)
        .manage(config)
        .mount(
            "/",
            routes![
                index,
                create_post,
                view_post,
                markup_page,
                legal_page,
                about_page,
                api_page,
                robots_txt,
                nojs_index,
                nojs_view_post,
                nojs_create_post,
                parser_js,
                parser_wasm
            ],
        );

    if let Some(url) = onion_url {
        rocket = rocket.attach(OnionLocationFairing { onion_url: url });
    }

    rocket
}

#[cfg(test)]
#[path = "../test/main.rs"]
mod tests;
