//! An insertion/recency-ordered map: the primitive every eviction policy is
//! built on.
//!
//! [`OrderedMap`] is a hash map whose entries are also threaded onto a single
//! doubly-linked list in *most-recently-touched-first* order. It exposes the
//! low-level moves the policies compose — insert at the front, look up without
//! reordering, promote an existing key to the front, remove an arbitrary key,
//! and pop the back (the least-recently-touched entry) — each amortized `O(1)`.
//!
//! Unlike a fixed-capacity cache, `OrderedMap` imposes no bound of its own; the
//! policy on top decides when to [`pop_back`](OrderedMap::pop_back). Entries
//! live in a flat arena of slots with a free-list, so removals recycle storage
//! without per-entry allocation, and the structure contains no `unsafe`.

use std::collections::HashMap;
use std::hash::Hash;

/// Sentinel index marking "no neighbour" — the ends of the list and a detached
/// slot's links.
const NIL: usize = usize::MAX;

/// One arena entry. Present slots are `Some`; freed slots are `None` and live in
/// the free-list awaiting reuse.
struct Node<K, V> {
    key: K,
    val: V,
    /// More-recently-touched neighbour, or [`NIL`] at the front.
    prev: usize,
    /// Less-recently-touched neighbour, or [`NIL`] at the back.
    next: usize,
}

/// A recency-ordered map with `O(1)` front/back operations and arbitrary
/// removal.
pub(crate) struct OrderedMap<K, V> {
    /// Key to slot-index lookup.
    map: HashMap<K, usize>,
    /// Slot arena; `None` entries are free.
    slots: Vec<Option<Node<K, V>>>,
    /// Recycled slot indices.
    free: Vec<usize>,
    /// Front (most-recently-touched) slot, or [`NIL`] when empty.
    front: usize,
    /// Back (least-recently-touched) slot, or [`NIL`] when empty.
    back: usize,
}

