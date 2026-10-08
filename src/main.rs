#[macro_use]
extern crate rocket;

mod archiver;
pub(crate) mod cache;
pub(crate) mod config;
pub(crate) mod csrf;
pub(crate) mod nostr;
mod pages;
pub(crate) mod publish;
pub(crate) mod relays;
pub(crate) mod save;
pub(crate) mod template;
pub(crate) mod tor_circuits;

pub(crate) use cache::{Post, PostCache, PostStorage};

use config::Config;
use nonograph_parser as parser;
use std::thread;

use chrono::{DateTime, Utc};
use deunicode::deunicode;
use rand::{thread_rng, Rng};
use rocket::{
    fairing::{Fairing, Info, Kind},
    http::{Header, Status},
    Request, Response,
};
use std::sync::Arc;

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
            content_security_policy(),
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
        let path = request.uri().path().as_str();
        response.set_header(Header::new(
            "Cache-Control",
            pages::cache_control_for_path(path),
        ));
        if let Some(etag) = pages::entity_tag_for_path(path) {
            response.set_header(Header::new("ETag", etag));
            if request
                .headers()
                .get_one("If-None-Match")
                .is_some_and(|presented| pages::if_none_match_matches(presented, etag))
            {
                response.set_status(Status::NotModified);
                response.body_mut().take();
            }
        }
    }
}

fn content_security_policy() -> String {
    // Relays are reached by this process over Tor, not by the tab.
    "default-src 'self'; \
     connect-src 'self'; \
     base-uri 'self'; \
     form-action 'self'; \
     frame-ancestors 'self'; \
     img-src 'self' https: http: data:; \
     media-src 'self' https: http:; \
     object-src 'none'; \
     script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; \
     script-src-attr 'none'; \
     style-src 'self' https: 'unsafe-inline'"
        .to_string()
}

/// Maximum length of a post identifier, matching typical filesystem limits on
/// a single path component.
const MAX_POST_ID_LEN: usize = 255;
const UNGUESSABLE_BYTES: usize = 4;
pub(crate) const UNGUESSABLE_HEX_LEN: usize = UNGUESSABLE_BYTES * 2;

