//! The lazy path: wrap an index and let repeated searches come from memory.
//!
//! Run with `cargo run --example quickstart`.

#[path = "support/flat_index.rs"]
mod flat_index;

use std::sync::Arc;

use flat_index::FlatIndex;
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

fn main() {
    // Build a small index of 3-D vectors.
    let mut index = FlatIndex::new(3);
    for (id, v) in [
        (1u64, [1.0, 0.0, 0.0]),
        (2, [0.0, 1.0, 0.0]),
        (3, [0.0, 0.0, 1.0]),
    ] {
        index
            .insert(VectorId::from(id), Arc::from(&v[..]), None)
            .expect("insert");
    }

    // Wrap it — that is the whole opt-in. `CachedIndex` is itself an `IndexCore`.
    let cached = CachedIndex::new(index);
    let params = SearchParams::new(2, DistanceMetric::Euclidean);

    // First search runs against the index (a miss); the second is served from
    // the cache (a hit). The results are identical.
    let cold = cached.search(&[0.9, 0.1, 0.0], &params).expect("search");
    let warm = cached.search(&[0.9, 0.1, 0.0], &params).expect("search");
    assert_eq!(cold, warm);

    println!(
        "nearest two ids: {:?}",
        warm.iter().map(|h| &h.id).collect::<Vec<_>>()
    );

    let stats = cached.cache_stats();
    println!(
        "hits={} misses={} hit_rate={:.0}%",
        stats.hits,
        stats.misses,
        stats.hit_rate() * 100.0
    );
}
