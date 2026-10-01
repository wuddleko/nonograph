use std::fs;
use std::io::Write;
use std::path::Path;

use crate::Post;

#[derive(Debug)]
pub enum SaveError {
    AlreadyExists,
    Io(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::AlreadyExists => write!(f, "post file already exists"),
            SaveError::Io(message) => write!(f, "{message}"),
        }
    }
}

pub fn save_post_to_file_in_dir(post: &Post, base_dir: &str) -> Result<(), SaveError> {
    if !crate::is_valid_post_id(&post.id) {
        return Err(SaveError::Io("invalid post id".to_string()));
    }
    ensure_content_dir(base_dir)?;

    let mut frontmatter = String::from("---\n");
    frontmatter.push_str(&format!("title: {}\n", post.title));
    frontmatter.push_str(&format!("date: {}\n", post.created_at.format("%Y-%m-%d")));
    if !post.author.is_empty() {
        frontmatter.push_str(&format!("author: {}\n", post.author));
    }
    if let Some(nostr_id) = &post.nostr_id {
        if crate::is_nostr_identifier(nostr_id) {
            frontmatter.push_str(&format!("nostr: {nostr_id}\n"));
        }
    }
    frontmatter.push_str(&format!(
        "generator: {} v{}\n",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    ));
    frontmatter.push_str("---\n\n");

    let file_content = format!("{}{}", frontmatter, post.raw_content);

    write_bytes(
        &post_path(&post.id, base_dir),
        file_content.as_bytes(),
        false,
    )
}

pub fn save_alias_pointer_in_dir(
    cache_id: &str,
    short_id: &str,
    base_dir: &str,
) -> Result<(), SaveError> {
    write_alias_pointer(cache_id, short_id, base_dir, false)
}

pub(crate) fn write_alias_pointer(
    cache_id: &str,
    short_id: &str,
    base_dir: &str,
    replace: bool,
) -> Result<(), SaveError> {
    if !crate::is_valid_post_id(cache_id) || !crate::is_valid_post_id(short_id) {
        return Err(SaveError::Io("invalid post id".to_string()));
    }
    ensure_content_dir(base_dir)?;
    write_bytes(
        &post_path(cache_id, base_dir),
        format!("---\nalias: {short_id}\n---\n").as_bytes(),
        replace,
    )
}

fn ensure_content_dir(base_dir: &str) -> Result<(), SaveError> {
    let content_dir = Path::new(base_dir).join("content");
    if !content_dir.exists() {
        fs::create_dir_all(&content_dir)
            .map_err(|e| SaveError::Io(format!("Failed to create content directory: {}", e)))?;
    }
    Ok(())
}

fn write_bytes(file_path: &Path, bytes: &[u8], replace: bool) -> Result<(), SaveError> {
    if replace {
        let mut tmp_name = file_path.as_os_str().to_os_string();
        tmp_name.push(".tmp");
        let tmp_path = Path::new(&tmp_name);
        let _ = fs::remove_file(tmp_path);
        write_bytes(tmp_path, bytes, false)?;
        if let Err(e) = fs::rename(tmp_path, file_path) {
            let _ = fs::remove_file(tmp_path);
            return Err(SaveError::Io(format!(
                "Failed to write post to file {:?}: {}",
                file_path, e
            )));
        }
        return Ok(());
    }
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file_path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(SaveError::AlreadyExists);
        }
        Err(e) => {
            return Err(SaveError::Io(format!(
                "Failed to write post to file {:?}: {}",
                file_path, e
            )));
        }
    };
    if let Err(e) = file.write_all(bytes) {
        let _ = fs::remove_file(file_path);
        return Err(SaveError::Io(format!(
            "Failed to write post to file {:?}: {}",
            file_path, e
        )));
    }
    Ok(())
}

pub fn remove_post_file_in_dir(post_id: &str, base_dir: &str) -> bool {
    if !crate::is_valid_post_id(post_id) {
        return false;
    }
    fs::remove_file(post_path(post_id, base_dir)).is_ok()
}

pub fn read_post_file_in_dir(post_id: &str, base_dir: &str) -> Option<String> {
    if !crate::is_valid_post_id(post_id) {
        return None;
    }
    let file_content = fs::read_to_string(post_path(post_id, base_dir)).ok()?;
    if let Some(alias) = alias_target(&file_content, post_id) {
        return fs::read_to_string(post_path(&alias, base_dir)).ok();
    }
    Some(file_content)
}

pub fn alias_target_in_dir(post_id: &str, base_dir: &str) -> Option<String> {
    if !crate::is_valid_post_id(post_id) {
        return None;
    }
    let file_content = fs::read_to_string(post_path(post_id, base_dir)).ok()?;
    alias_target(&file_content, post_id)
}

pub(crate) fn alias_target(file_content: &str, post_id: &str) -> Option<String> {
    let alias = crate::yaml_frontmatter_field(file_content, "alias:")?;
    if crate::is_valid_post_id(&alias) && alias != post_id {
        Some(alias)
    } else {
        None
    }
}

pub fn post_file_is_live_in_dir(post_id: &str, base_dir: &str) -> bool {
    let Ok(file_content) = fs::read_to_string(post_path(post_id, base_dir)) else {
        return false;
    };
    match alias_target(&file_content, post_id) {
        Some(alias) => post_file_exists_in_dir(&alias, base_dir),
        None => true,
    }
}

pub(crate) fn post_path(post_id: &str, base_dir: &str) -> std::path::PathBuf {
    Path::new(base_dir)
        .join("content")
        .join(format!("{post_id}.md"))
}

pub fn post_file_exists(post_id: &str) -> bool {
    post_file_exists_in_dir(post_id, ".")
}

pub fn post_file_exists_in_dir(post_id: &str, base_dir: &str) -> bool {
    post_path(post_id, base_dir).exists()
}

#[cfg(test)]
#[path = "../test/save.rs"]
mod tests;
