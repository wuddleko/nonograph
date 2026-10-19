use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::sync::Mutex;

use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::nostr::hex_encode;
use crate::relay_set::public_relays;

const FILE_TAG: &str = "nostorgraph-locator-file-v1";
const KEY_TAG: &str = "nostorgraph-locator-key-v1";
const CLAIM_TAG: &[u8] = b"nostorgraph-seal-claim-v1\0";
const HOLD_CLAIM_TAG: &[u8] = b"nostorgraph-hold-claim-v1\0";
const LOCK_FILE_TAG: &str = "nostorgraph-lock-file-v1";
const LOCK_KEY_TAG: &str = "nostorgraph-lock-key-v1";
const MAX_STORED_LINKS: usize = 4096;

type HmacSha256 = Hmac<Sha256>;

#[cfg(test)]
static LINK_CAP: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocatorRow {
    pub event_id: String,
    pub relays: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredSeal {
    event_id: String,
    relays: Vec<String>,
    /// Hash of the claim. A later copy of this row checks it.
    #[serde(default)]
    claim: String,
}

#[cfg(test)]
static TEST_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

#[cfg(test)]
pub(super) struct SealRowDirGuard;

#[cfg(test)]
impl SealRowDirGuard {
    pub(super) fn set(path: &Path) -> Self {
        *TEST_DIR.lock().expect("seal row dir") = Some(path.to_path_buf());
        Self
    }
}

#[cfg(test)]
impl Drop for SealRowDirGuard {
    fn drop(&mut self) {
        *TEST_DIR.lock().expect("seal row dir") = None;
    }
}

pub(super) fn content_root() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_DIR.lock().expect("seal row dir").clone() {
        return dir;
    }
    PathBuf::from("content")
}

fn row_dir() -> PathBuf {
    content_root().join("seal-rows")
}

pub(crate) fn storable(locator: &str, event_id: &str, relays: &[String]) -> bool {
    crate::nostr::sealed_locator(locator)
        && crate::nostr::sealed_event_id(event_id)
        && !public_relays(relays).is_empty()
}

pub(crate) fn claim_hex(secret: &[u8], locator: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(CLAIM_TAG);
    mac.update(locator.as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

pub(crate) enum LockPut {
    Created,
    Same,
    Rejected,
}

/// Records a hash of the claim for this locator. The event id and the relays are not stored.
pub(crate) fn lock_claim(locator: &str, claim: &str) -> LockPut {
    if !crate::nostr::sealed_locator(locator) {
        return LockPut::Rejected;
    }
    let Some(raw) = parse_claim(claim) else {
        return LockPut::Rejected;
    };
    let hash = hash_claim(&raw);
    match read_lock(locator) {
        Some(existing) if bytes_match(&existing, &hash) => return LockPut::Same,
        Some(_) => return LockPut::Rejected,
        None => {}
    }
    let Ok(plain) = serde_json::to_string(&StoredLock {
        claim: hex_encode(&hash),
    }) else {
        return LockPut::Rejected;
    };
    let key = tagged_hash(LOCK_KEY_TAG, locator);
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    let Ok(payload) = nostorgraph_nip44::encrypt(&plain, &key, &nonce) else {
        return LockPut::Rejected;
    };
    let dir = lock_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return LockPut::Rejected;
    }
    let name = hex_encode(&tagged_hash(LOCK_FILE_TAG, locator));
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.{}.writing", hex_encode(&nonce)));
    if std::fs::write(&tmp, payload.as_bytes()).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return LockPut::Rejected;
    }
    match std::fs::hard_link(&tmp, &path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            LockPut::Created
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&tmp);
            match read_lock(locator) {
                Some(existing) if bytes_match(&existing, &hash) => LockPut::Same,
                _ => LockPut::Rejected,
            }
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            LockPut::Rejected
        }
    }
}

pub(crate) fn release_lock(locator: &str) {
    if !crate::nostr::sealed_locator(locator) {
        return;
    }
    let _ = std::fs::remove_file(lock_dir().join(lock_file_name(locator)));
}

#[cfg(test)]
pub(super) struct LinkCapGuard;

#[cfg(test)]
impl LinkCapGuard {
    pub(super) fn set(cap: usize) -> Self {
        LINK_CAP.store(cap, Ordering::Relaxed);
        Self
    }
}

#[cfg(test)]
impl Drop for LinkCapGuard {
    fn drop(&mut self) {
        LINK_CAP.store(0, Ordering::Relaxed);
    }
}

pub(super) fn dir_has_room(path: &Path, cap: usize) -> bool {
    let Ok(dir) = std::fs::read_dir(path) else {
        return true;
    };
    let stored = dir
        .filter_map(|entry| entry.ok())
        .filter(|entry| !entry.file_name().to_string_lossy().ends_with(".writing"))
        .count();
    stored < cap
}

pub(super) fn stored_cap() -> usize {
    #[cfg(test)]
    {
        let cap = LINK_CAP.load(Ordering::Relaxed);
        if cap > 0 {
            return cap;
        }
    }
    MAX_STORED_LINKS
}

