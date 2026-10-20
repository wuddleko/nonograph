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
const SEALED_ABOUT: &str = r##"<div class="bottom-sheet-overlay" id="help-sheet">
<div class="bottom-sheet">
<div class="bottom-sheet-header">
<h2 class="bottom-sheet-title">About Nostorgraph</h2>
<a href="#" class="bottom-sheet-close"> &times; </a>
</div>
<div class="bottom-sheet-content">
<div class="help-section">
<h3>What's this?</h3>
<p>Nostorgraph is for writing things you want to exist somewhere permanent without that somewhere being attached to you, or to a single central entity.<br>A thought, an essay, a rant, class notes, up to you. You write it, you get a link. Share the link with whoever you'd like, or keep it to yourself.</p>
<p>Your post is encrypted in your browser the moment you click STORE.<br>Only you, and anyone with the link, can read it.<br>This host does not keep a copy.<br>The writing is ordinary markup, not a proprietary format.<br>You can also save a copy on this device as markdown or a PDF.</p>
<p>Notes are stored on <a href="https://nostr.com/info" target="_blank" rel="noopener noreferrer">nostr relays</a>. They are reached through Tor, so nobody sees who posted a note or who read it.</p>
</div>
<div class="help-section">
<h3>Links</h3>
<p>Store offers three addresses. A reader only has the one they were sent. The key is the part after #. This site never sees it.</p>
<p><strong>Long.</strong> Most private. This site stores nothing. The event and the relays are in the address. The key is the part after #.<br><code>https://&lt;site&gt;/&lt;locator&gt;/&lt;event-id&gt;?r=&lt;relay&gt;#key</code></p>
<p><strong>Short.</strong> This site stores the event id and the relays. It does not store the writing or the key.<br><code>https://&lt;site&gt;/&lt;locator&gt;#key</code></p>
<p><strong>Title.</strong> Least private. This site stores the title, the event id, and the relays. The title is in the address. It does not store the writing or the key.<br><code>https://&lt;site&gt;/&lt;title&gt;#key</code></p>
</div>
<div class="help-section">
<h3>Principles</h3>
<ul>
<li>Posts cannot be edited or deleted after publishing</li>
<li>No accounts, no tracking, no analytics</li>
<li>Open to all, no political or social biases</li>
<li>Open source, self-hostable</li>
</ul>
</div>
<div class="help-section">
<h3>Source code</h3>
<p>It's entirely open source. The code is at <a href="https://github.com/wuddleko/nostorgraph" target="_blank" rel="noopener noreferrer">github.com/wuddleko/nostorgraph</a>. You can read it, run it yourself, and host for others easily.</p>
<p>PDF text uses the <a href="/pdf/LICENSE">DejaVu fonts</a>.</p>
</div>
<div class="help-section">
<h3>Formatting</h3>
<p class="format-with-slash">Type <code>/</code> for a menu of formatting options, or use markdown directly.</p>
<p class="format-markdown">Use markdown directly.</p>
</div>
</div>
</div>
</div>"##;
const SEALED_SAVE: &str = "<span class=\"save-note\" hidden><button type=\"button\" class=\"save-open\" aria-expanded=\"false\" aria-controls=\"save-formats\">save</button><span class=\"save-formats\" id=\"save-formats\" role=\"group\" aria-label=\"Save format\" hidden><span class=\"view-mode-bar\" aria-hidden=\"true\">|</span><button type=\"button\" data-save=\"markdown\">markdown</button><span class=\"view-mode-bar\" aria-hidden=\"true\">|</span><button type=\"button\" data-save=\"pdf\">pdf</button><span class=\"save-status\" role=\"status\" hidden></span></span></span>";

fn sealed_key_box() -> &'static str {
    "<div id=\"sealed-open\">\
<p id=\"sealed-key-error\"></p>\
<input id=\"sealed-key\" type=\"text\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"The key is the part of the address after #.\" />\
<button type=\"button\">Open</button>\
</div>"
}

pub(crate) fn sealed_shell(events: &[String], config: &Config) -> String {
    sealed_shell_for(events, config, None)
}

