use std::sync::Arc;

use chrono::Utc;

use crate::config::Config;
use crate::nostr;
use crate::save;
use crate::{Post, PostStorage};

pub(crate) enum PublishFailure {
    Relays,
    Save(String),
}

pub(crate) fn publish_failure_redirect(
    nojs: bool,
    failure: PublishFailure,
) -> rocket::response::Redirect {
    let error = match failure {
        PublishFailure::Relays => "nostr_publish_failed",
        PublishFailure::Save(message) => {
            eprintln!("Nonograph: Failed to save post: {message}");
            "save_failed"
        }
    };
    let url = if nojs {
        format!("/nojs?error={error}")
    } else {
        format!("/?error={error}")
    };
    rocket::response::Redirect::to(url)
}

pub(crate) fn publish_note(
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
    let post = Arc::new(Post {
        id: wrapped.id_hex(),
        title: nonograph_parser::sanitize_text(title),
        author: nonograph_parser::sanitize_text(author),
        content: rendered_content.to_string(),
        raw_content: raw_content.to_string(),
        created_at,
    });
    if let Err(error) = save::save_post_to_file_in_dir(&post, ".") {
        return Err(PublishFailure::Save(error.to_string()));
    }
    storage.write().unwrap().insert(post.id.clone(), post);
    Ok(format!("{nevent}?nsec={nsec}"))
}

#[cfg(test)]
#[path = "../test/publish.rs"]
mod tests;
