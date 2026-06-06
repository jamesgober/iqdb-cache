//! Loom model checks for the shared result cache.
//!
//! `CachedIndex::search` takes `&self` and guards the cache behind a `Mutex`,
//! releasing it across the wrapped search so concurrent misses run in parallel.
//! Under `--cfg loom` the cache `Mutex` and the counters are loom's instrumented
//! types (see `src/sync.rs`), so these tests explore every interleaving of two
//! threads hitting the same cache and assert the result stays consistent — no
//! deadlock, no lost update, exactly the lookups that happened are counted.
//!
//! Run with `RUSTFLAGS="--cfg loom" cargo test --test loom_iqdb_cache`.
#![cfg(loom)]
#![allow(clippy::unwrap_used)]

mod common;

use common::MockIndex;
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};
use loom::sync::Arc;

#[test]
fn concurrent_searches_of_one_query_are_consistent() {
    loom::model(|| {
        let mut index = MockIndex::new(2);
        index
            .insert(
                VectorId::from(1u64),
                std::sync::Arc::from(&[0.0, 0.0][..]),
                None,
            )
            .unwrap();
        let cached = Arc::new(CachedIndex::new(index));
        let params = SearchParams::new(1, DistanceMetric::Euclidean);

        let threads: Vec<_> = (0..2)
            .map(|_| {
                let cached = Arc::clone(&cached);
                let params = params.clone();
                loom::thread::spawn(move || {
                    let hits = cached.search(&[0.0, 0.0], &params).unwrap();
                    // Whatever the interleaving, the result is the one stored
                    // vector — never corrupt, never empty.
                    assert_eq!(hits.len(), 1);
                    assert_eq!(hits[0].id, VectorId::from(1u64));
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }

        // Every lookup is accounted for exactly once, and the cache settles on
        // the single distinct query regardless of interleaving.
        let stats = cached.cache_stats();
        assert_eq!(stats.hits + stats.misses, 2);
        assert_eq!(stats.len, 1);
    });
}

#[test]
fn concurrent_searches_of_distinct_queries_settle() {
    loom::model(|| {
        let mut index = MockIndex::new(2);
        index
            .insert(
                VectorId::from(1u64),
                std::sync::Arc::from(&[0.0, 0.0][..]),
                None,
            )
            .unwrap();
        let cached = Arc::new(CachedIndex::new(index));
        let params = SearchParams::new(1, DistanceMetric::Euclidean);

        let a = {
            let cached = Arc::clone(&cached);
            let params = params.clone();
            loom::thread::spawn(move || {
                let _ = cached.search(&[0.0, 0.0], &params).unwrap();
            })
        };
        let b = {
            let cached = Arc::clone(&cached);
            let params = params.clone();
            loom::thread::spawn(move || {
                let _ = cached.search(&[1.0, 1.0], &params).unwrap();
            })
        };
        a.join().unwrap();
        b.join().unwrap();

        let stats = cached.cache_stats();
        // Two distinct queries, both missed and both stored.
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.len, 2);
    });
}