/// Returns `true` if `id` is a well-formed post identifier.
///
/// A valid identifier is a non-empty, length-bounded slug composed only of
/// ASCII letters, digits, hyphens, and underscores. Every identifier the
/// application produces satisfies this: [`generate_post_id`] emits
/// `[a-z0-9-]`, the static pages are lowercase words, and the Telegraph
/// archiver yields `[A-Za-z0-9_-]` slugs.
///
/// This is the trust boundary for untrusted path input. Because `.`, `/`, and
/// `\` are all rejected, a value that passes this check cannot express a
/// path-traversal sequence such as `../`, so it can be safely interpolated
/// into a `content/{id}.md` path.
pub(crate) fn is_valid_post_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_POST_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) fn is_nostr_identifier(id: &str) -> bool {
    let rest = id
        .strip_prefix("nevent1")
        .or_else(|| id.strip_prefix("naddr1"));
    let Some(rest) = rest else {
        return false;
    };
    !rest.is_empty() && id.len() <= 8192 && rest.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// 4 random bytes, hex-encoded. Enough that a title is not a usable guess,
/// short enough that the public link stays compact: `{slug}-{8 hex}`.
pub(crate) fn generate_unguessable_segment() -> String {
    let mut rng = thread_rng();
    (0..UNGUESSABLE_BYTES)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

fn assemble_post_id(slug: &str, random: &str, index: usize) -> String {
    if index == 0 {
        format!("{slug}-{random}")
    } else {
        format!("{slug}-{random}-{index}")
    }
}

fn id_is_taken(storage: &PostStorage, post_id: &str, base_dir: &str) -> bool {
    storage.read().unwrap().contains_key(post_id)
        || save::post_file_exists_in_dir(post_id, base_dir)
}

pub(crate) fn generate_post_id(title: &str, storage: &PostStorage) -> Result<String, String> {
    generate_post_id_in_dir(title, storage, ".", &generate_unguessable_segment())
}

pub(crate) fn generate_post_id_in_dir(
    title: &str,
    storage: &PostStorage,
    base_dir: &str,
    random: &str,
) -> Result<String, String> {
    generate_post_id_with_segment_in_dir(title, storage, random, base_dir)
}

#[cfg(test)]
fn generate_post_id_with_segment(
    title: &str,
    storage: &PostStorage,
    random: &str,
) -> Result<String, String> {
    generate_post_id_with_segment_in_dir(title, storage, random, ".")
}

fn generate_post_id_with_segment_in_dir(
    title: &str,
    storage: &PostStorage,
    random: &str,
    base_dir: &str,
) -> Result<String, String> {
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

    // Leave room for "-{random}" and a collision suffix up to "-999".
    let max_slug_length = 250 - 1 - UNGUESSABLE_HEX_LEN - 4;
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

        for i in 0..1000 {
            let post_id = assemble_post_id(&fallback_slug, random, i);

            if !id_is_taken(storage, &post_id, base_dir) {
                return Ok(post_id);
            }
        }

        return Err(
            "All slots for this title and date are taken. Please choose another title.".to_string(),
        );
    }

    // Try to find an available slot (0-999)
    for i in 0..1000 {
        let post_id = assemble_post_id(&final_slug, random, i);

        if !id_is_taken(storage, &post_id, base_dir) {
            return Ok(post_id);
        }
    }

    Err("All slots for this title and date are taken. Please choose another title.".to_string())
}

fn render_options(config: &Config) -> parser::RenderOptions {
    parser::RenderOptions {
        max_url_length: config.security.max_url_length,
        external_link_security: config.security.external_link_security,
        syntax_theme: config.theme.syntax_highlighting.clone(),
    }
}

pub(crate) fn parse_frontmatter(
    file_content: &str,
) -> Option<(String, String, DateTime<Utc>, String)> {
    if file_content.starts_with("---\n") {
        parse_yaml_frontmatter(file_content)
    } else {
        parse_legacy_frontmatter(file_content)
    }
}

fn yaml_frontmatter_block(file_content: &str) -> Option<(&str, &str)> {
    let after_open = file_content.strip_prefix("---\n")?;
    let closing_pos = after_open.find("\n---\n")?;
    let block = &after_open[..closing_pos];
    let after_closing = &after_open[closing_pos + 5..];
    Some((
        block,
        after_closing.strip_prefix('\n').unwrap_or(after_closing),
    ))
}

fn parse_yaml_frontmatter(file_content: &str) -> Option<(String, String, DateTime<Utc>, String)> {
    let (frontmatter_block, raw_content) = yaml_frontmatter_block(file_content)?;

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

pub(crate) fn yaml_frontmatter_field(file_content: &str, key: &str) -> Option<String> {
    let (frontmatter_block, _) = yaml_frontmatter_block(file_content)?;
    for line in frontmatter_block.lines() {
        if let Some(value) = line.trim().strip_prefix(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
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

fn start_cache_purge_worker(storage: PostStorage, interval_mins: u64) {
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_secs(interval_mins * 60));
        cache::purge_missing(&storage);
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

fn install_bundled_pages() {
    match install_bundled_pages_from(
        std::path::Path::new("pages"),
        std::path::Path::new("content"),
    ) {
        Ok(mut installed) => {
            installed.sort();
            for name in installed {
                println!("Nonograph: Installed bundled page {name}");
            }
        }
        Err(error) => eprintln!("Nonograph: {error}"),
    }
}

fn install_bundled_pages_from(
    pages_dir: &std::path::Path,
    content_dir: &std::path::Path,
) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(pages_dir).map_err(|error| {
        format!(
            "Bundled pages directory '{}' is not available: {error}",
            pages_dir.display()
        )
    })?;
    std::fs::create_dir_all(content_dir).map_err(|error| {
        format!(
            "Failed to create content directory '{}': {error}",
            content_dir.display()
        )
    })?;

    let mut installed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Failed to read bundled pages: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let dest = content_dir.join(name);
        if dest.exists() {
            continue;
        }
        std::fs::copy(&path, &dest).map_err(|error| {
            format!("Failed to install bundled page {}: {error}", path.display())
        })?;
        installed.push(name.to_string_lossy().into_owned());
    }
    Ok(installed)
}

fn rocket() -> rocket::Rocket<rocket::Build> {
    use rocket::data::{Limits, ToByteUnit};

    install_bundled_pages();
    pages::warm();

    let config = Config::load_with_logging();

    let limits = Limits::default()
        .limit("form", config.form_data_limit_bytes().bytes())
        .limit("data-form", config.form_data_limit_bytes().bytes())
        .limit("string", config.form_data_limit_bytes().bytes());

    let storage = PostCache::shared(config.cache.max_cache_size_mb);
    start_cache_purge_worker(Arc::clone(&storage), config.cache.cache_purge_interval_mins);

    match config.socks_addr() {
        Ok(Some(addr)) => {
            crate::nostr::set_relay_socks(crate::nostr::RelaySocks::Proxy(addr));
            println!("Nonograph: Nostr relays via SOCKS {addr}");
        }
        Ok(None) => {
            crate::nostr::set_relay_socks(crate::nostr::RelaySocks::Direct);
            println!("Nonograph: Nostr relays connect directly (no SOCKS)");
        }
        Err(()) => {
            crate::nostr::set_relay_socks(crate::nostr::RelaySocks::Disabled);
            eprintln!(
                "Nonograph: [nostr].socks is not a socket address; relay connections are disabled"
            );
        }
    }

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
                pages::index,
                pages::create_post,
                pages::nostr_publish,
                pages::tor_circuits,
                pages::assign_tor_circuits,
                pages::sealed_view,
                pages::view_post,
                pages::markup_page,
                pages::legal_page,
                pages::about_page,
                pages::api_page,
                pages::robots_txt,
                pages::nojs_index,
                pages::nojs_sealed_view,
                pages::nojs_view_post,
                pages::nojs_create_post,
                pages::parser_js,
                pages::parser_wasm,
                pages::home_css,
                pages::home_js,
                pages::nostr_js,
                pages::secp256k1_js,
                pages::post_css,
                pages::post_js,
                pages::post_noscript_css,
                pages::writemark_js,
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
