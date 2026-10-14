mod assets;
mod fetch_slot;
mod home;
mod layout;
#[cfg(test)]
mod nojs_test;
mod relays;
mod sealed;
mod static_pages;

pub use assets::{
    cache_control_for_path, entity_tag_for_path, home_css, home_js, home_noscript_css,
    if_none_match_matches, nostr_js, notice_css, parser_js, parser_wasm, post_css, post_js,
    post_noscript_css, relays_js, robots_txt, secp256k1_js, theme_js, warm, writemark_css,
    writemark_js, HOME_CSS_PATH, HOME_JS_PATH, HOME_NOSCRIPT_CSS_PATH, NOSTR_JS_PATH,
    NOTICE_CSS_PATH, PAGE_JS_PATH, PAGE_WASM_PATH, POST_CSS_PATH, POST_JS_PATH,
    POST_NOSCRIPT_CSS_PATH, RELAYS_JS_PATH, SECP256K1_JS_PATH, THEME_JS_PATH, WRITEMARK_CSS_PATH,
    WRITEMARK_JS_PATH,
};
pub(crate) use assets::{post_scripts, PAGE_JS};
pub(crate) use fetch_slot::{
    release_public_fetch, try_acquire_public_fetch, PublicFetchSlot, MAX_PUBLIC_FETCHES,
};
pub use home::{
    assign_tor_circuits, index, nojs_create_post, nojs_index, nostr_publish, tor_circuits,
    NostrPublishBody,
};
pub(crate) use home::{
    create_location, create_location_with_cookie, note_and_relays_for_publish,
    relays_for_circuit_pass, tab_relay_timeout_ms, NewPost, TorCircuitsBody,
};
pub(crate) use layout::{fill_layout, mode_href, mode_label, not_found_html, post_description};
pub use relays::{nojs_relays_page, nojs_relays_update, relays_page};
pub(crate) use sealed::{
    fetch_sealed_by_id, nojs_open_fetched, nojs_opened_page, nojs_query_secret,
    nojs_sealed_response, sealed_shell,
};
pub use sealed::{nojs_sealed_view, sealed_open, sealed_view};
pub use static_pages::{about_page, api_page, legal_page, markup_page, nojs_static_page};
pub(crate) use static_pages::{build_static_page, BuiltPage};

#[cfg(test)]
use rocket::http::{ContentType, Status};

#[cfg(test)]
use home::{
    home_context, home_error_message, nojs_error_code, nojs_publish_failure_page,
    nojs_publish_relays, nojs_publish_slot, nojs_sealed_location, publish_nojs_note_with_cookie,
    publish_result, publish_sealed_note,
};
#[cfg(test)]
use nojs_test::{
    nojs_fetch_hints_seen, nojs_publish_relays_seen, note_nojs_publish_relays,
    set_nojs_test_accepted, set_nojs_test_fetch, set_shared_nojs_accepted, NojsTestFetch,
    NOJS_FETCH_HINTS, NOJS_RECORD_HINTS,
};

pub(crate) use crate::relay_set::{public_relays, relays_to_assign};

#[cfg(test)]
#[path = "../../test/pages.rs"]
mod tests;
