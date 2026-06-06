//! The eviction policies, and the [`PolicyCache`] that dispatches over them.
//!
//! Each policy is a bounded key→value cache with the same small interface —
//! [`get`](PolicyCache::get), [`put`](PolicyCache::put),
//! [`clear`](PolicyCache::clear), [`len`](PolicyCache::len) — built on the
//! [`OrderedMap`](crate::ordered::OrderedMap) primitive. They differ only in
//! which entry [`put`] discards when the cache is full:
//!
//! - **LRU / FIFO** ([`RecencyCache`]) share one recency-ordered list; LRU
//!   promotes on access, FIFO does not.
//! - **LFU** ([`LfuCache`]) groups keys into frequency buckets and evicts from
//!   the least-frequent one, breaking ties by least-recently-used.
//! - **ARC** ([`ArcCache`]) keeps recency and frequency lists plus ghost lists
//!   and adapts the balance between them.
//!
//! `put` returns `true` when it discarded a previously cached value, so the
//! wrapper can count evictions.

use std::cmp::{max, min};
use std::collections::HashMap;
use std::hash::Hash;

use crate::config::EvictionPolicy;
use crate::ordered::OrderedMap;

/// A bounded recency cache covering both LRU and FIFO.
///
/// The two policies are identical except for one bit: LRU promotes an entry to
/// most-recently-used on a [`get`](Self::get), FIFO leaves order untouched, so
/// FIFO evicts strictly in insertion order.
pub(crate) struct RecencyCache<K, V> {
    map: OrderedMap<K, V>,
    capacity: usize,
    /// Whether a `get` promotes the entry (LRU) or not (FIFO).
    touch_on_get: bool,
}

