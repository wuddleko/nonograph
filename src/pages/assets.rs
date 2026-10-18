use std::sync::OnceLock;

use rocket::http::ContentType;
use rocket::response::content;
use sha2::{Digest, Sha256};

use crate::template;

pub const PAGE_JS_PATH: &str = "/page/nostorgraph_page.js";
pub const PAGE_WASM_PATH: &str = "/page/nostorgraph_page_bg.wasm";
pub const PDF_JS_PATH: &str = "/pdf/nostorgraph_pdf.js";
pub const PDF_WASM_PATH: &str = "/pdf/nostorgraph_pdf_bg.wasm";
pub const PDF_LICENSE_PATH: &str = "/pdf/LICENSE";
pub const HOME_CSS_PATH: &str = "/home.css";
pub const HOME_JS_PATH: &str = "/home.js";
pub const NOSTR_JS_PATH: &str = "/nostr.js";
pub const SECP256K1_JS_PATH: &str = "/secp256k1.js";
pub const POST_CSS_PATH: &str = "/post.css";
pub const POST_JS_PATH: &str = "/post.js";
pub const POST_NOSCRIPT_CSS_PATH: &str = "/post-noscript.css";
pub const HOME_NOSCRIPT_CSS_PATH: &str = "/home-noscript.css";
pub const WRITEMARK_JS_PATH: &str = "/writemark.js";
pub const WRITEMARK_LIVE_JS_PATH: &str = "/writemark-live.js";
pub const WRITEMARK_CSS_PATH: &str = "/writemark.css";
pub const NOTICE_CSS_PATH: &str = "/notice.css";
pub const THEME_JS_PATH: &str = "/theme.js";
pub const EDITOR_MODE_JS_PATH: &str = "/editor-mode.js";
pub const RELAYS_JS_PATH: &str = "/relays.js";

pub(crate) const PAGE_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/nostorgraph_page.js"));
const PAGE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nostorgraph_page_bg.wasm"));
pub(crate) const PDF_JS: &str = include_str!(concat!(env!("OUT_DIR"), "/nostorgraph_pdf.js"));
const PDF_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nostorgraph_pdf_bg.wasm"));
const PDF_LICENSE: &str = include_str!("../../pdf/fonts/LICENSE");

const LONG_CACHE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "public, no-cache";

pub fn cache_control_for_request(path: &str, query: Option<&str>) -> &'static str {
    let versioned = query.is_some_and(|value| {
        value
            .split('&')
            .any(|part| part.starts_with("v=") && part.len() > 2)
    });
    // The home fingerprint covers these two files. PDF URLs inherit the same
    // ?v= from home.js and are not part of that hash, so they keep revalidating.
    if versioned && matches!(path, PAGE_JS_PATH | PAGE_WASM_PATH) {
        return LONG_CACHE;
    }
    cache_control_for_path(path)
}

pub fn cache_control_for_path(path: &str) -> &'static str {
    if path.starts_with("/nojs/s/") {
        return "no-store";
    }
    match path {
        PAGE_JS_PATH | PAGE_WASM_PATH | PDF_JS_PATH | PDF_WASM_PATH | PDF_LICENSE_PATH => {
            REVALIDATE
        }
        HOME_CSS_PATH
        | HOME_JS_PATH
        | NOSTR_JS_PATH
        | SECP256K1_JS_PATH
        | POST_CSS_PATH
        | POST_JS_PATH
        | POST_NOSCRIPT_CSS_PATH
        | HOME_NOSCRIPT_CSS_PATH
        | WRITEMARK_JS_PATH
        | WRITEMARK_LIVE_JS_PATH
        | WRITEMARK_CSS_PATH
        | NOTICE_CSS_PATH
        | THEME_JS_PATH
        | EDITOR_MODE_JS_PATH
        | RELAYS_JS_PATH => LONG_CACHE,
        _ => "no-store, max-age=0",
    }
}

struct PageAssetTags {
    js: String,
    wasm: String,
    pdf_js: String,
    pdf_wasm: String,
    pdf_license: String,
}

