use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

// Public posts only. A link with ?nsec= is still this path: the site decrypts,
// then the plaintext post is stored here. A private note will not use this cache.
const MAX_CACHED_PAGES: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub author: String,
    pub content: String,
    pub raw_content: String,
    pub created_at: DateTime<Utc>,
}

impl Post {
    fn memory_size(&self) -> usize {
        self.id.len()
            + self.title.len()
            + self.author.len()
            + self.content.len()
            + self.raw_content.len()
            + 64
    }
}

#[derive(Debug)]
struct CachedPage {
    nojs: bool,
    public_id: String,
    html: Arc<str>,
}

#[derive(Debug)]
struct CacheEntry {
    post: Arc<Post>,
    pages: Vec<CachedPage>,
    last_accessed: Arc<AtomicU64>,
}

impl CacheEntry {
    fn stored_size(&self) -> usize {
        self.post.memory_size()
            + self
                .pages
                .iter()
                .map(|page| page.public_id.len() + page.html.len())
                .sum::<usize>()
    }
}

pub struct CacheHit {
    pub post: Arc<Post>,
    pub html: Option<Arc<str>>,
    last_accessed: Arc<AtomicU64>,
}

impl CacheHit {
    pub fn note_access(&self) {
        self.last_accessed.store(now_millis(), Ordering::Relaxed);
    }

    #[cfg(test)]
    fn accessed_at(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct PostCache {
    entries: HashMap<String, CacheEntry>,
    total_size: usize,
    max_size: usize,
}

pub type PostStorage = Arc<RwLock<PostCache>>;

impl PostCache {
    pub fn new(max_size_mb: usize) -> Self {
        PostCache {
            entries: HashMap::new(),
            total_size: 0,
            max_size: max_size_mb * 1024 * 1024,
        }
    }

    pub fn shared(max_size_mb: usize) -> PostStorage {
        Arc::new(RwLock::new(PostCache::new(max_size_mb)))
    }

    pub fn contains_key(&self, post_id: &str) -> bool {
        self.entries.contains_key(post_id)
    }

    /// Copy the post (and finished HTML, when this view was built before).
    /// The caller records the access after dropping the lock.
    pub fn lookup(&self, post_id: &str, nojs: bool, public_id: &str) -> Option<CacheHit> {
        let entry = self.entries.get(post_id)?;
        let html = entry
            .pages
            .iter()
            .find(|page| page.nojs == nojs && page.public_id == public_id)
            .map(|page| Arc::clone(&page.html));
        Some(CacheHit {
            post: Arc::clone(&entry.post),
            html,
            last_accessed: Arc::clone(&entry.last_accessed),
        })
    }

    pub fn insert(&mut self, post_id: String, post: Arc<Post>) {
        if let Some(old_entry) = self.entries.remove(&post_id) {
            self.total_size -= old_entry.stored_size();
        }

        self.total_size += post.memory_size();
        self.evict_until_within_limit(None);

        self.entries.insert(
            post_id,
            CacheEntry {
                post,
                pages: Vec::new(),
                last_accessed: Arc::new(AtomicU64::new(now_millis())),
            },
        );
    }

    pub fn remember_html(&mut self, post_id: &str, nojs: bool, public_id: &str, html: Arc<str>) {
        let Some(entry) = self.entries.get_mut(post_id) else {
            return;
        };
        let before = entry.stored_size();
        if let Some(page) = entry
            .pages
            .iter_mut()
            .find(|page| page.nojs == nojs && page.public_id == public_id)
        {
            page.html = html;
        } else {
            if entry.pages.len() == MAX_CACHED_PAGES {
                entry.pages.remove(0);
            }
            entry.pages.push(CachedPage {
                nojs,
                public_id: public_id.to_string(),
                html,
            });
        }
        let after = entry.stored_size();
        self.total_size = self.total_size - before + after;
        self.evict_until_within_limit(Some(post_id));
    }

    fn post_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    fn evict_until_within_limit(&mut self, keep: Option<&str>) {
        while self.total_size > self.max_size {
            let oldest = self
                .entries
                .iter()
                .filter(|(id, _)| keep != Some(id.as_str()))
                .min_by_key(|(_, entry)| entry.last_accessed.load(Ordering::Relaxed))
                .map(|(id, _)| id.clone());
            let Some(oldest) = oldest else {
                break;
            };
            self.evict_one(&oldest);
        }
    }

    fn evict_one(&mut self, id: &str) {
        if let Some(entry) = self.entries.remove(id) {
            let freed = entry.stored_size();
            self.total_size -= freed;
            println!(
                "Nonograph: Cache EVICT for post: {id} (freed: {} KB)",
                freed / 1024
            );
        }
    }

    fn remove_ids(&mut self, ids: &[String]) {
        for id in ids {
            if let Some(entry) = self.entries.remove(id) {
                self.total_size -= entry.stored_size();
                println!("Nonograph: Cache EVICT for post: {id}.md");
            }
        }
    }
}

pub fn purge_missing(storage: &PostStorage) {
    purge_if(storage, |id| {
        !std::path::Path::new(&format!("content/{id}.md")).exists()
    });
}

fn purge_if(storage: &PostStorage, missing: impl Fn(&str) -> bool) {
    let ids = {
        let cache = storage.read().unwrap();
        cache.post_ids()
    };
    let stale: Vec<String> = ids.into_iter().filter(|id| missing(id)).collect();
    if !stale.is_empty() {
        storage.write().unwrap().remove_ids(&stale);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../test/cache.rs"]
mod tests;
