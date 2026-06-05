//! The public Tier-2 surface: `CacheConfig` + `CachedIndex::with_config`,
//! exercised end-to-end without injecting a clock.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::MockIndex;
use iqdb_cache::{CacheConfig, CachedIndex};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

#[test]
fn with_config_sets_capacity_and_ttl() {
    let config = CacheConfig::new()
        .capacity(512)
        .ttl(Duration::from_secs(30));
    let cached = CachedIndex::with_config(MockIndex::new(2), config);
    assert_eq!(cached.capacity(), 512);
    assert_eq!(cached.ttl(), Some(Duration::from_secs(30)));
    assert!(cached.is_enabled());
}

#[test]
fn default_config_matches_new() {
    let a = CachedIndex::with_config(MockIndex::new(2), CacheConfig::new());
    let b = CachedIndex::new(MockIndex::new(2));
    assert_eq!(a.capacity(), b.capacity());
    assert_eq!(a.ttl(), b.ttl());
    assert_eq!(a.ttl(), None);
}

#[test]
fn no_ttl_clears_a_previously_set_ttl() {
    let config = CacheConfig::new().ttl(Duration::from_secs(5)).no_ttl();
    let cached = CachedIndex::with_config(MockIndex::new(2), config);
    assert_eq!(cached.ttl(), None);
}

#[test]
fn long_ttl_behaves_like_a_plain_cache() {
    // A TTL far longer than the test runtime must never expire: a repeat search
    // is a hit, exactly as with no TTL.
    let config = CacheConfig::new()
        .capacity(64)
        .ttl(Duration::from_secs(3600));
    let mut cached = CachedIndex::with_config(MockIndex::new(2), config);
    cached
        .insert(
            VectorId::from(1u64),
            std::sync::Arc::from(&[0.0, 0.0][..]),
            None,
        )
        .unwrap();

    let params = SearchParams::new(5, DistanceMetric::Euclidean);
    let _miss = cached.search(&[0.0, 0.0], &params).unwrap();
    let _hit = cached.search(&[0.0, 0.0], &params).unwrap();
    assert_eq!(cached.cache_stats().hits, 1);
    assert_eq!(cached.cache_stats().misses, 1);
}

#[test]
fn config_capacity_zero_disables() {
    let cached = CachedIndex::with_config(MockIndex::new(2), CacheConfig::new().capacity(0));
    assert!(!cached.is_enabled());
}