fn page_asset_tags() -> &'static PageAssetTags {
    static TAGS: OnceLock<PageAssetTags> = OnceLock::new();
    TAGS.get_or_init(|| PageAssetTags {
        js: entity_tag(PAGE_JS.as_bytes()),
        wasm: entity_tag(PAGE_WASM),
        pdf_js: entity_tag(PDF_JS.as_bytes()),
        pdf_wasm: entity_tag(PDF_WASM),
        pdf_license: entity_tag(PDF_LICENSE.as_bytes()),
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
        PDF_JS_PATH => Some(tags.pdf_js.as_str()),
        PDF_WASM_PATH => Some(tags.pdf_wasm.as_str()),
        PDF_LICENSE_PATH => Some(tags.pdf_license.as_str()),
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
    home_noscript_css: String,
    writemark_js: String,
    writemark_live_js: String,
    writemark_css: String,
    notice_css: String,
    theme_js: String,
    editor_mode_js: String,
    relays_js: String,
}

pub fn warm() {
    nostorgraph_parser::warm();
    let _ = assets();
    let _ = page_asset_tags();
    if let Err(error) = template::shared().preload(&["home", "post"]) {
        eprintln!("Nostorgraph: {error}");
    }
}

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let read = |name: &str| -> String {
            std::fs::read_to_string(format!("templates/{name}")).unwrap_or_else(|error| {
                panic!("Nostorgraph: failed to read templates/{name}: {error}")
            })
        };
        let home_css = read("home.css");
        let home_js = read("home.js");
        let nostr_js = read("nostr.js");
        let secp256k1_js = read("secp256k1.js");
        let post_css = read("post.css");
        let post_js = read("post.js");
        let post_noscript_css = read("post-noscript.css");
        let home_noscript_css = read("home-noscript.css");
        let writemark_live_js = read("writemark-live.js");
        let writemark_source = read("writemark.js");
        let writemark_css = read("writemark.css");
        let notice_css = read("notice.css");
        let theme_js = read("theme.js");
        let editor_mode_js = read("editor-mode.js");
        let relays_js = read("relays.js");
        let version = asset_version(&[
            &home_css,
            &home_js,
            &nostr_js,
            &secp256k1_js,
            &post_css,
            &post_js,
            &post_noscript_css,
            &home_noscript_css,
            &writemark_source,
            &writemark_live_js,
            &writemark_css,
            &notice_css,
            &theme_js,
            &editor_mode_js,
            &relays_js,
        ]);
        let writemark_js = writemark_source.replacen(
            "from \"./writemark-live.js\"",
            &format!("from \"./writemark-live.js?v={version}\""),
            1,
        );
        if writemark_js == writemark_source {
            panic!("Nostorgraph: writemark.js does not import writemark-live.js");
        }
        Assets {
            version,
            home_css,
            home_js,
            nostr_js,
            secp256k1_js,
            post_css,
            post_js,
            post_noscript_css,
            home_noscript_css,
            writemark_js,
            writemark_live_js,
            writemark_css,
            notice_css,
            theme_js,
            editor_mode_js,
            relays_js,
        }
    })
}

fn asset_version(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    hasher.update(PAGE_WASM);
    hasher.update(PAGE_JS.as_bytes());
    short_hex(&hasher.finalize())
}

pub(super) fn version() -> &'static str {
    assets().version.as_str()
}

pub(super) fn home_scripts(version: &str) -> String {
    format!(
        "<link rel=\"modulepreload\" href=\"/writemark.js?v={version}\" />\n<link rel=\"modulepreload\" href=\"/writemark-live.js?v={version}\" />\n<link rel=\"preload\" href=\"/page/nostorgraph_page_bg.wasm?v={version}\" as=\"fetch\" crossorigin />\n<script src=\"/relays.js?v={version}\"></script>\n<script src=\"/editor-mode.js?v={version}\"></script>\n<script type=\"module\" src=\"/writemark.js?v={version}\"></script>\n<script type=\"module\" src=\"/home.js?v={version}\"></script>"
    )
}

pub(crate) fn post_scripts(version: &str) -> String {
    format!("<script src=\"/post.js?v={version}\"></script>")
}

#[get("/page/nostorgraph_page.js")]
pub fn parser_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, PAGE_JS)
}

#[get("/page/nostorgraph_page_bg.wasm")]
pub fn parser_wasm() -> (ContentType, &'static [u8]) {
    (ContentType::new("application", "wasm"), PAGE_WASM)
}

#[get("/pdf/nostorgraph_pdf.js")]
pub fn pdf_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, PDF_JS)
}

#[get("/pdf/nostorgraph_pdf_bg.wasm")]
pub fn pdf_wasm() -> (ContentType, &'static [u8]) {
    (ContentType::new("application", "wasm"), PDF_WASM)
}

#[get("/pdf/LICENSE")]
pub fn pdf_license() -> (ContentType, &'static str) {
    (ContentType::Plain, PDF_LICENSE)
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

#[get("/home-noscript.css")]
pub fn home_noscript_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().home_noscript_css.as_str())
}

#[get("/writemark.js")]
pub fn writemark_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().writemark_js.as_str())
}

#[get("/writemark-live.js")]
pub fn writemark_live_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().writemark_live_js.as_str())
}

#[get("/writemark.css")]
pub fn writemark_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().writemark_css.as_str())
}

#[get("/notice.css")]
pub fn notice_css() -> (ContentType, &'static str) {
    (ContentType::CSS, assets().notice_css.as_str())
}

#[get("/theme.js")]
pub fn theme_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().theme_js.as_str())
}

#[get("/editor-mode.js")]
pub fn editor_mode_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().editor_mode_js.as_str())
}

#[get("/relays.js")]
pub fn relays_js() -> (ContentType, &'static str) {
    (ContentType::JavaScript, assets().relays_js.as_str())
}

const ROBOTS_TXT: &str = include_str!("../../robots.txt");

#[get("/robots.txt")]
pub fn robots_txt() -> content::RawText<&'static str> {
    content::RawText(ROBOTS_TXT)
}