impl<K: Hash + Eq + Clone, V> OrderedMap<K, V> {
    /// An empty map with room reserved for `capacity` entries.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            slots: Vec::with_capacity(capacity),
            free: Vec::new(),
            front: NIL,
            back: NIL,
        }
    }

    /// The number of live entries.
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether `key` is present.
    pub(crate) fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Looks up `key` without changing its position.
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        let idx = *self.map.get(key)?;
        self.slots
            .get(idx)
            .and_then(|slot| slot.as_ref())
            .map(|node| &node.val)
    }

    /// Looks up `key` in a single hash probe, optionally promoting it to the
    /// front. This is the hot-path accessor the recency policies use, avoiding
    /// the separate `contains` + `move_to_front` + `get` lookups.
    pub(crate) fn access(&mut self, key: &K, promote: bool) -> Option<&V> {
        let idx = *self.map.get(key)?;
        if promote {
            self.detach(idx);
            self.attach_front(idx);
        }
        self.slots
            .get(idx)
            .and_then(|slot| slot.as_ref())
            .map(|node| &node.val)
    }

    /// Replaces the value for an existing `key` without reordering. Does nothing
    /// if the key is absent.
    pub(crate) fn update_value(&mut self, key: &K, val: V) {
        if let Some(&idx) = self.map.get(key) {
            if let Some(node) = self.slots[idx].as_mut() {
                node.val = val;
            }
        }
    }

    /// Reads a slot's neighbours, or `(NIL, NIL)` if it is somehow vacant.
    fn links(&self, idx: usize) -> (usize, usize) {
        match self.slots.get(idx).and_then(|slot| slot.as_ref()) {
            Some(node) => (node.prev, node.next),
            None => (NIL, NIL),
        }
    }

    /// Unlinks `idx` from the list, repairing its neighbours.
    fn detach(&mut self, idx: usize) {
        let (prev, next) = self.links(idx);
        if prev != NIL {
            if let Some(node) = self.slots[prev].as_mut() {
                node.next = next;
            }
        } else {
            self.front = next;
        }
        if next != NIL {
            if let Some(node) = self.slots[next].as_mut() {
                node.prev = prev;
            }
        } else {
            self.back = prev;
        }
    }

    /// Links the already-detached `idx` in at the front (most-recently-touched).
    fn attach_front(&mut self, idx: usize) {
        let old_front = self.front;
        if let Some(node) = self.slots[idx].as_mut() {
            node.prev = NIL;
            node.next = old_front;
        }
        if old_front != NIL {
            if let Some(node) = self.slots[old_front].as_mut() {
                node.prev = idx;
            }
        } else {
            self.back = idx;
        }
        self.front = idx;
    }

    /// Inserts `key` at the front. If `key` already exists, updates its value and
    /// promotes it to the front.
    pub(crate) fn insert_front(&mut self, key: K, val: V) {
        if let Some(&idx) = self.map.get(&key) {
            if let Some(node) = self.slots[idx].as_mut() {
                node.val = val;
            }
            self.detach(idx);
            self.attach_front(idx);
            return;
        }
        let node = Node {
            key: key.clone(),
            val,
            prev: NIL,
            next: NIL,
        };
        let idx = match self.free.pop() {
            Some(reused) => {
                self.slots[reused] = Some(node);
                reused
            }
            None => {
                self.slots.push(Some(node));
                self.slots.len() - 1
            }
        };
        let _prev = self.map.insert(key, idx);
        self.attach_front(idx);
    }

    /// Promotes an existing `key` to the front. Does nothing if absent.
    pub(crate) fn move_to_front(&mut self, key: &K) {
        if let Some(&idx) = self.map.get(key) {
            self.detach(idx);
            self.attach_front(idx);
        }
    }

    /// Removes `key`, returning its value if it was present.
    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let idx = self.map.remove(key)?;
        self.detach(idx);
        let node = self.slots[idx].take();
        self.free.push(idx);
        node.map(|node| node.val)
    }

    /// Removes and returns the back (least-recently-touched) entry.
    pub(crate) fn pop_back(&mut self) -> Option<(K, V)> {
        if self.back == NIL {
            return None;
        }
        let idx = self.back;
        let key = self
            .slots
            .get(idx)
            .and_then(|slot| slot.as_ref())
            .map(|node| node.key.clone())?;
        self.detach(idx);
        let _removed = self.map.remove(&key);
        let node = self.slots[idx].take();
        self.free.push(idx);
        node.map(|node| (key, node.val))
    }

    /// Empties the map, keeping allocated capacity for reuse.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.slots.clear();
        self.free.clear();
        self.front = NIL;
        self.back = NIL;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn insert_get_contains() {
        let mut m: OrderedMap<u32, u32> = OrderedMap::with_capacity(4);
        m.insert_front(1, 10);
        m.insert_front(2, 20);
        assert!(m.contains(&1));
        assert_eq!(m.get(&2), Some(&20));
        assert_eq!(m.get(&3), None);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn pop_back_is_least_recently_touched() {
        let mut m = OrderedMap::with_capacity(4);
        m.insert_front(1u32, 10u32);
        m.insert_front(2, 20);
        m.insert_front(3, 30);
        // Back is the oldest insert (1), front is newest (3).
        assert_eq!(m.pop_back(), Some((1, 10)));
        assert_eq!(m.pop_back(), Some((2, 20)));
        assert_eq!(m.pop_back(), Some((3, 30)));
        assert_eq!(m.pop_back(), None);
    }

    #[test]
    fn move_to_front_changes_eviction_order() {
        let mut m = OrderedMap::with_capacity(4);
        m.insert_front(1u32, 10u32);
        m.insert_front(2, 20);
        // Touch 1 -> it is no longer the back.
        m.move_to_front(&1);
        assert_eq!(m.pop_back(), Some((2, 20)));
    }

    #[test]
    fn remove_arbitrary_and_reuse_slot() {
        let mut m = OrderedMap::with_capacity(4);
        m.insert_front(1u32, 10u32);
        m.insert_front(2, 20);
        m.insert_front(3, 30);
        assert_eq!(m.remove(&2), Some(20));
        assert_eq!(m.remove(&2), None);
        assert_eq!(m.len(), 2);
        // The list is still consistent end to end.
        assert_eq!(m.pop_back(), Some((1, 10)));
        assert_eq!(m.pop_back(), Some((3, 30)));
    }

    #[test]
    fn insert_existing_updates_and_promotes() {
        let mut m = OrderedMap::with_capacity(4);
        m.insert_front(1u32, 10u32);
        m.insert_front(2, 20);
        m.insert_front(1, 11); // update + promote
        assert_eq!(m.get(&1), Some(&11));
        assert_eq!(m.len(), 2);
        // 2 is now the back.
        assert_eq!(m.pop_back(), Some((2, 20)));
    }

    #[test]
    fn update_value_does_not_reorder() {
        let mut m = OrderedMap::with_capacity(4);
        m.insert_front(1u32, 10u32);
        m.insert_front(2, 20);
        m.update_value(&1, 99);
        assert_eq!(m.get(&1), Some(&99));
        // 1 is still the back (unchanged order).
        assert_eq!(m.pop_back(), Some((1, 99)));
    }

    #[test]
    fn clear_then_reuse() {
        let mut m = OrderedMap::with_capacity(2);
        m.insert_front(1u32, 1u32);
        m.clear();
        assert_eq!(m.len(), 0);
        assert_eq!(m.get(&1), None);
        m.insert_front(2, 2);
        assert_eq!(m.get(&2), Some(&2));
    }

    #[test]
    fn single_element_pop_resets_ends() {
        let mut m = OrderedMap::with_capacity(2);
        m.insert_front(1u32, 1u32);
        assert_eq!(m.pop_back(), Some((1, 1)));
        assert_eq!(m.pop_back(), None);
        // Reinsert works after the list emptied.
        m.insert_front(2, 2);
        assert_eq!(m.get(&2), Some(&2));
    }
}
