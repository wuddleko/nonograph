use super::*;

fn sample_post(id: &str) -> Arc<Post> {
    Arc::new(Post {
        id: id.to_string(),
        title: "Test Post".to_string(),
        author: "Test Author".to_string(),
        content: "<p>Test content</p>".to_string(),
        raw_content: "Test content".to_string(),
        created_at: Utc::now(),
    })
}

#[test]
fn lookup_copies_the_same_post() {
    let storage = PostCache::shared(128);
    let post = sample_post("test-post");
    storage
        .write()
        .unwrap()
        .insert("test-post".to_string(), Arc::clone(&post));

    let hit = storage
        .read()
        .unwrap()
        .lookup("test-post", false, "test-post")
        .unwrap();
    assert!(Arc::ptr_eq(&hit.post, &post));
    assert_eq!(hit.post.title, "Test Post");
    assert!(hit.html.is_none());
}

#[test]
fn finished_html_is_reused() {
    let storage = PostCache::shared(128);
    let post = sample_post("test-post");
    {
        let mut cache = storage.write().unwrap();
        cache.insert("test-post".to_string(), Arc::clone(&post));
        cache.remember_html("test-post", false, "abc", Arc::from("page"));
    }

    let hit = storage
        .read()
        .unwrap()
        .lookup("test-post", false, "abc")
        .unwrap();
    let again = storage
        .read()
        .unwrap()
        .lookup("test-post", false, "abc")
        .unwrap();
    assert_eq!(hit.html.as_deref(), Some("page"));
    assert!(Arc::ptr_eq(
        hit.html.as_ref().unwrap(),
        again.html.as_ref().unwrap()
    ));
    assert!(storage
        .read()
        .unwrap()
        .lookup("test-post", true, "abc")
        .unwrap()
        .html
        .is_none());
}

#[test]
fn read_updates_last_access_without_a_write_lock() {
    let storage = PostCache::shared(128);
    storage
        .write()
        .unwrap()
        .insert("test-post".to_string(), sample_post("test-post"));
    let hit = storage
        .read()
        .unwrap()
        .lookup("test-post", false, "test-post")
        .unwrap();
    let first = hit.accessed_at();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let _read = storage.read().unwrap();
    hit.note_access();
    assert!(hit.accessed_at() > first);
}

#[test]
fn a_full_cache_drops_the_oldest_post() {
    let storage = PostCache::shared(0);
    {
        let mut cache = storage.write().unwrap();
        cache.insert("older".to_string(), sample_post("older"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        cache.insert("newer".to_string(), sample_post("newer"));
    }
    let cache = storage.read().unwrap();
    assert!(!cache.contains_key("older"));
    assert!(cache.contains_key("newer"));
}

#[test]
fn purge_checks_files_after_releasing_the_lock() {
    let storage = PostCache::shared(128);
    storage
        .write()
        .unwrap()
        .insert("gone".to_string(), sample_post("gone"));
    let probe = Arc::clone(&storage);
    purge_if(&storage, |_| {
        let _guard = probe
            .try_write()
            .expect("file check must run after the cache lock is released");
        true
    });
    assert!(!storage.read().unwrap().contains_key("gone"));
}

#[test]
fn purge_keeps_a_post_whose_file_is_still_there() {
    let storage = PostCache::shared(128);
    storage
        .write()
        .unwrap()
        .insert("kept".to_string(), sample_post("kept"));
    purge_if(&storage, |_| false);
    assert!(storage.read().unwrap().contains_key("kept"));
}