fn sealed_shell_for(events: &[String], config: &Config, locator: Option<&str>) -> String {
    let version = super::assets::version();
    let theme_boot = if events.is_empty() {
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
    let event = sealed_event_markup(events, version);
    let locator_mark = match locator {
        Some(locator) if crate::nostr::sealed_locator(locator) => {
            format!(r#"<div id="sealed-locator" hidden>{locator}</div>"#)
        }
        _ => String::new(),
    };
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    {theme_boot}
    <title>{title}</title>
    <link rel="icon" type="image/png" href="/logo.png?v={version}" />
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
        {about}
    <div class="footer">
        <button type="button" id="sealed-copy">Copy link</button>
        <a href="/">write your own</a>
        <a href="#help-sheet">about</a>
        <a href="/legal">legal</a>
        <a href="/api">api</a>
        <a href="{source}">source code</a>
        <button type="button" class="theme-toggle" aria-label="Dark mode"><span class="theme-to-dark">dark</span><span class="theme-to-light">light</span></button>
        {save}
    </div>
    {locator_mark}
    {event}
</body>
</html>"##,
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
        save = if events.is_empty() { "" } else { SEALED_SAVE },
        about = SEALED_ABOUT,
        locator_mark = locator_mark,
    )
}

#[derive(FromForm)]
pub(crate) struct SealHints {
    #[field(name = "r", default = Vec::<String>::new())]
    r: Vec<String>,
}

#[get("/<segment>?<hints..>")]
pub async fn root_link_view(
    segment: &str,
    hints: SealHints,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    if crate::nostr::sealed_locator(segment) {
        return sealed_locator_view(segment, hints, config).await;
    }
    title_view(segment, config).await
}

#[get("/<locator>/<event_id>?<hints..>")]
pub async fn root_sealed_view(
    locator: &str,
    event_id: &str,
    hints: SealHints,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    sealed_view(locator, event_id, hints, config).await
}

#[get("/s/<locator>?<hints..>")]
pub async fn sealed_locator_view(
    locator: &str,
    hints: SealHints,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    if !crate::nostr::sealed_locator(locator) {
        return Err(sealed_not_found());
    }
    open_sealed(locator.to_string(), None, hints.r, config, false).await
}

#[get("/s/<locator>/<event_id>?<hints..>")]
pub async fn sealed_view(
    locator: &str,
    event_id: &str,
    hints: SealHints,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    if !crate::nostr::sealed_locator(locator) || !crate::nostr::sealed_event_id(event_id) {
        return Err(sealed_not_found());
    }
    open_sealed(
        locator.to_string(),
        Some(event_id.to_string()),
        hints.r,
        config,
        false,
    )
    .await
}

#[get("/t/<slug>")]
pub async fn title_view(
    slug: &str,
    config: &State<Config>,
) -> Result<content::RawHtml<String>, ViewErr> {
    let Some(locator) = super::title_link::locator_for(slug) else {
        return Err(sealed_not_found());
    };
    open_sealed(locator, None, Vec::new(), config, true).await
}

fn sealed_not_found() -> ViewErr {
    (
        Status::NotFound,
        rocket::Either::Right(content::RawHtml(not_found_html())),
    )
}

async fn open_sealed(
    locator: String,
    event_id: Option<String>,
    hints: Vec<String>,
    config: &State<Config>,
    show_locator: bool,
) -> Result<content::RawHtml<String>, ViewErr> {
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err((
            Status::ServiceUnavailable,
            rocket::Either::Left(content::RawText("Service unavailable".to_string())),
        ));
    };
    let config = config.inner().clone();
    let shell = config.clone();
    let embed = if show_locator {
        Some(locator.clone())
    } else {
        None
    };
    let events = match rocket::tokio::task::spawn_blocking(move || {
        read_sealed_events(&config, &hints, &locator, event_id.as_deref())
    })
    .await
    {
        Ok(events) => events,
        Err(_) => Vec::new(),
    };
    if events.is_empty() {
        return Err(sealed_not_found());
    }
    Ok(content::RawHtml(sealed_shell_for(
        &events,
        &shell,
        embed.as_deref(),
    )))
}

fn read_sealed_events(
    config: &Config,
    hints: &[String],
    locator: &str,
    event_id: Option<&str>,
) -> Vec<String> {
    #[cfg(test)]
    if let Some(canned) = canned_nojs_event() {
        note_nojs_fetch_hints(hints);
        return canned.into_iter().collect();
    }
    let timeout = relay_timeout(config);
    let pool = crate::crawl::pool_path(Path::new("content"));
    if let Some(event_id) = event_id {
        #[cfg(test)]
        note_nojs_fetch_hints(hints);
        return fetch_sealed_by_id(
            hints,
            event_id,
            locator,
            config.limits.title_max_length,
            config.limits.alias_max_length,
            config.limits.content_max_length,
            timeout,
            &pool,
        )
        .into_iter()
        .collect();
    }
    // A short link opens the saved event. Query relays do not choose another one.
    let Some(saved) = super::seal_row::read(locator) else {
        return Vec::new();
    };
    #[cfg(test)]
    note_nojs_fetch_hints(&saved.relays);
    fetch_sealed_by_id(
        &saved.relays,
        &saved.event_id,
        locator,
        config.limits.title_max_length,
        config.limits.alias_max_length,
        config.limits.content_max_length,
        timeout,
        &pool,
    )
    .into_iter()
    .collect()
}

fn sealed_event_markup(events: &[String], version: &str) -> String {
    if events.is_empty() {
        return String::new();
    }
    let mut list = String::from("[");
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            list.push(',');
        }
        list.push_str(event);
    }
    list.push(']');
    format!(
        "<script type=\"module\" src=\"/nostr.js?v={version}\"></script>\n    <div id=\"sealed-event\" hidden>{}</div>",
        list.replace('<', "\\u003c"),
    )
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
    <link rel="icon" type="image/png" href="/logo.png?v={version}" />
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

#[get("/nojs/s/<locator>")]
pub fn nojs_sealed_locator_view(
    locator: &str,
) -> NojsSealedHeaders<Result<content::RawHtml<String>, ViewErr>> {
    if !crate::nostr::sealed_locator(locator) {
        return NojsSealedHeaders(Err(nojs_not_found()));
    }
    NojsSealedHeaders(Ok(content::RawHtml(run_locally_page())))
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
    let relays = public_relays(hints);
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
    // Relays past the six were not queried. Leave them eligible for the pool.
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
