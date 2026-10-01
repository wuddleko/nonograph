use std::sync::Arc;

use chrono::Utc;

use crate::save;
use crate::{Post, PostStorage};

#[derive(Debug)]
pub(crate) enum PublishFailure {
    Save(String),
    NoSlots,
}

pub(crate) fn publish_failure_href(nojs: bool, failure: PublishFailure) -> String {
    let error = match failure {
        PublishFailure::Save(message) => {
            eprintln!("Nonograph: Failed to save post: {message}");
            "save_failed"
        }
        PublishFailure::NoSlots => "no_available_slots",
    };
    if nojs {
        format!("/nojs?error={error}")
    } else {
        format!("/?error={error}")
    }
}

pub(crate) fn publish_failure_redirect(
    nojs: bool,
    failure: PublishFailure,
) -> rocket::response::Redirect {
    rocket::response::Redirect::to(publish_failure_href(nojs, failure))
}

/// Path for a post we just saved. Never a query string, never `nsec`.
pub(crate) fn published_href(nojs: bool, post_id: &str) -> String {
    let id = post_id
        .split(['?', '&', '#'])
        .next()
        .unwrap_or(post_id)
        .trim_start_matches('/');
    if nojs {
        format!("/nojs/{id}")
    } else {
        format!("/{id}")
    }
}

pub(crate) fn publish_note(
    storage: &PostStorage,
    title: &str,
    author: &str,
    rendered_content: &str,
    raw_content: &str,
) -> Result<String, PublishFailure> {
    publish_note_in_dir(storage, title, author, rendered_content, raw_content, ".")
}

pub(crate) fn publish_note_in_dir(
    storage: &PostStorage,
    title: &str,
    author: &str,
    rendered_content: &str,
    raw_content: &str,
    base_dir: &str,
) -> Result<String, PublishFailure> {
    let created_at = Utc::now();
    let id = if base_dir == "." {
        crate::generate_post_id(title, storage)
    } else {
        crate::generate_post_id_in_dir(
            title,
            storage,
            base_dir,
            &crate::generate_unguessable_segment(),
        )
    }
    .map_err(|_| PublishFailure::NoSlots)?;
    let post = Arc::new(Post {
        id: id.clone(),
        title: nonograph_parser::sanitize_text(title),
        author: nonograph_parser::sanitize_text(author),
        content: rendered_content.to_string(),
        raw_content: raw_content.to_string(),
        created_at,
    });
    if let Err(error) = save::save_post_to_file_in_dir(&post, base_dir) {
        return Err(PublishFailure::Save(error.to_string()));
    }
    storage.write().unwrap().insert(post.id.clone(), post);
    Ok(id)
}

#[cfg(test)]
#[path = "../test/publish.rs"]
mod tests;
