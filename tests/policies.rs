//! Every eviction policy is a *correct* cache: it stays transparent (results
//! equal the reference index) and bounded (occupancy never exceeds capacity),
//! no matter how it chooses victims.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::MockIndex;
use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};
use proptest::prelude::*;

const POLICIES: [EvictionPolicy; 4] = [
    EvictionPolicy::Lru,
    EvictionPolicy::Lfu,
    EvictionPolicy::Fifo,
    EvictionPolicy::Arc,
];

fn vec_arc(values: &[f32]) -> Arc<[f32]> {
    Arc::from(values)
}

#[test]
fn policy_getter_round_trips() {
    for policy in POLICIES {
        let cached = CachedIndex::with_config(MockIndex::new(2), CacheConfig::new().policy(policy));
        assert_eq!(cached.policy(), policy);
    }
}

#[test]
fn default_policy_is_lru() {
    let cached = CachedIndex::new(MockIndex::new(2));
    assert_eq!(cached.policy(), EvictionPolicy::Lru);
}

#[test]
fn evictions_are_counted_under_capacity_pressure() {
    for policy in POLICIES {
        // Capacity 2, three distinct queries -> at least one eviction.
        let config = CacheConfig::new().capacity(2).policy(policy);
        let mut index = MockIndex::new(1);
        index
            .insert(VectorId::from(1u64), vec_arc(&[0.0]), None)
            .unwrap();
        let cached = CachedIndex::with_config(index, config);
        let params = SearchParams::new(1, DistanceMetric::Euclidean);

        for q in 0..3 {
            let _ = cached.search(&[q as f32], &params).unwrap();
        }
        let stats = cached.cache_stats();
        assert!(stats.len <= 2, "{policy:?} exceeded capacity");
        assert!(
            stats.evictions >= 1,
            "{policy:?} did not report an eviction"
        );
    }
}

proptest! {
    /// Under every policy, and even with a cache far smaller than the query
    /// set (so eviction is constant), each search equals the reference index.
    /// Eviction order can only change hit/miss — never correctness.
    #[test]
    fn every_policy_stays_transparent(
        policy_idx in 0usize..4,
        ids in prop::collection::vec(0u64..24, 0..24),
        queries in prop::collection::vec((-4.0f32..4.0, -4.0f32..4.0), 1..12),
        cap in 1usize..6,
        k in 1usize..8,
    ) {
        let policy = POLICIES[policy_idx];
        let dim = 2usize;
        let mut reference = MockIndex::new(dim);
        let mut cached = CachedIndex::with_config(
            MockIndex::new(dim),
            CacheConfig::new().capacity(cap).policy(policy),
        );

        for (i, raw) in ids.iter().enumerate() {
            let id = VectorId::from(*raw);
            let v = vec_arc(&[*raw as f32, i as f32]);
            let r_ref = reference.insert(id.clone(), v.clone(), None);
            let r_cache = cached.insert(id, v, None);
            prop_assert_eq!(r_ref.is_ok(), r_cache.is_ok());
        }

        let params = SearchParams::new(k, DistanceMetric::Euclidean);
        for (qx, qy) in queries {
            let q = [qx, qy];
            let want = reference.search(&q, &params).unwrap();
            // Repeat each query so cache hits (when they happen) are exercised.
            let first = cached.search(&q, &params).unwrap();
            let second = cached.search(&q, &params).unwrap();
            prop_assert_eq!(&first, &want);
            prop_assert_eq!(&second, &want);
            prop_assert!(cached.cache_stats().len <= cap);
        }
    }
}
