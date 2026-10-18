use std::path::{Path, PathBuf};

use rocket::form::Form;
use rocket::http::{CookieJar, SameSite};
use rocket::response::content;
use rocket::State;
use serde::{Deserialize, Serialize};

use crawler::deal;

use crate::config::Config;
use crate::crawl::{candidates, pool_path};
use crate::csrf;
use crate::relays::{render_dealt_rows, render_nojs_relay_form};

const OPEN_SEATS: usize = 3;
const RELAY_CAP: usize = 6;
pub(crate) const RELAY_LIST_COOKIE: &str = "nostorgraph_relays";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RelayCookie {
    #[serde(default)]
    pub dealt: Vec<String>,
    #[serde(default)]
    pub pinned: Vec<String>,
}

impl RelayCookie {
    fn empty() -> Self {
        Self {
            dealt: Vec::new(),
            pinned: Vec::new(),
        }
    }
}

fn parsed_relay_cookie(value: Option<&str>) -> Option<RelayCookie> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;
    let mut cookie: RelayCookie = serde_json::from_str(value).ok()?;
    cookie.pinned.truncate(RELAY_CAP);
    cookie
        .dealt
        .truncate(RELAY_CAP.saturating_sub(cookie.pinned.len()));
    Some(cookie)
}

pub(crate) fn read_relay_cookie(value: Option<&str>) -> RelayCookie {
    parsed_relay_cookie(value).unwrap_or_else(RelayCookie::empty)
}

pub(crate) fn saved_relay_cookie(value: Option<&str>) -> Option<RelayCookie> {
    parsed_relay_cookie(value)
}

pub(crate) fn write_relay_cookie(list: &RelayCookie) -> String {
    serde_json::to_string(list)
        .unwrap_or_else(|_| serde_json::to_string(&RelayCookie::empty()).expect("empty relay list"))
}

pub(crate) fn relays_page_html(path: &Path, configured: &[String]) -> String {
    relays_page_html_with_csrf(path, "", configured)
}

pub(super) fn landing_relay_blocks(configured: &[String]) -> (String, String) {
    let urls = candidates(&relay_pool(), configured);
    let dealt = deal(&urls, OPEN_SEATS, &[], &[]);
    let sidebar = format!(
        "<div class=\"sidebar-relays\" id=\"sidebar-relays\">\n\
        <a href=\"#\" class=\"relay-sheet-backdrop\" aria-label=\"Close relays\"></a>\n\
        <div class=\"relay-sheet-panel\">\n\
        <a href=\"#\" class=\"relay-sheet-close\" aria-label=\"Close relays\">&times;</a>\n\
        <p class=\"sidebar-relays-title\">Relays</p>\n\
        {}\
        </div>\n\
        </div>\n",
        crate::relays::render_dealt_rows(&dealt, &urls)
    );
    (sidebar, crate::relays::render_about_section(&dealt, false))
}

fn relays_page_html_with_csrf(path: &Path, csrf_token: &str, configured: &[String]) -> String {
    let urls = candidates(path, configured);
    let dealt = deal(&urls, OPEN_SEATS, &[], &[]);
    let mut body = render_dealt_rows(&dealt, &urls);
    body.push_str(&format!(
        "<input type=\"hidden\" name=\"csrf_token\" value=\"{}\" />\n",
        nostorgraph_parser::html_attr_escape(csrf_token)
    ));
    relays_document(&body, true)
}

struct NojsRelaysPage {
    html: String,
    cookie: RelayCookie,
    write_cookie: bool,
}

pub(super) struct NojsRelayState {
    pub(super) cookie: RelayCookie,
    pub(super) write_cookie: bool,
}

pub(super) fn nojs_relay_state(
    path: &Path,
    saved: Option<&str>,
    configured: &[String],
) -> NojsRelayState {
    let urls = candidates(path, configured);
    let (cookie, write_cookie) = match parsed_relay_cookie(saved) {
        Some(list) => (list, false),
        None => (
            RelayCookie {
                dealt: deal(&urls, OPEN_SEATS, &[], &[]),
                pinned: Vec::new(),
            },
            true,
        ),
    };
    NojsRelayState {
        cookie,
        write_cookie,
    }
}

fn nojs_relays_view(
    path: &Path,
    saved: Option<&str>,
    csrf_token: &str,
    configured: &[String],
) -> NojsRelaysPage {
    let state = nojs_relay_state(path, saved, configured);
    let body = render_nojs_relay_form(
        &state.cookie.dealt,
        &state.cookie.pinned,
        csrf_token,
        "",
        "",
    );
    NojsRelaysPage {
        html: relays_document(&body, false),
        cookie: state.cookie,
        write_cookie: state.write_cookie,
    }
}

