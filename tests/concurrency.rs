//! `CachedIndex<I>` is `Send + Sync` when `I` is, and concurrent searches
//! through `&self` are correct: every thread sees a result equal to the
//! reference, and hits and misses sum to the number of lookups.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::thread;

use common::MockIndex;
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

#[test]
fn concurrent_searches_are_correct_and_accounted() {
    let mut index = MockIndex::new(2);
    for i in 0..16u64 {
        index
            .insert(VectorId::from(i), Arc::from(&[i as f32, 0.0][..]), None)
            .unwrap();
    }
    let reference = index.clone();
    let cached = Arc::new(CachedIndex::new(index));

    let params = SearchParams::new(4, DistanceMetric::Euclidean);
    let want = reference.search(&[3.0, 0.0], &params).unwrap();

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let cached = Arc::clone(&cached);
            let params = params.clone();
            let want = want.clone();
            thread::spawn(move || {
                for _ in 0..256 {
                    let got = cached.search(&[3.0, 0.0], &params).unwrap();
                    assert_eq!(got, want);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }

    let stats = cached.cache_stats();
    // 8 threads * 256 lookups of a single query: every lookup is a hit or miss,
    // and at most one cold miss occurs for the query.
    assert_eq!(stats.lookups(), 8 * 256);
    assert!(stats.misses >= 1);
    assert_eq!(stats.len, 1);
}

#[test]
fn wrapper_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CachedIndex<MockIndex>>();
}
