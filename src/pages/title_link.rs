use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use crate::nostr::hex_encode;

const TITLE_TAG: &str = "nostorgraph-title-file-v1";
const SLUG_CHARS: usize = 128;
const SLUG_TRIES: u32 = 1000;
const NAME_MAX: usize = 255;
const SLUG_SUFFIX_BYTES: usize = 5;

static TITLE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn save(
    title: &str,
    locator: &str,
    event_id: &str,
    relays: &[String],
    seal_claim: &str,
) -> Result<String, ()> {
    if !super::seal_row::storable(locator, event_id, relays) {
        return Err(());
    }
    let _lock = TITLE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(slug) = slug_owned_by(locator) {
        super::seal_row::remember(locator, event_id, relays, seal_claim)?;
        return Ok(slug);
    }
    if !super::seal_row::claim_matches(locator, event_id, relays, seal_claim) {
        return Err(());
    }
    if !super::seal_row::dir_has_room(&slug_dir(), super::seal_row::stored_cap()) {
        return Err(());
    }
    let base = slugify(title).ok_or(())?;
    let mut number = 1u32;
    let (slug, created) = loop {
        if number > SLUG_TRIES {
            return Err(());
        }
        let candidate = if number == 1 {
            base.clone()
        } else {
            format!("{base}-{number}")
        };
        match claim(&candidate, locator)? {
            Claim::Created => break (candidate, true),
            Claim::Reused => break (candidate, false),
            Claim::Taken => number += 1,
        }
    };
    if let Err(error) = write_slug_index(locator, &slug) {
        if created {
            remove_slug(&slug);
        }
        return Err(error);
    }
    if let Err(error) = super::seal_row::remember(locator, event_id, relays, seal_claim) {
        remove_index(locator);
        if created {
            remove_slug(&slug);
        }
        return Err(error);
    }
    Ok(slug)
}

pub(crate) fn locator_for(slug: &str) -> Option<String> {
    if !slug_name_ok(slug) {
        return None;
    }
    let text = std::fs::read_to_string(slug_path(slug)).ok()?;
    let locator = text.trim();
    if crate::nostr::sealed_locator(locator) {
        Some(locator.to_string())
    } else {
        None
    }
}

pub(super) fn slugify(title: &str) -> Option<String> {
    let mut slug = String::new();
    let mut dash = false;
    let max_bytes = NAME_MAX - SLUG_SUFFIX_BYTES;
    for ch in title.trim().chars() {
        if slug.chars().count() >= SLUG_CHARS {
            break;
        }
        if ch.is_control()
            || matches!(
                ch,
                '/' | '\\' | '?' | '#' | '%' | ':' | '*' | '"' | '<' | '>' | '|'
            )
        {
            if !slug.is_empty() {
                dash = true;
            }
            continue;
        }
        if ch.is_whitespace() {
            if !slug.is_empty() {
                dash = true;
            }
            continue;
        }
        let extra = ch.len_utf8() + usize::from(dash);
        if slug.len() + extra > max_bytes {
            break;
        }
        if dash {
            slug.push('-');
            dash = false;
        }
        slug.push(ch);
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() || slug == "." || slug == ".." || !slug_name_ok(&slug) {
        None
    } else {
        Some(slug)
    }
}

enum Claim {
    Created,
    Reused,
    Taken,
}

fn claim(slug: &str, locator: &str) -> Result<Claim, ()> {
    if !slug_name_ok(slug) {
        return Err(());
    }
    let path = slug_path(slug);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if file.write_all(locator.as_bytes()).is_err() {
                let _ = std::fs::remove_file(&path);
                return Err(());
            }
            Ok(Claim::Created)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            match locator_for(slug) {
                Some(found) if found == locator => Ok(Claim::Reused),
                _ => Ok(Claim::Taken),
            }
        }
        Err(_) => Err(()),
    }
}

fn slug_owned_by(locator: &str) -> Option<String> {
    let text = std::fs::read_to_string(index_path(locator)).ok()?;
    let slug = text.trim();
    if locator_for(slug)? == locator {
        Some(slug.to_string())
    } else {
        None
    }
}

fn write_slug_index(locator: &str, slug: &str) -> Result<(), ()> {
    let path = index_path(locator);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    let tmp = path.with_extension("writing");
    if std::fs::write(&tmp, slug.as_bytes()).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(());
    }
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(());
    }
    Ok(())
}

fn remove_slug(slug: &str) {
    let _ = std::fs::remove_file(slug_path(slug));
}

fn remove_index(locator: &str) {
    let path = index_path(locator);
    let _ = std::fs::remove_file(path.with_extension("writing"));
    let _ = std::fs::remove_file(path);
}

fn slug_dir() -> PathBuf {
    super::seal_row::content_root().join("title-slugs")
}

fn slug_path(slug: &str) -> PathBuf {
    slug_dir().join(slug)
}

fn index_path(locator: &str) -> PathBuf {
    let name = hex_encode(&tagged_hash(locator));
    super::seal_row::content_root().join("title-for").join(name)
}

fn tagged_hash(locator: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(TITLE_TAG.as_bytes());
    hasher.update([0]);
    hasher.update(locator.as_bytes());
    hasher.finalize().into()
}

fn slug_name_ok(slug: &str) -> bool {
    let chars = slug.chars().count();
    !slug.is_empty()
        && slug != "."
        && slug != ".."
        && chars <= SLUG_CHARS + SLUG_SUFFIX_BYTES
        && slug.len() <= NAME_MAX
        && !slug.contains(['/', '\\'])
        && !slug.chars().any(|ch| ch.is_control())
}
