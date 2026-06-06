//! A consumer simulation: drive `CachedIndex` the way a real search service
//! would — a realistic index, a skewed (hot-set) query stream, and a mid-run
//! read/write mix — and assert the two guarantees that matter end to end:
//!
//! 1. **Transparency.** Every cached result equals the bare reference index's,
//!    through inserts and deletes, so the cache is never observed to be stale.
//! 2. **It earns its keep.** Against a repeating hot-set the hit rate is high,
//!    i.e. the cache actually turns repeated queries into memory reads.
//!
//! This exercises only the public surface, the way `iqdb-flat` / `iqdb-hnsw` /
//! `iqdb-ivf` will when they wrap an index in a cache.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::MockIndex;
use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

const DIM: usize = 8;
const N_VECTORS: u64 = 200;
const N_QUERIES: usize = 500;
const HOT_SET: usize = 10;

/// Deterministic vector for id `i` — no RNG, fully reproducible.
fn vector_for(i: u64) -> Arc<[f32]> {
    let base = (i % 50) as f32;
    let buf: Vec<f32> = (0..DIM).map(|d| base + d as f32 * 0.25).collect();
    Arc::from(buf.into_boxed_slice())
}

/// The `j`-th query of the stream: 4 in 5 are drawn from a small hot-set (so
/// they repeat and should hit); 1 in 5 is a unique cold query (a miss).
fn query_for(j: usize) -> Vec<f32> {
    let pick = if j.is_multiple_of(5) {
        100 + j // cold: unique each time
    } else {
        j % HOT_SET // hot: one of HOT_SET values, repeats often
    };
    let base = (pick % 50) as f32 + 0.1;
    (0..DIM).map(|d| base + d as f32 * 0.25).collect()
}

fn build(policy: EvictionPolicy) -> (MockIndex, CachedIndex<MockIndex>) {
    let mut reference = MockIndex::new(DIM);
    let mut inner = MockIndex::new(DIM);
    for i in 0..N_VECTORS {
        let v = vector_for(i);
        reference
            .insert(VectorId::from(i), v.clone(), None)
            .unwrap();
        inner.insert(VectorId::from(i), v, None).unwrap();
    }
    let cached = CachedIndex::with_config(inner, CacheConfig::new().capacity(64).policy(policy));
    (reference, cached)
}

fn run_policy(policy: EvictionPolicy) {
    let (mut reference, mut cached) = build(policy);
    let params = SearchParams::new(10, DistanceMetric::Euclidean);

    for j in 0..N_QUERIES {
        // A mid-stream write mix: insert a new vector at 1/3, delete one at 2/3.
        // Both are mirrored on the reference, and both must invalidate the cache.
        if j == N_QUERIES / 3 {
            let v = vector_for(N_VECTORS + 1);
            reference
                .insert(VectorId::from(N_VECTORS + 1), v.clone(), None)
                .unwrap();
            cached
                .insert(VectorId::from(N_VECTORS + 1), v, None)
                .unwrap();
        }
        if j == 2 * N_QUERIES / 3 {
            reference.delete(&VectorId::from(0u64)).unwrap();
            cached.delete(&VectorId::from(0u64)).unwrap();
        }

        let q = query_for(j);
        let want = reference.search(&q, &params).unwrap();
        let got = cached.search(&q, &params).unwrap();
        assert_eq!(got, want, "{policy:?}: cached result diverged at query {j}");
    }

    let stats = cached.cache_stats();
    // The cache must stay within its bound and demonstrably help on the hot-set.
    assert!(stats.len <= 64, "{policy:?}: exceeded capacity");
    assert!(
        stats.hit_rate() > 0.3,
        "{policy:?}: hit rate {:.2} too low to be useful",
        stats.hit_rate()
    );
}

#[test]
fn lru_consumer_workload() {
    run_policy(EvictionPolicy::Lru);
}

#[test]
fn lfu_consumer_workload() {
    run_policy(EvictionPolicy::Lfu);
}

#[test]
fn fifo_consumer_workload() {
    run_policy(EvictionPolicy::Fifo);
}

#[test]
fn arc_consumer_workload() {
    run_policy(EvictionPolicy::Arc);
}

#[test]
fn disabled_cache_still_correct_under_workload() {
    // Capacity 0 = pure passthrough: still exactly the reference, just no hits.
    let mut reference = MockIndex::new(DIM);
    let mut inner = MockIndex::new(DIM);
    for i in 0..N_VECTORS {
        let v = vector_for(i);
        reference
            .insert(VectorId::from(i), v.clone(), None)
            .unwrap();
        inner.insert(VectorId::from(i), v, None).unwrap();
    }
    let cached = CachedIndex::with_capacity(inner, 0);
    let params = SearchParams::new(10, DistanceMetric::Euclidean);

    for j in 0..100 {
        let q = query_for(j);
        assert_eq!(
            cached.search(&q, &params).unwrap(),
            reference.search(&q, &params).unwrap()
        );
    }
    assert_eq!(cached.cache_stats().hits, 0);
}
