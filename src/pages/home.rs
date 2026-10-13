use std::collections::HashMap;

use rocket::http::{CookieJar, SameSite, Status};
use rocket::response::content;
use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::csrf;
use crate::relay_set::{
    push_public_relay, relay_timeout, relays_for_public_fetch, relays_for_public_publish,
};
use crate::render_options;
use crate::template;

use super::fetch_slot::PublicFetchSlot;

fn home_content_field(
    nojs: bool,
    max_length: &str,
    content: &str,
    options: &nonograph_parser::RenderOptions,
) -> String {
    if nojs {
        format!(
            "<textarea name=\"content\" placeholder=\"Write something with markdown...\" maxlength=\"{max_length}\" title=\"Mind your opsec, be careful what you share.\" required>{}</textarea>",
            nonograph_parser::html_escape(content)
        )
    } else {
        let theme = nonograph_parser::html_attr_escape(&options.syntax_theme);
        let external = if options.external_link_security {
            "true"
        } else {
            "false"
        };
        format!(
            "<writemark-editor name=\"content\" mode=\"live\" placeholder=\"Write something with markdown or type / for formatting options...\" maxlength=\"{max_length}\" data-syntax-theme=\"{theme}\" data-max-url-length=\"{max_url}\" data-external-links=\"{external}\" title=\"Mind your opsec, be careful what you share.\" required></writemark-editor>",
            max_url = options.max_url_length,
        )
    }
}

fn split_home_error(error: Option<&str>) -> (Option<&str>, &str) {
    match error {
        Some(code @ ("relay_required" | "relay_invalid" | "relay_configured")) => (None, code),
        other => (other, ""),
    }
}

pub(super) fn home_error_message(error: Option<&str>) -> String {
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
        _ => "Something went wrong. Try again.".to_string(),
    }
}

pub(crate) const MIN_TAB_RELAY_TIMEOUT_MS: u64 = 10_000;

pub(crate) fn tab_relay_timeout_ms(timeout_secs: u64) -> u64 {
    timeout_secs
        .max(1)
        .saturating_mul(1000)
        .max(MIN_TAB_RELAY_TIMEOUT_MS)
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

struct HomeDraft<'a> {
    title: &'a str,
    alias: &'a str,
    content: &'a str,
    saved_relay: &'a str,
    typed_relay: &'a str,
    list_relay: bool,
    surface_relay_error: bool,
}

pub(super) fn home_context(
    config: &Config,
    nojs: bool,
    error: Option<&str>,
    cookies: Option<&CookieJar<'_>>,
) -> HashMap<String, String> {
    home_form_context(
        config,
        nojs,
        error,
        HomeDraft {
            title: "",
            alias: "",
            content: "",
            saved_relay: "",
            typed_relay: "",
            list_relay: false,
            surface_relay_error: false,
        },
        cookies,
    )
}

fn home_form_context(
    config: &Config,
    nojs: bool,
    error: Option<&str>,
    draft: HomeDraft<'_>,
    cookies: Option<&CookieJar<'_>>,
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
    context.insert("title_value".to_string(), draft.title.to_string());
    context.insert("alias_value".to_string(), draft.alias.to_string());
    context.insert(
        "content_field".to_string(),
        home_content_field(nojs, &max_length, draft.content, &render_options(config)),
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
    if draft.surface_relay_error && !relay_error.is_empty() {
        form_error = Some(relay_error);
    }
    context.insert("error".to_string(), home_error_message(form_error));
    let relay_message = if relay_error.is_empty() {
        String::new()
    } else {
        home_error_message(Some(relay_error))
    };
    context.insert("csrf_token".to_string(), form_csrf_token(config, cookies));
    context.insert(
        "form_action".to_string(),
        if nojs { "/nojs/create" } else { "/nojs" }.to_string(),
    );
    context.insert(
        "form_method".to_string(),
        if nojs { "post" } else { "get" }.to_string(),
    );
    let version = super::assets::version();
    context.insert("asset_version".to_string(), version.to_string());
    context.insert(
        "scripts".to_string(),
        if nojs {
            String::new()
        } else {
            super::assets::home_scripts(version)
        },
    );
    context.insert(
        "theme_boot".to_string(),
        if nojs {
            String::new()
        } else {
            super::layout::theme_boot(version)
        },
    );
    let relays = relays_for_public_fetch(&[], &config.nostr.relays);
    context.insert(
        "relays_sidebar".to_string(),
        crate::relays::render_sidebar_field(
            &relays,
            nojs,
            draft.saved_relay,
            draft.typed_relay,
            draft.list_relay,
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
            crate::relays::render_saved_relay_input(draft.saved_relay)
        } else {
            String::new()
        },
    );
    let origin = config.normalized_public_origin().unwrap_or_default();
    context.insert(
        "public_url".to_string(),
        super::layout::join_public_origin(&origin, "/"),
    );
    context
}

fn about_durability(nojs: bool) -> &'static str {
    if nojs {
        "The host seals the note and does not keep a copy. The writing is ordinary markup, not a proprietary format."
    } else {
        "The browser seals the note and this host does not keep a copy. The writing is ordinary markup, not a proprietary format."
    }
}