impl<K: Hash + Eq + Clone, V> RecencyCache<K, V> {
    fn new(capacity: usize, touch_on_get: bool) -> Self {
        Self {
            map: OrderedMap::with_capacity(capacity),
            capacity,
            touch_on_get,
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        // One hash probe: promote on access for LRU, leave order alone for FIFO.
        self.map.access(key, self.touch_on_get)
    }

    fn put(&mut self, key: K, val: V) -> bool {
        if self.capacity == 0 {
            return false;
        }
        if self.map.contains(&key) {
            self.map.update_value(&key, val);
            if self.touch_on_get {
                self.map.move_to_front(&key);
            }
            return false;
        }
        self.map.insert_front(key, val);
        if self.map.len() > self.capacity {
            return self.map.pop_back().is_some();
        }
        false
    }

    fn clear(&mut self) {
        self.map.clear();
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}

/// A least-frequently-used cache.
///
/// Every key carries a hit count. Keys with the same count live in one
/// recency-ordered bucket, so the eviction victim is the least-recently-used
/// key in the least-frequent bucket. `min_freq` tracks the lowest occupied
/// count so eviction is `O(1)`.
pub(crate) struct LfuCache<K, V> {
    /// Value and current frequency per key.
    values: HashMap<K, (V, u64)>,
    /// Keys grouped by frequency, each ordered most-recently-used first.
    buckets: HashMap<u64, OrderedMap<K, ()>>,
    /// The lowest frequency currently occupied.
    min_freq: u64,
    capacity: usize,
}

impl<K: Hash + Eq + Clone, V> LfuCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            values: HashMap::with_capacity(capacity),
            buckets: HashMap::new(),
            min_freq: 0,
            capacity,
        }
    }

    /// Promotes `key` from frequency `f` to `f + 1`, maintaining `min_freq`.
    fn bump(&mut self, key: &K) {
        let freq = match self.values.get(key) {
            Some((_, freq)) => *freq,
            None => return,
        };
        if let Some(entry) = self.values.get_mut(key) {
            entry.1 = freq + 1;
        }
        if let Some(bucket) = self.buckets.get_mut(&freq) {
            let _removed = bucket.remove(key);
            if bucket.len() == 0 {
                let _empty = self.buckets.remove(&freq);
                if freq == self.min_freq {
                    self.min_freq = freq + 1;
                }
            }
        }
        self.buckets
            .entry(freq + 1)
            .or_insert_with(|| OrderedMap::with_capacity(1))
            .insert_front(key.clone(), ());
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if !self.values.contains_key(key) {
            return None;
        }
        self.bump(key);
        self.values.get(key).map(|(val, _)| val)
    }

    fn put(&mut self, key: K, val: V) -> bool {
        if self.capacity == 0 {
            return false;
        }
        if self.values.contains_key(&key) {
            if let Some(entry) = self.values.get_mut(&key) {
                entry.0 = val;
            }
            self.bump(&key);
            return false;
        }

        let mut evicted = false;
        if self.values.len() == self.capacity {
            if let Some((victim, _)) = self
                .buckets
                .get_mut(&self.min_freq)
                .and_then(OrderedMap::pop_back)
            {
                let _removed = self.values.remove(&victim);
                evicted = true;
                if self
                    .buckets
                    .get(&self.min_freq)
                    .is_some_and(|b| b.len() == 0)
                {
                    let _empty = self.buckets.remove(&self.min_freq);
                }
            }
        }

        let _prev = self.values.insert(key.clone(), (val, 1));
        self.buckets
            .entry(1)
            .or_insert_with(|| OrderedMap::with_capacity(1))
            .insert_front(key, ());
        self.min_freq = 1;
        evicted
    }

    fn clear(&mut self) {
        self.values.clear();
        self.buckets.clear();
        self.min_freq = 0;
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

/// An Adaptive Replacement Cache (Megiddo & Modha).
///
/// Two value lists hold live entries — `t1` for keys seen once (recency) and
/// `t2` for keys seen again (frequency) — and two ghost lists, `b1` and `b2`,
/// remember keys recently evicted from each. A ghost hit nudges the target size
/// `p` toward the list that would have kept the key, so the cache shifts itself
/// between LRU- and LFU-like behavior as the workload changes. Occupancy
/// (`|t1| + |t2|`) never exceeds the capacity `c`.
pub(crate) struct ArcCache<K, V> {
    /// Recency list: keys seen once.
    t1: OrderedMap<K, V>,
    /// Frequency list: keys seen at least twice.
    t2: OrderedMap<K, V>,
    /// Ghosts evicted from `t1`.
    b1: OrderedMap<K, ()>,
    /// Ghosts evicted from `t2`.
    b2: OrderedMap<K, ()>,
    /// Adaptive target size for `t1`, in `0..=c`.
    p: usize,
    /// Capacity `c`.
    capacity: usize,
}

impl<K: Hash + Eq + Clone, V> ArcCache<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            t1: OrderedMap::with_capacity(capacity),
            t2: OrderedMap::with_capacity(capacity),
            b1: OrderedMap::with_capacity(capacity),
            b2: OrderedMap::with_capacity(capacity),
            p: 0,
            capacity,
        }
    }

    /// Demotes one live entry to a ghost list to free a slot. Returns `true`
    /// (an entry was evicted from the cache proper). `x_in_b2` is whether the
    /// key currently being admitted came from `b2`.
    fn replace(&mut self, x_in_b2: bool) -> bool {
        if self.t1.len() >= 1 && (self.t1.len() > self.p || (x_in_b2 && self.t1.len() == self.p)) {
            if let Some((key, _val)) = self.t1.pop_back() {
                self.b1.insert_front(key, ());
                return true;
            }
        } else if let Some((key, _val)) = self.t2.pop_back() {
            self.b2.insert_front(key, ());
            return true;
        }
        false
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        // Seen a second time: promote from recency (t1) to frequency (t2).
        if let Some(val) = self.t1.remove(key) {
            self.t2.insert_front(key.clone(), val);
            return self.t2.get(key);
        }
        // Already frequent: refresh its position in t2 (single hash probe).
        self.t2.access(key, true)
    }

    fn put(&mut self, key: K, val: V) -> bool {
        if self.capacity == 0 {
            return false;
        }
        let c = self.capacity;

        if self.b1.contains(&key) {
            // Ghost hit in b1: lean toward recency.
            let delta = max(self.b2.len() / max(self.b1.len(), 1), 1);
            self.p = min(self.p + delta, c);
            let evicted = self.replace(false);
            let _ghost = self.b1.remove(&key);
            self.t2.insert_front(key, val);
            return evicted;
        }
        if self.b2.contains(&key) {
            // Ghost hit in b2: lean toward frequency.
            let delta = max(self.b1.len() / max(self.b2.len(), 1), 1);
            self.p = self.p.saturating_sub(delta);
            let evicted = self.replace(true);
            let _ghost = self.b2.remove(&key);
            self.t2.insert_front(key, val);
            return evicted;
        }

        // Total miss: not live, not a ghost.
        let mut evicted = false;
        if self.t1.len() + self.b1.len() == c {
            if self.t1.len() < c {
                let _dropped = self.b1.pop_back();
                evicted |= self.replace(false);
            } else {
                // b1 empty and t1 full: drop the LRU of t1 outright (no ghost).
                evicted |= self.t1.pop_back().is_some();
            }
        } else {
            let total = self.t1.len() + self.t2.len() + self.b1.len() + self.b2.len();
            if total >= c {
                if total == 2 * c {
                    let _dropped = self.b2.pop_back();
                }
                evicted |= self.replace(false);
            }
        }
        self.t1.insert_front(key, val);
        evicted
    }

    fn clear(&mut self) {
        self.t1.clear();
        self.t2.clear();
        self.b1.clear();
        self.b2.clear();
        self.p = 0;
    }

    fn len(&self) -> usize {
        self.t1.len() + self.t2.len()
    }
}