pub(super) fn set_relay_list_cookie(cookies: &CookieJar<'_>, list: &RelayCookie, secure: bool) {
    cookies.add(relay_list_cookie(&write_relay_cookie(list), secure));
}

fn relays_document(body: &str, relay_script: bool) -> String {
    let version = super::assets::version();
    let script = if relay_script {
        format!("\n    <script src=\"/relays.js?v={version}\"></script>")
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Relays</title>
    <link rel="icon" href="data:," />
    <script src="/theme.js?v={version}"></script>
    <link rel="stylesheet" href="/home.css?v={version}" />
</head>
<body class="relays-page">
    <h1>Relays</h1>
    {body}{script}
</body>
</html>"#
    )
}

fn relay_list_cookie(value: &str, secure: bool) -> rocket::http::Cookie<'static> {
    rocket::http::Cookie::build((RELAY_LIST_COOKIE, value.to_owned()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .max_age(rocket::time::Duration::seconds(60 * 60 * 24 * 365))
        .into()
}

#[get("/relays")]
pub fn relays_page(
    cookies: &CookieJar<'_>,
    config: &State<Config>,
    https: csrf::HttpsForward,
) -> content::RawHtml<String> {
    let token = csrf::form_csrf_token(config, Some(cookies), https.0);
    content::RawHtml(relays_page_html_with_csrf(
        &pool_path(Path::new("content")),
        &token,
        &config.nostr.relays,
    ))
}

#[derive(FromForm)]
pub(crate) struct RelayForm {
    #[field(default = String::new())]
    action: String,
    #[field(default = String::new())]
    relay: String,
    #[field(default = String::new())]
    remove: String,
    #[field(default = String::new())]
    count: String,
    #[field(default = String::new())]
    csrf_token: String,
}

fn drop_relay(list: &RelayCookie, url: &str) -> RelayCookie {
    let url = url.trim();
    RelayCookie {
        dealt: list
            .dealt
            .iter()
            .filter(|have| have.as_str() != url)
            .cloned()
            .collect(),
        pinned: list
            .pinned
            .iter()
            .filter(|have| have.as_str() != url)
            .cloned()
            .collect(),
    }
}

fn pin_relay(list: &RelayCookie, url: &str) -> Result<RelayCookie, &'static str> {
    let Some(canonical) = crate::nostr::canonical_public_relay(url) else {
        return Err(if url.trim().is_empty() {
            "Enter a relay URL."
        } else {
            "Use a public wss:// relay URL."
        });
    };
    if list.pinned.iter().any(|have| have == &canonical) {
        return Ok(list.clone());
    }
    if list.pinned.len() >= RELAY_CAP {
        return Err("Six relays are already pinned.");
    }
    let mut pinned = list.pinned.clone();
    pinned.push(canonical.clone());
    let mut dealt: Vec<String> = list
        .dealt
        .iter()
        .filter(|have| *have != &canonical)
        .cloned()
        .collect();
    while dealt.len() + pinned.len() > RELAY_CAP {
        dealt.pop();
    }
    Ok(RelayCookie { dealt, pinned })
}

fn bounded_count(pinned: usize, count: &str) -> usize {
    let floor = pinned.min(RELAY_CAP);
    let requested = count.trim().parse::<usize>().unwrap_or(floor);
    requested.max(floor).min(RELAY_CAP)
}

fn set_relay_count(
    path: &Path,
    list: &RelayCookie,
    count: &str,
    configured: &[String],
) -> RelayCookie {
    let total = bounded_count(list.pinned.len(), count);
    let open = total.saturating_sub(list.pinned.len());
    let mut dealt: Vec<String> = list.dealt.iter().take(open).cloned().collect();
    if dealt.len() < open {
        let mut skip = list.pinned.clone();
        skip.extend(dealt.iter().cloned());
        dealt.extend(deal(
            &candidates(path, configured),
            open - dealt.len(),
            &skip,
            &[],
        ));
    }
    RelayCookie {
        dealt,
        pinned: list.pinned.clone(),
    }
}

fn reshuffle_relays(
    path: &Path,
    list: &RelayCookie,
    count: &str,
    configured: &[String],
) -> RelayCookie {
    let total = bounded_count(list.pinned.len(), count);
    let open = total.saturating_sub(list.pinned.len());
    RelayCookie {
        dealt: deal(
            &candidates(path, configured),
            open,
            &list.pinned,
            &list.dealt,
        ),
        pinned: list.pinned.clone(),
    }
}

pub(super) fn relay_pool() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = test_content_dir() {
        return pool_path(&dir);
    }
    pool_path(Path::new("content"))
}

