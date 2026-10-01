use super::*;
use chrono::{Datelike, Utc};
use serial_test::serial;
use std::path::PathBuf;
use tempfile::tempdir;

fn setup_test_env() -> (tempfile::TempDir, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let content_dir = temp_dir.path().join("content");
    std::fs::create_dir_all(&content_dir).unwrap();
    (temp_dir, content_dir)
}

#[test]
fn test_ensure_content_directory() {
    let (_temp_dir, content_dir) = setup_test_env();

    // Test that our setup worked - content directory should exist
    assert!(content_dir.exists());
    assert!(content_dir.is_dir());
}

#[test]
#[serial]
fn test_save_and_load_post() {
    let (temp_dir, _content_dir) = setup_test_env();

    let post = Post {
        id: "test-post-01-01-2024".to_string(),
        title: "Test Post".to_string(),
        author: "Test Author".to_string(),
        content: "<p>Rendered content</p>".to_string(),
        raw_content: "Raw content here".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    let temp_path = temp_dir.path().to_str().unwrap();

    // Save post
    assert!(save_post_to_file_in_dir(&post, temp_path).is_ok());

    // Check file exists
    assert!(post_file_exists_in_dir("test-post-01-01-2024", temp_path));
}

#[test]
#[serial]
fn test_load_nonexistent_post() {
    let (temp_dir, _content_dir) = setup_test_env();

    let temp_path = temp_dir.path().to_str().unwrap();

    // Test that a non-existent post doesn't exist
    assert!(!post_file_exists_in_dir("nonexistent-post", temp_path));
}

#[test]
#[serial]
fn test_delete_post_file() {
    let (temp_dir, _content_dir) = setup_test_env();

    let temp_path = temp_dir.path().to_str().unwrap();

    let post = Post {
        id: "delete-test-01-01-2024".to_string(),
        title: "Delete Test".to_string(),
        author: "Test Author".to_string(),
        content: "<p>Content</p>".to_string(),
        raw_content: "Content".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    // Save and verify exists
    save_post_to_file_in_dir(&post, temp_path).unwrap();
    assert!(post_file_exists_in_dir("delete-test-01-01-2024", temp_path));

    // Delete the file
    let file_path = temp_dir
        .path()
        .join("content")
        .join("delete-test-01-01-2024.md");
    fs::remove_file(file_path).unwrap();
    assert!(!post_file_exists_in_dir(
        "delete-test-01-01-2024",
        temp_path
    ));
}

#[test]
#[serial]
fn test_file_format() {
    let (temp_dir, _content_dir) = setup_test_env();

    let temp_path = temp_dir.path().to_str().unwrap();

    let post = Post {
        id: "format-test-01-01-2024".to_string(),
        title: "Format Test".to_string(),
        author: "Test Author".to_string(),
        content: "<p>Rendered</p>".to_string(),
        raw_content: "This is the user content\nWith multiple lines".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    assert!(save_post_to_file_in_dir(&post, temp_path).is_ok());

    // Read raw file content to verify format
    let file_path = temp_dir
        .path()
        .join("content")
        .join("format-test-01-01-2024.md");
    let raw_file = fs::read_to_string(file_path).unwrap();
    let lines: Vec<&str> = raw_file.lines().collect();

    // YAML frontmatter format
    assert_eq!(lines[0], "---");
    assert_eq!(lines[1], "title: Format Test");
    let current_year = post.created_at.year();
    assert!(lines[2].starts_with("date: "));
    assert!(lines[2].contains(&current_year.to_string()));
    assert_eq!(lines[3], "author: Test Author");
    assert_eq!(
        lines[4],
        format!(
            "generator: {} v{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        )
    );
    assert_eq!(lines[5], "---");
    assert_eq!(lines[6], "");
    assert_eq!(lines[7], "This is the user content");
    assert_eq!(lines[8], "With multiple lines");
}

#[test]
#[serial]
fn test_file_format_no_author() {
    let (temp_dir, _content_dir) = setup_test_env();

    let temp_path = temp_dir.path().to_str().unwrap();

    let post = Post {
        id: "no-author-test-01-01-2024".to_string(),
        title: "No Author Test".to_string(),
        author: "".to_string(),
        content: "<p>Rendered</p>".to_string(),
        raw_content: "Content without author".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };

    assert!(save_post_to_file_in_dir(&post, temp_path).is_ok());

    // Read raw file content to verify format
    let file_path = temp_dir
        .path()
        .join("content")
        .join("no-author-test-01-01-2024.md");
    let raw_file = fs::read_to_string(file_path).unwrap();
    let lines: Vec<&str> = raw_file.lines().collect();

    assert_eq!(lines[0], "---");
    assert_eq!(lines[1], "title: No Author Test");
    let current_year = post.created_at.year();
    assert!(lines[2].starts_with("date: "));
    assert!(lines[2].contains(&current_year.to_string()));
    assert_eq!(
        lines[3],
        format!(
            "generator: {} v{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        )
    );
    assert_eq!(lines[4], "---");
    assert_eq!(lines[5], "");
    assert_eq!(lines[6], "Content without author");
}

#[test]
#[serial]
fn test_file_format_with_nostr_id() {
    let (temp_dir, _content_dir) = setup_test_env();
    let temp_path = temp_dir.path().to_str().unwrap();
    let nostr = "nevent1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
    let post = Post {
        id: "nostr-test-01-01-2024".to_string(),
        title: "Nostr Test".to_string(),
        author: "Ada".to_string(),
        content: "<p>hi</p>".to_string(),
        raw_content: "hi".to_string(),
        created_at: Utc::now(),
        nostr_id: Some(nostr.to_string()),
    };
    save_post_to_file_in_dir(&post, temp_path).unwrap();
    let raw_file = fs::read_to_string(
        temp_dir
            .path()
            .join("content")
            .join("nostr-test-01-01-2024.md"),
    )
    .unwrap();
    assert!(raw_file.contains(&format!("nostr: {nostr}")));
}

#[test]
#[serial]
fn test_alias_pointer_file() {
    let (temp_dir, _content_dir) = setup_test_env();
    let temp_path = temp_dir.path().to_str().unwrap();
    let cache_id = "ab".repeat(32);
    save_alias_pointer_in_dir(&cache_id, "hello-short-01-01-2024", temp_path).unwrap();
    let raw_file = fs::read_to_string(
        temp_dir
            .path()
            .join("content")
            .join(format!("{cache_id}.md")),
    )
    .unwrap();
    assert_eq!(raw_file, "---\nalias: hello-short-01-01-2024\n---\n");
    assert!(save_alias_pointer_in_dir(&cache_id, "other-short-01-01-2024", temp_path).is_err());
}

#[test]
#[serial]
fn test_read_post_file_follows_alias() {
    let (temp_dir, _content_dir) = setup_test_env();
    let temp_path = temp_dir.path().to_str().unwrap();
    let post = Post {
        id: "hello-short-01-01-2024".to_string(),
        title: "Hello".to_string(),
        author: "Ada".to_string(),
        content: "<p>hi</p>".to_string(),
        raw_content: "the article".to_string(),
        created_at: Utc::now(),
        nostr_id: None,
    };
    save_post_to_file_in_dir(&post, temp_path).unwrap();
    let cache_id = "ab".repeat(32);
    save_alias_pointer_in_dir(&cache_id, &post.id, temp_path).unwrap();
    let by_short = read_post_file_in_dir(&post.id, temp_path).unwrap();
    let by_alias = read_post_file_in_dir(&cache_id, temp_path).unwrap();
    assert_eq!(by_short, by_alias);
    assert!(by_alias.contains("the article"));
    assert!(!by_alias.contains("alias:"));
    assert!(remove_post_file_in_dir(&post.id, temp_path));
    assert!(read_post_file_in_dir(&cache_id, temp_path).is_none());
    assert!(!post_file_is_live_in_dir(&cache_id, temp_path));
}

#[test]
#[serial]
fn test_replace_alias_pointer_file() {
    let (temp_dir, _content_dir) = setup_test_env();
    let temp_path = temp_dir.path().to_str().unwrap();
    let cache_id = "ab".repeat(32);
    save_alias_pointer_in_dir(&cache_id, "hello-short-01-01-2024", temp_path).unwrap();
    write_alias_pointer(&cache_id, "other-short-01-01-2024", temp_path, true).unwrap();
    let raw_file = fs::read_to_string(
        temp_dir
            .path()
            .join("content")
            .join(format!("{cache_id}.md")),
    )
    .unwrap();
    assert_eq!(raw_file, "---\nalias: other-short-01-01-2024\n---\n");
}

#[test]
#[serial]
fn test_dangling_alias_is_not_live() {
    let (temp_dir, _content_dir) = setup_test_env();
    let temp_path = temp_dir.path().to_str().unwrap();
    let cache_id = "ab".repeat(32);
    save_alias_pointer_in_dir(&cache_id, "hello-short-01-01-2024", temp_path).unwrap();
    assert!(post_file_exists_in_dir(&cache_id, temp_path));
    assert!(!post_file_is_live_in_dir(&cache_id, temp_path));
}
