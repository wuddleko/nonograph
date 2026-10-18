use std::path::Path;
use std::time::Duration;

use rocket::http::{Header, Status};
use rocket::response::content;
use rocket::response::Responder;
use rocket::State;

use crate::config::Config;
use crate::relay_set::relay_timeout;
use crate::render_options;

use super::fetch_slot::PublicFetchSlot;
use super::{not_found_html, public_relays};

#[cfg(test)]
use super::nojs_test::{canned_nojs_event, note_nojs_fetch_hints};

type ViewErr = (
    Status,
    rocket::Either<content::RawText<String>, content::RawHtml<String>>,
);

const SEALED_TITLE: &str = "Sealed note";
const SEALED_PREVIEW: &str = "A note sealed in the browser.";
const SOURCE: &str = "https://github.com/wuddleko/nostorgraph";

fn sealed_key_box() -> &'static str {
    "<div id=\"sealed-open\">\
<p id=\"sealed-key-error\"></p>\
<input id=\"sealed-key\" type=\"text\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"The key is the part of the address after #.\" />\
<button type=\"button\">Open</button>\
</div>"
}

pub(crate) fn sealed_shell(event_json: &str, config: &Config) -> String {
    let version = super::assets::version();
    let theme_boot = if event_json.is_empty() {
        String::new()
    } else {
        super::layout::theme_boot(version)
    };
    let options = render_options(config);
    let theme = nostorgraph_parser::html_attr_escape(&options.syntax_theme);
    let external = if options.external_link_security {
        "true"
    } else {
        "false"
    };
    let event = if event_json.is_empty() {
        String::new()
    } else {
        format!(
            "<script type=\"module\" src=\"/nostr.js?v={version}\"></script>\n    <div id=\"sealed-event\" hidden>{}</div>",
            event_json.replace('<', "\\u003c"),
        )
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    {theme_boot}
    <title>{title}</title>
    <link rel="icon" href="data:," />
    <meta property="og:title" content="{title}" />
    <meta property="og:type" content="website" />
    <meta property="og:description" content="{preview}" />
    <meta name="description" content="{preview}" />
    <link rel="stylesheet" href="/post.css?v={version}" />
    <noscript>
        <link rel="stylesheet" href="/post-noscript.css?v={version}" />
    </noscript>
</head>
<body>
    <div class="container">
        <div class="content">
            <h1 id="sealed-title">{title}</h1>
            <svg class="wave" viewBox="0 0 640 16" preserveAspectRatio="none" aria-hidden="true"><path d="M0 8 H640"></path></svg>
            <div class="article-meta" id="sealed-author" hidden></div>
            <p class="run-locally">Run locally to read the note. <a href="{source}">source code</a></p>
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
        <a href="{source}">source code</a>
        <button type="button" class="theme-toggle" aria-label="Dark mode"><span class="theme-to-dark">dark</span><span class="theme-to-light">light</span></button>
    </div>
    {event}
</body>
</html>"#,
        title = SEALED_TITLE,
        preview = SEALED_PREVIEW,
        source = SOURCE,
        form = sealed_key_box(),
        event = event,
        version = version,
        theme_boot = theme_boot,
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
    let query = locator.to_string();
    let query_id = event_id.to_string();
    let hints = hints.r;
    let event = match rocket::tokio::task::spawn_blocking(move || {
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
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
            timeout,
            &crate::crawl::pool_path(Path::new("content")),
        )
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
    Ok(content::RawHtml(sealed_shell(&event, &shell)))
}

fn nojs_not_found() -> ViewErr {
    (
        Status::NotFound,
        rocket::Either::Right(content::RawHtml(not_found_html())),
    )
}

fn run_locally_page() -> String {
    let version = super::assets::version();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta name="referrer" content="no-referrer" />
    <meta http-equiv="refresh" content="0; url={source}" />
    <title>Run locally to read the note</title>
    <link rel="icon" href="data:," />
    <link rel="stylesheet" href="/notice.css?v={version}" />
</head>
<body>
    <h1>Run locally to read the note</h1>
    <p><a href="{source}">source code</a></p>
</body>
</html>"#,
        source = SOURCE,
        version = version,
    )
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

#[get("/nojs/s/<locator>/<event_id>")]
pub fn nojs_sealed_view(
    locator: &str,
    event_id: &str,
) -> NojsSealedHeaders<Result<content::RawHtml<String>, ViewErr>> {
    if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
        return NojsSealedHeaders(Err(nojs_not_found()));
    }
    NojsSealedHeaders(Ok(content::RawHtml(run_locally_page())))
}

const MAX_CALLER_HINTS: usize = 2;

pub(crate) fn fetch_sealed_by_id(
    hints: &[String],
    event_id: &str,
    locator: &str,
    title_max: usize,
    alias_max: usize,
    content_max: usize,
    timeout: Duration,
    pool: &Path,
) -> Option<String> {
    let mut relays = public_relays(hints);
    relays.truncate(MAX_CALLER_HINTS);
    if let Some(note) = crate::nostr::fetch_sealed(
        &relays,
        event_id,
        locator,
        title_max,
        alias_max,
        content_max,
        timeout,
    ) {
        return Some(note);
    }
    // Later hints were not queried. Leave them eligible for the pool.
    let rows = crate::crawl::next_pool_relays(pool, &relays);
    crate::nostr::fetch_sealed(
        &rows,
        event_id,
        locator,
        title_max,
        alias_max,
        content_max,
        timeout,
    )
}
