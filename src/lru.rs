//! A bounded least-recently-used map.
//!
//! [`LruCache`] is the storage engine behind [`CachedIndex`](crate::CachedIndex).
//! It is an arena-backed intrusive doubly-linked list paired with a hash map:
//! lookups, insertions, and recency updates are all amortized `O(1)`, and the
//! data structure never exceeds its configured capacity.
//!
//! ## Design
//!
//! Entries live in a flat `Vec<Slot<K, V>>` arena rather than in individually
//! boxed nodes, so traversal touches contiguous memory and there is no
//! per-entry allocation on the hot path. Each slot carries the index of its
//! predecessor and successor in recency order; a separate `HashMap<K, usize>`
//! maps keys to their slot index for `O(1)` lookup. The most-recently-used
//! entry sits at `head`, the least-recently-used at `tail`; eviction recycles
//! the tail slot in place, so a full cache performs zero allocation per insert
//! in steady state. The implementation contains no `unsafe`.

use std::collections::HashMap;
use std::hash::Hash;

/// Sentinel index marking "no neighbour" — the ends of the recency list and a
/// fresh slot's links. `usize::MAX` can never be a real slot index because the
/// arena is bounded by `capacity`, which is itself bounded by available memory.
const NIL: usize = usize::MAX;

/// One arena entry: the stored pair plus its recency-list neighbours.
struct Slot<K, V> {
    key: K,
    val: V,
    /// More-recently-used neighbour, or [`NIL`] at the head.
    prev: usize,
    /// Less-recently-used neighbour, or [`NIL`] at the tail.
    next: usize,
}

/// A fixed-capacity least-recently-used map.
///
/// Generic over the key `K` (`Hash + Eq + Clone`) and an arbitrary value `V`.
/// The key is stored twice — once in the lookup map and once in its slot — so
/// the tail can be evicted from the map without a reverse index; for the small
/// keys this crate caches against, that is a deliberate space-for-simplicity
/// trade with no `unsafe`.
pub(crate) struct LruCache<K, V> {
    /// Key to slot-index lookup table.
    map: HashMap<K, usize>,
    /// The slot arena. Grows to at most `capacity` entries, then recycles.
    slots: Vec<Slot<K, V>>,
    /// Index of the most-recently-used slot, or [`NIL`] when empty.
    head: usize,
    /// Index of the least-recently-used slot, or [`NIL`] when empty.
    tail: usize,
    /// Hard upper bound on the number of live entries.
    capacity: usize,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    /// Creates an empty cache that holds at most `capacity` entries.
    ///
    /// A `capacity` of `0` makes every [`put`](LruCache::put) a no-op that
    /// returns the rejected pair: a permanently empty, allocation-free cache.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            slots: Vec::with_capacity(capacity),
            head: NIL,
            tail: NIL,
            capacity,
        }
    }

    /// The number of entries currently stored.
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Unlinks `idx` from the recency list, repairing its neighbours' links.
    fn detach(&mut self, idx: usize) {
        let (prev, next) = {
            let slot = &self.slots[idx];
            (slot.prev, slot.next)
        };
        if prev != NIL {
            self.slots[prev].next = next;
        } else {
            self.head = next;
        }
        if next != NIL {
            self.slots[next].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    /// Links `idx` in at the head (most-recently-used) position. `idx` must
    /// already be detached.
    fn push_front(&mut self, idx: usize) {
        let old_head = self.head;
        {
            let slot = &mut self.slots[idx];
            slot.prev = NIL;
            slot.next = old_head;
        }
        if old_head != NIL {
            self.slots[old_head].prev = idx;
        } else {
            // The list was empty: this entry is both head and tail.
            self.tail = idx;
        }
        self.head = idx;
    }

    /// Looks up `key`, promoting it to most-recently-used on a hit.
    ///
    /// Returns `None` without touching recency on a miss.
    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        let idx = *self.map.get(key)?;
        self.detach(idx);
        self.push_front(idx);
        Some(&self.slots[idx].val)
    }

    /// Inserts or updates `key`, making it most-recently-used.
    ///
    /// Returns the evicted least-recently-used pair when a fresh insert
    /// overflows capacity, or `None` otherwise. When the capacity is `0` the
    /// pair is rejected immediately and returned unchanged.
    pub(crate) fn put(&mut self, key: K, val: V) -> Option<(K, V)> {
        if self.capacity == 0 {
            return Some((key, val));
        }
        if let Some(&idx) = self.map.get(&key) {
            // Key already present: replace the value and promote.
            self.slots[idx].val = val;
            self.detach(idx);
            self.push_front(idx);
            return None;
        }
        if self.map.len() == self.capacity {
            // Full: evict the tail and recycle its slot in place.
            let idx = self.tail;
            self.detach(idx);
            let evicted_key = std::mem::replace(&mut self.slots[idx].key, key.clone());
            let evicted_val = std::mem::replace(&mut self.slots[idx].val, val);
            let _removed = self.map.remove(&evicted_key);
            let _prev = self.map.insert(key, idx);
            self.push_front(idx);
            return Some((evicted_key, evicted_val));
        }
        // Room to grow: append a fresh slot.
        let idx = self.slots.len();
        self.slots.push(Slot {
            key: key.clone(),
            val,
            prev: NIL,
            next: NIL,
        });
        let _prev = self.map.insert(key, idx);
        self.push_front(idx);
        None
    }

    /// Removes every entry, keeping the allocated capacity for reuse.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.slots.clear();
        self.head = NIL;
        self.tail = NIL;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn get_miss_on_empty() {
        let mut cache: LruCache<u32, u32> = LruCache::with_capacity(4);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn put_then_get() {
        let mut cache = LruCache::with_capacity(2);
        assert_eq!(cache.put(1u32, 10u32), None);
        assert_eq!(cache.put(2, 20), None);
        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn put_updates_existing_without_growth() {
        let mut cache = LruCache::with_capacity(2);
        assert_eq!(cache.put(1u32, 10u32), None);
        assert_eq!(cache.put(1, 11), None);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&1), Some(&11));
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = LruCache::with_capacity(2);
        let _ = cache.put(1u32, 10u32);
        let _ = cache.put(2, 20);
        // Touch 1 so 2 becomes the LRU.
        assert_eq!(cache.get(&1), Some(&10));
        // Inserting 3 evicts 2.
        assert_eq!(cache.put(3, 30), Some((2, 20)));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(cache.get(&3), Some(&30));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn capacity_one_keeps_only_newest() {
        let mut cache = LruCache::with_capacity(1);
        let _ = cache.put(1u32, 10u32);
        assert_eq!(cache.put(2, 20), Some((1, 10)));
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
    }

    #[test]
    fn capacity_zero_rejects_everything() {
        let mut cache = LruCache::with_capacity(0);
        assert_eq!(cache.put(1u32, 10u32), Some((1, 10)));
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn clear_empties_and_keeps_working() {
        let mut cache = LruCache::with_capacity(2);
        let _ = cache.put(1u32, 10u32);
        let _ = cache.put(2, 20);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&1), None);
        // Still usable after a clear.
        let _ = cache.put(3, 30);
        assert_eq!(cache.get(&3), Some(&30));
    }

    #[test]
    fn never_exceeds_capacity_under_churn() {
        let mut cache = LruCache::with_capacity(8);
        for i in 0..1000u32 {
            let _ = cache.put(i, i);
            assert!(cache.len() <= 8);
        }
        // Only the last 8 keys survive.
        for i in 992..1000u32 {
            assert_eq!(cache.get(&i), Some(&i));
        }
        for i in 0..992u32 {
            assert_eq!(cache.get(&i), None);
        }
    }
}
