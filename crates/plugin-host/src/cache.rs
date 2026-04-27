//! Predicate dispatch cache (Phase F.3).
//!
//! Custom-predicate eval is a pure function of `(item_canonical_hash,
//! plugin_id, predicate_name, args_hash) -> bool`. Caching the result
//! lets the planner re-evaluate a predicate against the same item
//! multiple times during one beam-search expansion without paying the
//! wasm-call cost twice.
//!
//! The cache is a tiny LRU keyed by a 64-bit composite hash. Capacity
//! is configurable; default 4096 entries (~32KB) covers a depth-3
//! beam search with 5 plugins comfortably.

use std::collections::VecDeque;
use std::sync::Mutex;

use ahash::AHashMap;

/// Composite key for a cache entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PredicateKey(u64);

impl PredicateKey {
    fn from_parts(item_hash: u64, plugin_id: &str, name: &str, args_hash: u64) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        item_hash.hash(&mut h);
        plugin_id.hash(&mut h);
        name.hash(&mut h);
        args_hash.hash(&mut h);
        Self(h.finish())
    }
}

pub struct PredicateCache {
    inner: Mutex<CacheInner>,
    capacity: usize,
}

struct CacheInner {
    map: AHashMap<PredicateKey, bool>,
    order: VecDeque<PredicateKey>,
}

impl PredicateCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                map: AHashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
            }),
            capacity,
        }
    }

    /// Look up an entry. Returns `None` on cache miss.
    pub fn get(&self, item_hash: u64, plugin_id: &str, name: &str, args_hash: u64) -> Option<bool> {
        let key = PredicateKey::from_parts(item_hash, plugin_id, name, args_hash);
        let inner = self.inner.lock().ok()?;
        inner.map.get(&key).copied()
    }

    /// Insert an entry, evicting the LRU entry if at capacity.
    pub fn insert(&self, item_hash: u64, plugin_id: &str, name: &str, args_hash: u64, value: bool) {
        let key = PredicateKey::from_parts(item_hash, plugin_id, name, args_hash);
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.map.len() >= self.capacity {
            if let Some(victim) = inner.order.pop_front() {
                inner.map.remove(&victim);
            }
        }
        if inner.map.insert(key, value).is_none() {
            inner.order.push_back(key);
        }
    }

    /// Capacity (max entries before eviction).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Approximate size (locked count).
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |i| i.map.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_insert_get() {
        let c = PredicateCache::new(4);
        c.insert(1, "p", "n", 5, true);
        assert_eq!(c.get(1, "p", "n", 5), Some(true));
        assert_eq!(c.get(1, "p", "n", 6), None);
    }

    #[test]
    fn evicts_lru_at_capacity() {
        let c = PredicateCache::new(2);
        c.insert(1, "p", "n", 1, true);
        c.insert(2, "p", "n", 2, true);
        assert_eq!(c.len(), 2);
        c.insert(3, "p", "n", 3, true);
        assert_eq!(c.len(), 2);
        // First inserted is evicted.
        assert_eq!(c.get(1, "p", "n", 1), None);
        assert_eq!(c.get(2, "p", "n", 2), Some(true));
        assert_eq!(c.get(3, "p", "n", 3), Some(true));
    }
}