fn render_home(
    config: &Config,
    nojs: bool,
    error: Option<&str>,
    cookies: &CookieJar<'_>,
) -> content::RawHtml<String> {
    let context = home_context(config, nojs, error, Some(cookies));
    match template::shared().render("home", &context) {
        Ok(html) => content::RawHtml(html),
        Err(error) => content::RawHtml(format!("Template error: {error}")),
    }
}

#[get("/?<error>")]
pub fn index(
    error: Option<&str>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> content::RawHtml<String> {
    render_home(config, false, error, cookies)
}

#[get("/nojs?<error>")]
pub fn nojs_index(
    error: Option<&str>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> content::RawHtml<String> {
    render_home(config, true, error, cookies)
}

#[derive(Clone, FromForm)]
pub(crate) struct NewPost {
    pub(super) title: String,
    pub(super) content: String,
    pub(super) alias: String,
    pub(super) csrf_token: String,
    #[field(default = String::new())]
    pub(super) relay: String,
    #[field(default = String::new())]
    pub(super) relay_box: String,
    #[field(default = false)]
    pub(super) relay_remove: bool,
}

fn csrf_cookie(token: &str) -> rocket::http::Cookie<'static> {
    rocket::http::Cookie::build((csrf::COOKIE_NAME, token.to_owned()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(false)
        .max_age(rocket::time::Duration::seconds(csrf::TOKEN_TTL_SECS as i64))
        .into()
}

fn form_csrf_token(config: &Config, cookies: Option<&CookieJar<'_>>) -> String {
    if !config.security.csrf_protection_enabled {
        return String::new();
    }
    let existing = cookies.and_then(|jar| {
        jar.get(csrf::COOKIE_NAME)
            .map(|cookie| cookie.value().to_string())
    });
    let (token, set_cookie) = csrf::token_for_form(existing.as_deref());
    if set_cookie {
        if let Some(jar) = cookies {
            jar.add(csrf_cookie(&token));
        }
    }
    token
}

fn request_csrf_cookie(cookies: &CookieJar<'_>) -> String {
    cookies
        .get(csrf::COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
        .unwrap_or_default()
}

fn csrf_failure(config: &Config, field: &str, cookie: &str) -> Option<&'static str> {
    if config.security.csrf_protection_enabled && !csrf::accepts_submission(field, cookie) {
        Some("csrf_token_invalid")
    } else {
        None
    }
}

pub(crate) fn create_location(form: &NewPost, config: &Config, csrf_cookie: &str) -> String {
    create_location_with_cookie(form, config, csrf_cookie, None)
}

fn create_location_with_cookie(
    form: &NewPost,
    config: &Config,
    csrf_cookie: &str,
    relay_cookie: Option<&str>,
) -> String {
    let home = "/nojs";
    if let Some(error) = csrf_failure(config, &form.csrf_token, csrf_cookie) {
        return format!("{home}?error={error}");
    }

    let title = form.title.trim();
    let alias = form.alias.trim();
    let alias_check = if alias.is_empty() { None } else { Some(alias) };
    if let Err(error) = config.validate_post(title, &form.content, alias_check) {
        return format!("{home}?error={error}");
    }
    let relay = nojs_chosen_relay(form);
    if !relay.is_empty() && !crate::nostr::public_relay_url(relay) {
        return format!("{home}?error=relay_invalid");
    }
    if relays_for_nojs_write(relay_cookie, relay, &config.nostr.relays).is_none() {
        return format!("{home}?error=nostr_publish_failed");
    }
    match publish_nojs_note_with_cookie(form, config, relay_cookie) {
        Ok(published) => nojs_sealed_location(&published, &config.nostr.relays)
            .unwrap_or_else(|| format!("{home}?error=nostr_publish_failed")),
        Err(error) => format!("{home}?error={error}"),
    }
}

pub(super) struct NojsPublished {
    pub(super) accepted: Vec<String>,
    pub(super) locator: String,
    pub(super) event_id: String,
    pub(super) payload: String,
    pub(super) k: String,
}

pub(super) fn publish_nojs_note(
    form: &NewPost,
    config: &Config,
) -> Result<NojsPublished, &'static str> {
    publish_nojs_note_with_cookie(form, config, None)
}

fn publish_nojs_note_with_cookie(
    form: &NewPost,
    config: &Config,
    relay_cookie: Option<&str>,
) -> Result<NojsPublished, &'static str> {
    let relays = relays_for_nojs_write(relay_cookie, nojs_chosen_relay(form), &config.nostr.relays)
        .ok_or("nostr_publish_failed")?;
    #[cfg(test)]
    super::nojs_test::note_nojs_publish_relays(&relays);
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
    super::nojs_test::remember_nojs_test_event(&sealed.note.event_json);
    let timeout = relay_timeout(config);
    let accepted = {
        let _slot = nojs_publish_slot().map_err(|_| "nostr_publish_failed")?;
        #[cfg(test)]
        {
            if let Some(accepted) = super::nojs_test::nojs_test_accepted() {
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

pub(super) fn nojs_sealed_location(
    published: &NojsPublished,
    configured: &[String],
) -> Option<String> {
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
    Some(super::layout::js_seal_href(
        &published.locator,
        &published.event_id,
        &published.k,
        &hints,
    ))
}

pub(super) fn nojs_chosen_relay(form: &NewPost) -> &str {
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

fn relays_for_nojs_write(
    relay_cookie: Option<&str>,
    relay: &str,
    configured: &[String],
) -> Option<Vec<String>> {
    if let Some(list) = super::relays::saved_relay_cookie(relay_cookie) {
        let mut relays = Vec::new();
        for url in list.pinned.iter().chain(list.dealt.iter()) {
            push_public_relay(&mut relays, url);
        }
        if relays.is_empty() {
            None
        } else {
            Some(relays)
        }
    } else {
        nojs_publish_relays(relay, configured)
    }
}

pub(super) fn nojs_publish_relays(relay: &str, configured: &[String]) -> Option<Vec<String>> {
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

pub(super) fn nojs_publish_slot() -> Result<PublicFetchSlot, &'static str> {
    PublicFetchSlot::acquire().ok_or("/nojs?error=nostr_publish_failed")
}

fn nojs_relay_add_status(
    relay: &str,
    csrf_token: &str,
    config: &Config,
    csrf_cookie: &str,
) -> (Option<&'static str>, bool) {
    if let Some(error) = csrf_failure(config, csrf_token, csrf_cookie) {
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
    cookies: Option<&CookieJar<'_>>,
) -> content::RawHtml<String> {
    let saved = crate::nostr::canonical_public_relay(form.relay.trim()).unwrap_or_default();
    let context = home_form_context(
        config,
        true,
        error,
        HomeDraft {
            title: &form.title,
            alias: &form.alias,
            content: &form.content,
            saved_relay: &saved,
            typed_relay: form.relay_box.trim(),
            list_relay,
            surface_relay_error,
        },
        cookies,
    );
    match template::shared().render("home", &context) {
        Ok(html) => content::RawHtml(html),
        Err(error) => content::RawHtml(format!("Template error: {error}")),
    }
}

pub(super) fn render_nojs_draft(
    config: &Config,
    form: &NewPost,
    cookies: Option<&CookieJar<'_>>,
    csrf_cookie: &str,
) -> content::RawHtml<String> {
    let mut form = form.clone();
    if let Some(error) = csrf_failure(config, &form.csrf_token, csrf_cookie) {
        return render_nojs_form(
            config,
            &form,
            Some(error),
            nojs_lists_extra_relay(form.relay.trim(), config),
            false,
            cookies,
        );
    }
    if form.relay_remove {
        form.relay.clear();
        return render_nojs_form(config, &form, None, false, false, cookies);
    }
    form.relay_box = form.relay_box.trim().to_string();
    let (error, accept) =
        nojs_relay_add_status(&form.relay_box, &form.csrf_token, config, csrf_cookie);
    if accept {
        form.relay = crate::nostr::canonical_public_relay(&form.relay_box)
            .unwrap_or_else(|| form.relay_box.clone());
        form.relay_box.clear();
        return render_nojs_form(config, &form, None, true, false, cookies);
    }
    render_nojs_form(
        config,
        &form,
        error,
        nojs_lists_extra_relay(form.relay.trim(), config),
        false,
        cookies,
    )
}

pub(super) fn nojs_error_code(location: &str) -> Option<&str> {
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

pub(super) fn nojs_publish_failure_page(
    config: &Config,
    form: &NewPost,
    error: &str,
    cookies: Option<&CookieJar<'_>>,
) -> content::RawHtml<String> {
    render_nojs_form(
        config,
        form,
        Some(error),
        nojs_lists_extra_relay(form.relay.trim(), config),
        true,
        cookies,
    )
}

#[post("/nojs/relay", data = "<form>")]
pub fn nojs_add_relay(
    form: rocket::form::Form<NewPost>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> content::RawHtml<String> {
    let csrf_cookie = request_csrf_cookie(cookies);
    render_nojs_draft(config, &form, Some(cookies), &csrf_cookie)
}

#[post("/nojs/create", data = "<form>")]
pub async fn nojs_create_post(
    form: rocket::form::Form<NewPost>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> rocket::Either<rocket::response::Redirect, content::RawHtml<String>> {
    let form = form.into_inner();
    let config_owned = config.inner().clone();
    let moved = form.clone();
    let csrf_cookie = request_csrf_cookie(cookies);
    let relay_cookie = cookies
        .get(super::relays::RELAY_LIST_COOKIE)
        .map(|cookie| cookie.value().to_string());
    let location = match rocket::tokio::task::spawn_blocking(move || {
        create_location_with_cookie(&moved, &config_owned, &csrf_cookie, relay_cookie.as_deref())
    })
    .await
    {
        Ok(location) => location,
        Err(_) => {
            return rocket::Either::Right(nojs_publish_failure_page(
                config,
                &form,
                "nostr_publish_failed",
                Some(cookies),
            ));
        }
    };
    if let Some(error) = nojs_error_code(&location) {
        rocket::Either::Right(nojs_publish_failure_page(
            config,
            &form,
            error,
            Some(cookies),
        ))
    } else {
        rocket::Either::Left(rocket::response::Redirect::to(location))
    }
}

#[derive(Deserialize)]
pub struct NostrPublishBody {
    pub(super) event: serde_json::Value,
    #[serde(default)]
    pub(super) relays: Vec<String>,
    #[serde(default)]
    pub(super) csrf_token: String,
}

#[derive(Serialize)]
pub struct NostrPublishResult {
    pub(super) locator: String,
    pub(super) accepted: Vec<String>,
}

pub(crate) fn note_and_relays_for_publish(
    body: &NostrPublishBody,
    config: &Config,
    csrf_cookie: &str,
) -> Option<(crate::nostr::SignedNote, Vec<String>, String)> {
    if csrf_failure(config, &body.csrf_token, csrf_cookie).is_some() {
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

pub(super) fn publish_result(locator: String, accepted: Vec<String>) -> NostrPublishResult {
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
    pub(super) relays: Vec<String>,
    #[serde(default)]
    pub(super) csrf_token: String,
}

pub(crate) fn relays_for_circuit_pass(
    body: &TorCircuitsBody,
    config: &Config,
    csrf_cookie: &str,
) -> Option<Vec<String>> {
    if csrf_failure(config, &body.csrf_token, csrf_cookie).is_some() {
        return None;
    }
    Some(body.relays.clone())
}

#[post("/tor-circuits", data = "<body>")]
pub fn assign_tor_circuits(
    body: Json<TorCircuitsBody>,
    config: &State<Config>,
    cookies: &CookieJar<'_>,
) -> Status {
    let config = config.inner();
    let csrf_cookie = request_csrf_cookie(cookies);
    let Some(relays) = relays_for_circuit_pass(&body, config, &csrf_cookie) else {
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
    cookies: &CookieJar<'_>,
) -> Result<Json<NostrPublishResult>, Status> {
    let config = config.inner().clone();
    let body = body.into_inner();
    let csrf_cookie = request_csrf_cookie(cookies);
    let Some(_slot) = PublicFetchSlot::acquire() else {
        return Err(Status::ServiceUnavailable);
    };
    match rocket::tokio::task::spawn_blocking(move || {
        let (note, relays, locator) = note_and_relays_for_publish(&body, &config, &csrf_cookie)?;
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
