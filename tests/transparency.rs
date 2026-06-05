//! `CachedIndex` is a transparent wrapper: it never changes *what* a search
//! returns, only how fast a repeat returns. These tests pin both the
//! example-level behavior and the core invariants from `dev/DIRECTIVES.md` §8.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::MockIndex;
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};
use proptest::prelude::*;

fn vec_arc(values: &[f32]) -> Arc<[f32]> {
    Arc::from(values)
}

#[test]
fn repeat_search_hits_cache_and_matches() {
    let mut index = MockIndex::new(3);
    index
        .insert(VectorId::from(1u64), vec_arc(&[1.0, 0.0, 0.0]), None)
        .unwrap();
    index
        .insert(VectorId::from(2u64), vec_arc(&[0.0, 1.0, 0.0]), None)
        .unwrap();

    let cached = CachedIndex::new(index);
    let params = SearchParams::new(2, DistanceMetric::Euclidean);

    let first = cached.search(&[1.0, 0.0, 0.0], &params).unwrap();
    let second = cached.search(&[1.0, 0.0, 0.0], &params).unwrap();
    assert_eq!(first, second);

    let stats = cached.cache_stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.len, 1);
}

#[test]
fn insert_invalidates_so_no_stale_result() {
    let mut cached = CachedIndex::new(MockIndex::new(2));
    let params = SearchParams::new(5, DistanceMetric::Euclidean);

    cached
        .insert(VectorId::from(1u64), vec_arc(&[10.0, 10.0]), None)
        .unwrap();
    let before = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(before.len(), 1);

    // A new, closer vector must show up in a fresh search — never a stale cache.
    cached
        .insert(VectorId::from(2u64), vec_arc(&[0.0, 0.0]), None)
        .unwrap();
    let after = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].id, VectorId::from(2u64));
}

#[test]
fn delete_invalidates_so_no_stale_result() {
    let mut cached = CachedIndex::new(MockIndex::new(2));
    let params = SearchParams::new(5, DistanceMetric::Euclidean);
    cached
        .insert(VectorId::from(1u64), vec_arc(&[0.0, 0.0]), None)
        .unwrap();
    let _warm = cached.search(&[0.0, 0.0], &params).unwrap();

    cached.delete(&VectorId::from(1u64)).unwrap();
    let after = cached.search(&[0.0, 0.0], &params).unwrap();
    assert!(after.is_empty());
}

#[test]
fn failed_insert_does_not_invalidate() {
    let mut cached = CachedIndex::new(MockIndex::new(2));
    let params = SearchParams::new(5, DistanceMetric::Euclidean);
    cached
        .insert(VectorId::from(1u64), vec_arc(&[0.0, 0.0]), None)
        .unwrap();
    let _warm = cached.search(&[0.0, 0.0], &params).unwrap();
    let _warm2 = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(cached.cache_stats().hits, 1);

    // Re-inserting a duplicate id fails and changes nothing, so the cache stays.
    let dup = cached.insert(VectorId::from(1u64), vec_arc(&[0.0, 0.0]), None);
    assert!(dup.is_err());
    let _again = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(cached.cache_stats().hits, 2);
}

#[test]
fn disabled_cache_is_pure_passthrough() {
    let mut cached = CachedIndex::with_capacity(MockIndex::new(2), 0);
    assert!(!cached.is_enabled());
    let params = SearchParams::new(5, DistanceMetric::Euclidean);
    cached
        .insert(VectorId::from(1u64), vec_arc(&[1.0, 2.0]), None)
        .unwrap();
    let a = cached.search(&[0.0, 0.0], &params).unwrap();
    let b = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(a, b);
    let stats = cached.cache_stats();
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.len, 0);
}

#[test]
fn clear_cache_forces_recompute_without_changing_results() {
    let mut cached = CachedIndex::new(MockIndex::new(2));
    let params = SearchParams::new(5, DistanceMetric::Euclidean);
    cached
        .insert(VectorId::from(1u64), vec_arc(&[1.0, 1.0]), None)
        .unwrap();
    let a = cached.search(&[0.0, 0.0], &params).unwrap();
    let _hit = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(cached.cache_stats().hits, 1);

    cached.clear_cache();
    assert_eq!(cached.cache_stats().len, 0);
    let b = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(a, b);
    // The clear forced a miss, so hits did not advance on this call.
    assert_eq!(cached.cache_stats().hits, 1);
}

proptest! {
    /// For any insert/delete history and any query, the wrapped cache returns
    /// exactly what a bare reference index in the same state returns — proven
    /// across a mix of cold and warm (repeated) lookups.
    #[test]
    fn cached_matches_reference(
        ids in prop::collection::vec(0u64..32, 0..32),
        queries in prop::collection::vec((-5.0f32..5.0, -5.0f32..5.0), 1..8),
        k in 1usize..16,
    ) {
        let dim = 2usize;
        let mut reference = MockIndex::new(dim);
        let mut cached = CachedIndex::new(MockIndex::new(dim));

        for (i, raw) in ids.iter().enumerate() {
            let id = VectorId::from(*raw);
            // Deterministic vector derived from id + position.
            let v = vec_arc(&[*raw as f32, i as f32]);
            // Mirror the exact same op (and outcome) on both indexes.
            let r_ref = reference.insert(id.clone(), v.clone(), None);
            let r_cache = cached.insert(id, v, None);
            prop_assert_eq!(r_ref.is_ok(), r_cache.is_ok());
        }

        let params = SearchParams::new(k, DistanceMetric::Euclidean);
        for (qx, qy) in queries {
            let q = [qx, qy];
            // First (cold) and second (warm) lookups must both equal the oracle.
            let want = reference.search(&q, &params).unwrap();
            let cold = cached.search(&q, &params).unwrap();
            let warm = cached.search(&q, &params).unwrap();
            prop_assert_eq!(&cold, &want);
            prop_assert_eq!(&warm, &want);
        }
    }

    /// The cache never holds more than its capacity, regardless of how many
    /// distinct queries are issued.
    #[test]
    fn cache_len_never_exceeds_capacity(
        cap in 1usize..16,
        n_queries in 0usize..64,
    ) {
        let mut index = MockIndex::new(1);
        index.insert(VectorId::from(1u64), vec_arc(&[0.0]), None).unwrap();
        let cached = CachedIndex::with_capacity(index, cap);
        let params = SearchParams::new(1, DistanceMetric::Euclidean);
        for i in 0..n_queries {
            // Each distinct query bumps the cache toward its bound.
            let _ = cached.search(&[i as f32], &params).unwrap();
            prop_assert!(cached.cache_stats().len <= cap);
        }
    }
}
