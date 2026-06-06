//! The configured path: size the cache, set a TTL, and read the stats back to
//! tune it. Also shows that a write invalidates the cache so a search is never
//! stale.
//!
//! Run with `cargo run --example tuning`.

#[path = "support/flat_index.rs"]
mod flat_index;

use std::sync::Arc;
use std::time::Duration;

use flat_index::FlatIndex;
use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

fn main() {
    let mut index = FlatIndex::new(2);
    for id in 0..8u64 {
        index
            .insert(VectorId::from(id), Arc::from(&[id as f32, 0.0][..]), None)
            .expect("insert");
    }

    // Tier-2 configuration: 1024 entries, a 5-minute TTL, LFU eviction.
    let config = CacheConfig::new()
        .capacity(1024)
        .ttl(Duration::from_secs(300))
        .policy(EvictionPolicy::Lfu);
    let mut cached = CachedIndex::with_config(index, config);
    println!(
        "configured: capacity={} ttl={:?} policy={:?}",
        cached.capacity(),
        cached.ttl(),
        cached.policy()
    );

    let params = SearchParams::new(3, DistanceMetric::Euclidean);

    // Re-issue a handful of queries from a small hot-set; later repeats hit.
    let hot_set = [[1.0, 0.0], [2.0, 0.0], [1.0, 0.0], [2.0, 0.0], [1.0, 0.0]];
    for q in hot_set {
        let _ = cached.search(&q, &params).expect("search");
    }
    let warm = cached.cache_stats();
    println!(
        "after warm-up: hits={} misses={} hit_rate={:.0}%",
        warm.hits,
        warm.misses,
        warm.hit_rate() * 100.0
    );

    // A write invalidates the cache: the next search reflects it immediately.
    cached
        .insert(VectorId::from(99u64), Arc::from(&[1.0, 0.0][..]), None)
        .expect("insert");
    let after = cached.search(&[1.0, 0.0], &params).expect("search");
    println!(
        "after insert: top id={:?} (recomputed, never stale)",
        after.first().map(|h| &h.id)
    );
}
