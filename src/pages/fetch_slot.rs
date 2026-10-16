use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const MAX_PUBLIC_FETCHES: usize = 8;
pub(crate) const MAX_PUBLIC_PUBLISHES: usize = 8;
static PUBLIC_FETCHES: AtomicUsize = AtomicUsize::new(0);
static PUBLIC_PUBLISHES: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct PublicFetchSlot {
    _private: (),
}

pub(crate) struct PublicPublishSlot {
    _private: (),
}

impl PublicFetchSlot {
    pub(crate) fn acquire() -> Option<Self> {
        if try_acquire_public_fetch(&PUBLIC_FETCHES, MAX_PUBLIC_FETCHES) {
            Some(Self { _private: () })
        } else {
            None
        }
    }
}

impl PublicPublishSlot {
    pub(crate) fn acquire() -> Option<Self> {
        if try_acquire_public_fetch(&PUBLIC_PUBLISHES, MAX_PUBLIC_PUBLISHES) {
            Some(Self { _private: () })
        } else {
            None
        }
    }
}

impl Drop for PublicFetchSlot {
    fn drop(&mut self) {
        release_public_fetch(&PUBLIC_FETCHES);
    }
}

impl Drop for PublicPublishSlot {
    fn drop(&mut self) {
        release_public_fetch(&PUBLIC_PUBLISHES);
    }
}

pub(crate) fn try_acquire_public_fetch(counter: &AtomicUsize, max: usize) -> bool {
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current >= max {
            return false;
        }
        if counter
            .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
    }
}

pub(crate) fn release_public_fetch(counter: &AtomicUsize) {
    counter.fetch_sub(1, Ordering::Release);
}
