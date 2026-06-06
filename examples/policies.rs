//! Eviction policies side by side: how each one chooses a victim when the cache
//! is full.
//!
//! Run with `cargo run --example policies`.

#[path = "support/flat_index.rs"]
mod flat_index;

use std::sync::Arc;

use flat_index::FlatIndex;
use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

/// Builds a fresh 1-D index with a single vector — enough to make every query
/// return a result while we focus on caching behavior.
fn index() -> FlatIndex {
    let mut idx = FlatIndex::new(1);
    idx.insert(VectorId::from(1u64), Arc::from(&[0.0][..]), None)
        .expect("insert");
    idx
}

fn main() {
    let params = SearchParams::new(1, DistanceMetric::Euclidean);

    for policy in [
        EvictionPolicy::Lru,
        EvictionPolicy::Lfu,
        EvictionPolicy::Fifo,
        EvictionPolicy::Arc,
    ] {
        // A tiny capacity so eviction kicks in immediately.
        let cached =
            CachedIndex::with_config(index(), CacheConfig::new().capacity(2).policy(policy));

        // Warm two queries, hammer the first so it is "hot", then introduce a
        // third distinct query that forces an eviction.
        for q in [0.0_f32, 1.0] {
            let _ = cached.search(&[q], &params).expect("search");
        }
        for _ in 0..3 {
            let _ = cached.search(&[0.0], &params).expect("search"); // keep q=0 hot
        }
        let _ = cached.search(&[2.0], &params).expect("search"); // forces eviction

        let stats = cached.cache_stats();
        println!(
            "{policy:<5?} -> len={} hits={} misses={} evictions={}",
            stats.len, stats.hits, stats.misses, stats.evictions
        );
    }

    println!(
        "\nAll four keep the cache within capacity; they differ only in which entry they drop."
    );
}