/// A cache that dispatches every operation to the configured eviction policy.
pub(crate) enum PolicyCache<K, V> {
    /// LRU (`touch_on_get = true`) or FIFO (`false`).
    Recency(RecencyCache<K, V>),
    /// Least-frequently-used.
    Lfu(LfuCache<K, V>),
    /// Adaptive replacement cache. Boxed: it holds four ordered maps and is far
    /// larger than the other variants, so the box keeps `PolicyCache` compact.
    Arc(Box<ArcCache<K, V>>),
}

impl<K: Hash + Eq + Clone, V> PolicyCache<K, V> {
    /// Builds the cache backing `policy` with room for `capacity` entries.
    pub(crate) fn new(policy: EvictionPolicy, capacity: usize) -> Self {
        match policy {
            EvictionPolicy::Lru => Self::Recency(RecencyCache::new(capacity, true)),
            EvictionPolicy::Fifo => Self::Recency(RecencyCache::new(capacity, false)),
            EvictionPolicy::Lfu => Self::Lfu(LfuCache::new(capacity)),
            EvictionPolicy::Arc => Self::Arc(Box::new(ArcCache::new(capacity))),
        }
    }

    /// Looks up `key`, applying the policy's access bookkeeping on a hit.
    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        match self {
            Self::Recency(cache) => cache.get(key),
            Self::Lfu(cache) => cache.get(key),
            Self::Arc(cache) => cache.get(key),
        }
    }

    /// Inserts `key`, evicting per the policy if full. Returns `true` when a
    /// previously cached value was discarded.
    pub(crate) fn put(&mut self, key: K, val: V) -> bool {
        match self {
            Self::Recency(cache) => cache.put(key, val),
            Self::Lfu(cache) => cache.put(key, val),
            Self::Arc(cache) => cache.put(key, val),
        }
    }

    /// Removes every entry.
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Recency(cache) => cache.clear(),
            Self::Lfu(cache) => cache.clear(),
            Self::Arc(cache) => cache.clear(),
        }
    }

    /// The number of live entries currently held.
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Recency(cache) => cache.len(),
            Self::Lfu(cache) => cache.len(),
            Self::Arc(cache) => cache.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn drive(policy: EvictionPolicy) -> PolicyCache<u32, u32> {
        PolicyCache::new(policy, 2)
    }

    #[test]
    fn every_policy_caches_and_bounds_capacity() {
        for policy in [
            EvictionPolicy::Lru,
            EvictionPolicy::Fifo,
            EvictionPolicy::Lfu,
            EvictionPolicy::Arc,
        ] {
            let mut cache = drive(policy);
            for i in 0..100u32 {
                let _ = cache.put(i, i * 10);
                assert!(cache.len() <= 2, "{policy:?} exceeded capacity");
            }
            // The most recent insert is always retained.
            assert_eq!(cache.get(&99), Some(&990), "{policy:?} lost newest");
        }
    }

    #[test]
    fn lru_evicts_least_recently_used() {
        let mut cache = PolicyCache::new(EvictionPolicy::Lru, 2);
        assert!(!cache.put(1, 1));
        assert!(!cache.put(2, 2));
        // Touch 1, so 2 is the LRU.
        assert_eq!(cache.get(&1), Some(&1));
        assert!(cache.put(3, 3)); // evicts 2
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&1));
        assert_eq!(cache.get(&3), Some(&3));
    }

    #[test]
    fn fifo_evicts_oldest_inserted_regardless_of_access() {
        let mut cache = PolicyCache::new(EvictionPolicy::Fifo, 2);
        let _ = cache.put(1, 1);
        let _ = cache.put(2, 2);
        // Access 1 — FIFO does NOT protect it.
        assert_eq!(cache.get(&1), Some(&1));
        let _ = cache.put(3, 3); // evicts 1 (oldest insert)
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&2));
        assert_eq!(cache.get(&3), Some(&3));
    }

    #[test]
    fn lfu_evicts_least_frequently_used() {
        let mut cache = PolicyCache::new(EvictionPolicy::Lfu, 2);
        let _ = cache.put(1, 1);
        let _ = cache.put(2, 2);
        // Make 1 frequent; 2 stays at frequency 1.
        assert_eq!(cache.get(&1), Some(&1));
        assert_eq!(cache.get(&1), Some(&1));
        let _ = cache.put(3, 3); // evicts 2 (least frequent)
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&1));
        assert_eq!(cache.get(&3), Some(&3));
    }

    #[test]
    fn arc_keeps_frequent_keys_and_bounds_occupancy() {
        let mut cache = PolicyCache::new(EvictionPolicy::Arc, 2);
        let _ = cache.put(1, 1);
        let _ = cache.put(2, 2);
        // Promote 1 into the frequency list.
        assert_eq!(cache.get(&1), Some(&1));
        // Churn new keys; occupancy must stay within capacity throughout.
        for k in 3..20u32 {
            let _ = cache.put(k, k);
            assert!(cache.len() <= 2);
        }
    }

    #[test]
    fn put_reports_eviction() {
        let mut cache = PolicyCache::new(EvictionPolicy::Lru, 1);
        assert!(!cache.put(1, 1)); // fits, no eviction
        assert!(cache.put(2, 2)); // evicts 1
    }

    #[test]
    fn clear_empties_every_policy() {
        for policy in [
            EvictionPolicy::Lru,
            EvictionPolicy::Fifo,
            EvictionPolicy::Lfu,
            EvictionPolicy::Arc,
        ] {
            let mut cache = drive(policy);
            let _ = cache.put(1, 1);
            let _ = cache.put(2, 2);
            cache.clear();
            assert_eq!(cache.len(), 0, "{policy:?} not cleared");
            assert_eq!(cache.get(&1), None);
        }
    }

    #[test]
    fn capacity_zero_stores_nothing() {
        for policy in [
            EvictionPolicy::Lru,
            EvictionPolicy::Fifo,
            EvictionPolicy::Lfu,
            EvictionPolicy::Arc,
        ] {
            let mut cache = PolicyCache::new(policy, 0);
            assert!(!cache.put(1, 1));
            assert_eq!(cache.get(&1), None);
            assert_eq!(cache.len(), 0);
        }
    }
}
