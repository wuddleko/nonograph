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
    let content_dir = Path::new(base_dir).join("content");
    if !content_dir.exists() {
        fs::create_dir_all(&content_dir)
            .map_err(|e| SaveError::Io(format!("Failed to create content directory: {}", e)))?;
    }

    let filename = format!("{}.md", post.id);
    let file_path = content_dir.join(filename);

    let mut frontmatter = String::from("---\n");
    frontmatter.push_str(&format!("title: {}\n", post.title));
    frontmatter.push_str(&format!("date: {}\n", post.created_at.format("%Y-%m-%d")));
    if !post.author.is_empty() {
        frontmatter.push_str(&format!("author: {}\n", post.author));
    }
    frontmatter.push_str(&format!(
        "generator: {} v{}\n",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    ));
    frontmatter.push_str("---\n\n");

    let file_content = format!("{}{}", frontmatter, post.raw_content);

    // create_new refuses when content/{id}.md is already there, so a second
    // save of one id cannot replace the first body.
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path)
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
    if let Err(e) = file.write_all(file_content.as_bytes()) {
        let _ = fs::remove_file(&file_path);
        return Err(SaveError::Io(format!(
            "Failed to write post to file {:?}: {}",
            file_path, e
        )));
    }

    Ok(())
}

pub fn post_file_exists(post_id: &str) -> bool {
    post_file_exists_in_dir(post_id, ".")
}

pub fn post_file_exists_in_dir(post_id: &str, base_dir: &str) -> bool {
    let filename = format!("{}.md", post_id);
    Path::new(base_dir).join("content").join(filename).exists()
}

#[cfg(test)]
#[path = "../test/save.rs"]
mod tests;
