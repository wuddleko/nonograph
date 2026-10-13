use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rocket::http::{Header, Status};
use rocket::response::content;
use rocket::response::Responder;
use rocket::State;

use crate::config::Config;
use crate::relay_set::relay_timeout;
use crate::render_options;
use crate::template;

use super::fetch_slot::PublicFetchSlot;
use super::layout::js_seal_href;
use super::{not_found_html, post_description, relays_for_public_fetch};

#[cfg(test)]
use super::nojs_test::{canned_nojs_event, note_nojs_fetch_hints};

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
    let version = super::assets::version();
    let theme_boot = if event_json.is_empty() {
        String::new()
    } else {
        super::layout::theme_boot(version)
    };
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
</head>
<body>
    <div class="container">
        <div class="content">
            <h1 id="sealed-title">{title}</h1>
            <svg class="wave" viewBox="0 0 640 16" preserveAspectRatio="none" aria-hidden="true"><path d="M0 8 H640"></path></svg>
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
        <button type="button" class="theme-toggle" aria-label="Dark mode"><span class="theme-to-dark">dark</span><span class="theme-to-light">light</span></button>
    </div>
    {event}
</body>
</html>"#,
        title = SEALED_TITLE,
        preview = SEALED_PREVIEW,
        form = sealed_open_form(locator, event_id, hints, key_error),
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
                config.limits.title_max_length,
                config.limits.alias_max_length,
                config.limits.content_max_length,
                timeout,
                &crate::crawl::pool_path(Path::new("content")),
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
    super::layout::centered_notice(
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
    let page = share
        .split_once('#')
        .map(|(page, _)| page)
        .unwrap_or(&share);
    let origin = config.normalized_public_origin().unwrap_or_default();
    let mut context = super::layout::article_context(
        &note.title,
        &note.author,
        event_created_at(event_json),
        page,
        &post_description(&note.content),
        true,
        locator,
        &origin,
    );
    context.insert("content".to_string(), content);
    context.insert("mode_href".to_string(), share);
    context.insert("mode_label".to_string(), "link".to_string());
    template::shared().render("post", &context)
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
                config.limits.title_max_length,
                config.limits.alias_max_length,
                config.limits.content_max_length,
                timeout,
                &crate::crawl::pool_path(Path::new("content")),
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
    let relays = relays_for_public_fetch(hints);
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
    let rows = crate::crawl::next_pool_relays(pool, hints);
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