pub(crate) fn remember(
    locator: &str,
    event_id: &str,
    relays: &[String],
    claim: &str,
) -> Result<(), ()> {
    let claim_hash = hash_claim(&parse_claim(claim).ok_or(())?);
    let relays_now = public_relays(relays);
    if !crate::nostr::sealed_locator(locator)
        || !crate::nostr::sealed_event_id(event_id)
        || relays_now.is_empty()
    {
        return Err(());
    }
    // A later copy must not point that row at another note. The saved row is the record.
    if let Some(existing) = read_seal(locator) {
        if !seal_matches(&existing, event_id, &relays_now, claim_hash) {
            return Err(());
        }
        return Ok(());
    }
    if !lock_matches(locator, claim) {
        return Err(());
    }
    if !dir_has_room(&row_dir(), stored_cap()) {
        return Err(());
    }
    let plain = serde_json::to_string(&StoredSeal {
        event_id: event_id.to_string(),
        relays: relays_now.clone(),
        claim: hex_encode(&claim_hash),
    })
    .map_err(|_| ())?;
    let key = tagged_hash(KEY_TAG, locator);
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    let payload = nostorgraph_nip44::encrypt(&plain, &key, &nonce).map_err(|_| ())?;
    let dir = row_dir();
    std::fs::create_dir_all(&dir).map_err(|_| ())?;
    let name = hex_encode(&tagged_hash(FILE_TAG, locator));
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.{}.writing", hex_encode(&nonce)));
    if std::fs::write(&tmp, payload.as_bytes()).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(());
    }
    match std::fs::hard_link(&tmp, &path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            release_lock(locator);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&tmp);
            match read_seal(locator) {
                Some(existing) if seal_matches(&existing, event_id, &relays_now, claim_hash) => {
                    release_lock(locator);
                    Ok(())
                }
                _ => Err(()),
            }
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            Err(())
        }
    }
}

pub(crate) fn read(locator: &str) -> Option<LocatorRow> {
    let stored = read_seal(locator)?;
    Some(LocatorRow {
        event_id: stored.event_id,
        relays: stored.relays,
    })
}

fn read_seal(locator: &str) -> Option<StoredSeal> {
    if !crate::nostr::sealed_locator(locator) {
        return None;
    }
    let name = hex_encode(&tagged_hash(FILE_TAG, locator));
    let bytes = std::fs::read(row_dir().join(name)).ok()?;
    let payload = std::str::from_utf8(&bytes).ok()?.trim();
    let key = tagged_hash(KEY_TAG, locator);
    let plain = nostorgraph_nip44::decrypt(payload, &key).ok()?;
    let mut stored: StoredSeal = serde_json::from_str(&plain).ok()?;
    if !crate::nostr::sealed_event_id(&stored.event_id) {
        return None;
    }
    stored.relays = public_relays(&stored.relays);
    if stored.relays.is_empty() {
        return None;
    }
    Some(stored)
}

fn seal_matches(
    stored: &StoredSeal,
    event_id: &str,
    relays: &[String],
    claim_hash: [u8; 32],
) -> bool {
    stored.event_id == event_id
        && stored.relays == relays
        && stored_claim_matches(&stored.claim, claim_hash)
}

fn stored_claim_matches(stored: &str, claim_hash: [u8; 32]) -> bool {
    let Some(have) = parse_claim(stored) else {
        return false;
    };
    bytes_match(&have, &claim_hash)
}

pub(super) fn claim_matches(locator: &str, event_id: &str, relays: &[String], claim: &str) -> bool {
    let Some(raw) = parse_claim(claim) else {
        return false;
    };
    let relays_now = public_relays(relays);
    if relays_now.is_empty()
        || !crate::nostr::sealed_locator(locator)
        || !crate::nostr::sealed_event_id(event_id)
    {
        return false;
    }
    if let Some(existing) = read_seal(locator) {
        return seal_matches(&existing, event_id, &relays_now, hash_claim(&raw));
    }
    lock_matches(locator, claim)
}

fn parse_claim(claim: &str) -> Option<[u8; 32]> {
    crate::nostr::decode_fixed_hex(claim)
}

fn tagged_hash(tag: &str, locator: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(tag.as_bytes());
    hasher.update([0]);
    hasher.update(locator.as_bytes());
    hasher.finalize().into()
}

#[derive(Serialize, Deserialize)]
struct StoredLock {
    claim: String,
}

fn lock_dir() -> PathBuf {
    content_root().join("seal-locks")
}

fn lock_file_name(locator: &str) -> String {
    hex_encode(&tagged_hash(LOCK_FILE_TAG, locator))
}

fn read_lock(locator: &str) -> Option<[u8; 32]> {
    let bytes = std::fs::read(lock_dir().join(lock_file_name(locator))).ok()?;
    let payload = std::str::from_utf8(&bytes).ok()?.trim();
    let key = tagged_hash(LOCK_KEY_TAG, locator);
    let plain = nostorgraph_nip44::decrypt(payload, &key).ok()?;
    let stored: StoredLock = serde_json::from_str(&plain).ok()?;
    parse_claim(&stored.claim)
}

fn lock_matches(locator: &str, claim: &str) -> bool {
    let Some(raw) = parse_claim(claim) else {
        return false;
    };
    let Some(have) = read_lock(locator) else {
        return false;
    };
    bytes_match(&have, &hash_claim(&raw))
}

fn hash_claim(claim: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(HOLD_CLAIM_TAG);
    hasher.update(claim);
    hasher.finalize().into()
}

fn bytes_match(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (one, other) in left.iter().zip(right.iter()) {
        diff |= one ^ other;
    }
    diff == 0
}