#[cfg(test)]
fn test_content_dir() -> Option<PathBuf> {
    TEST_CONTENT_DIR.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
thread_local! {
    static TEST_CONTENT_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct RelayContentGuard;

#[cfg(test)]
impl RelayContentGuard {
    pub(super) fn set(path: &Path) -> Self {
        TEST_CONTENT_DIR.with(|slot| *slot.borrow_mut() = Some(path.to_path_buf()));
        Self
    }
}

#[cfg(test)]
impl Drop for RelayContentGuard {
    fn drop(&mut self) {
        TEST_CONTENT_DIR.with(|slot| *slot.borrow_mut() = None);
    }
}

fn save_relay_list(cookies: &CookieJar<'_>, list: &RelayCookie, config: &Config, secure: bool) {
    cookies.add(relay_list_cookie(&write_relay_cookie(list), secure));
    let relays = list
        .pinned
        .iter()
        .cloned()
        .chain(list.dealt.iter().cloned())
        .collect();
    crate::nostr::schedule_relay_assignments(
        relays,
        Vec::new(),
        crate::relay_set::relay_timeout(config),
    );
}

#[get("/nojs/relays")]
pub fn nojs_relays_page(
    cookies: &CookieJar<'_>,
    config: &State<Config>,
    https: csrf::HttpsForward,
) -> content::RawHtml<String> {
    let secure = https.0;
    let token = csrf::form_csrf_token(config, Some(cookies), secure);
    let saved = cookies
        .get(RELAY_LIST_COOKIE)
        .map(|cookie| cookie.value().to_string());
    let page = nojs_relays_view(
        &relay_pool(),
        saved.as_deref(),
        &token,
        &config.nostr.relays,
    );
    if page.write_cookie {
        set_relay_list_cookie(cookies, &page.cookie, secure);
    }
    content::RawHtml(page.html)
}

fn nojs_form_location(add_error: &str, typed: &str) -> String {
    let code = match add_error {
        "" => return "/nojs".to_string(),
        "Enter a relay URL." => "relay_required",
        "Use a public wss:// relay URL." => "relay_invalid",
        "Six relays are already pinned." => "relay_full",
        "The publish form expired. Try again." => "csrf_token_invalid",
        _ => return "/nojs".to_string(),
    };
    if typed.is_empty() {
        return format!("/nojs?error={code}");
    }
    let typed = url::form_urlencoded::byte_serialize(typed.as_bytes()).collect::<String>();
    format!("/nojs?error={code}&typed={typed}")
}

#[post("/nojs/relays", data = "<form>")]
pub fn nojs_relays_update(
    form: Form<RelayForm>,
    cookies: &CookieJar<'_>,
    config: &State<Config>,
    https: csrf::HttpsForward,
) -> rocket::response::Redirect {
    let secure = https.0;
    let pool = relay_pool();
    let saved = cookies
        .get(RELAY_LIST_COOKIE)
        .map(|cookie| cookie.value().to_string());
    let list = read_relay_cookie(saved.as_deref());
    let mut add_error = "";
    let csrf_cookie = csrf::request_csrf_cookie(cookies);
    if config.security.csrf_protection_enabled
        && !csrf::accepts_submission(&form.csrf_token, &csrf_cookie)
    {
        add_error = "The publish form expired. Try again.";
    } else if !form.remove.is_empty() {
        let next = drop_relay(&list, &form.remove);
        if next != list {
            save_relay_list(cookies, &next, config, secure);
        }
    } else if form.action == "delete" {
        save_relay_list(cookies, &RelayCookie::empty(), config, secure);
    } else if form.action == "add" {
        match pin_relay(&list, &form.relay) {
            Ok(next) => {
                if next != list {
                    save_relay_list(cookies, &next, config, secure);
                }
            }
            Err(error) => add_error = error,
        }
    } else if form.action == "count" || form.action == "up" || form.action == "down" {
        let count = if form.action == "up" {
            (list.pinned.len() + list.dealt.len())
                .saturating_add(1)
                .to_string()
        } else if form.action == "down" {
            (list.pinned.len() + list.dealt.len())
                .saturating_sub(1)
                .to_string()
        } else {
            form.count.clone()
        };
        let next = set_relay_count(&pool, &list, &count, &config.nostr.relays);
        if next != list {
            save_relay_list(cookies, &next, config, secure);
        }
    } else if form.action == "reshuffle" {
        let next = reshuffle_relays(&pool, &list, &form.count, &config.nostr.relays);
        if next != list {
            save_relay_list(cookies, &next, config, secure);
        }
    }
    let typed = if matches!(
        add_error,
        "Use a public wss:// relay URL." | "Six relays are already pinned."
    ) {
        form.relay.as_str()
    } else {
        ""
    };
    rocket::response::Redirect::to(nojs_form_location(add_error, typed))
}

#[cfg(test)]
#[path = "../../test/relay_pages.rs"]
mod tests;
