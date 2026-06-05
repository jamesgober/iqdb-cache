//! The result-cache key.
//!
//! [`ResultKey`] identifies a search exactly: two searches share a cached
//! result only when their query vector and every parameter that can change the
//! outcome are identical. That makes the cache *correct by construction* — a
//! lookup can never serve the result of a different query.

use std::hash::{Hash, Hasher};

use iqdb_types::{DistanceMetric, Filter, SearchParams};

/// An owned, hashable identity for a `(query, params)` search.
///
/// Equality is exact: the query is compared bit-for-bit (via [`f32::to_bits`],
/// so it is reflexive even for `NaN` components), and every
/// [`SearchParams`] field that influences the result — `k`, `ef`, `metric`,
/// and `filter` — must match. The hash is derived from the query bits and the
/// scalar parameters; the [`Filter`] participates in equality but not in the
/// hash (it is compared, never hashed), which only affects bucket distribution
/// and never correctness.
#[derive(Clone, Debug)]
pub(crate) struct ResultKey {
    /// The query vector, owned bit-exactly.
    query: Box<[f32]>,
    /// Number of neighbours requested.
    k: usize,
    /// Optional search-breadth knob.
    ef: Option<usize>,
    /// Distance metric.
    metric: DistanceMetric,
    /// Optional metadata predicate.
    filter: Option<Filter>,
}

impl ResultKey {
    /// Builds a key from a query slice and its search parameters, copying the
    /// query into owned storage so the key outlives the borrowed call.
    pub(crate) fn new(query: &[f32], params: &SearchParams) -> Self {
        Self {
            query: Box::from(query),
            k: params.k,
            ef: params.ef,
            metric: params.metric,
            filter: params.filter.clone(),
        }
    }
}

impl PartialEq for ResultKey {
    fn eq(&self, other: &Self) -> bool {
        self.k == other.k
            && self.ef == other.ef
            && self.metric == other.metric
            && self.query.len() == other.query.len()
            && self
                .query
                .iter()
                .zip(other.query.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits())
            && self.filter == other.filter
    }
}

// SAFETY of `Eq`: equality is reflexive. Query components are compared on their
// raw bits, so `NaN == NaN` holds here; the scalar fields are themselves `Eq`.
// `Filter` carries floating-point values whose `PartialEq` is not reflexive for
// `NaN`; a filter literal containing `NaN` is pathological and unsupported as a
// cache key. Under that documented restriction, `eq` is a full equivalence.
impl Eq for ResultKey {}

impl Hash for ResultKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.k.hash(state);
        self.ef.hash(state);
        self.metric.hash(state);
        for component in self.query.iter() {
            state.write_u32(component.to_bits());
        }
        // `filter` is deliberately omitted: it is not `Hash`, and excluding it
        // affects only how keys distribute across buckets, never which keys
        // compare equal.
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::hash_map::DefaultHasher;

    use iqdb_types::Value;

    use super::*;

    fn hash_of(key: &ResultKey) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn identical_searches_are_equal_and_hash_equal() {
        let params = SearchParams::new(5, DistanceMetric::Cosine);
        let a = ResultKey::new(&[1.0, 2.0, 3.0], &params);
        let b = ResultKey::new(&[1.0, 2.0, 3.0], &params);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn different_query_differs() {
        let params = SearchParams::new(5, DistanceMetric::Cosine);
        let a = ResultKey::new(&[1.0, 2.0, 3.0], &params);
        let b = ResultKey::new(&[1.0, 2.0, 3.5], &params);
        assert_ne!(a, b);
    }

    #[test]
    fn different_k_differs() {
        let a = ResultKey::new(&[1.0], &SearchParams::new(5, DistanceMetric::Cosine));
        let b = ResultKey::new(&[1.0], &SearchParams::new(6, DistanceMetric::Cosine));
        assert_ne!(a, b);
    }

    #[test]
    fn different_metric_differs() {
        let a = ResultKey::new(&[1.0], &SearchParams::new(5, DistanceMetric::Cosine));
        let b = ResultKey::new(&[1.0], &SearchParams::new(5, DistanceMetric::Euclidean));
        assert_ne!(a, b);
    }

    #[test]
    fn different_filter_differs() {
        let with_filter = SearchParams {
            filter: Some(Filter::eq("k", Value::Bool(true))),
            ..SearchParams::new(5, DistanceMetric::Cosine)
        };
        let a = ResultKey::new(&[1.0], &SearchParams::new(5, DistanceMetric::Cosine));
        let b = ResultKey::new(&[1.0], &with_filter);
        assert_ne!(a, b);
    }

    #[test]
    fn different_length_differs() {
        let params = SearchParams::new(5, DistanceMetric::Cosine);
        let a = ResultKey::new(&[1.0, 2.0], &params);
        let b = ResultKey::new(&[1.0, 2.0, 3.0], &params);
        assert_ne!(a, b);
    }

    #[test]
    fn negative_zero_distinct_from_zero() {
        // -0.0 and 0.0 differ in bits, so they are distinct cache keys.
        let params = SearchParams::new(1, DistanceMetric::Cosine);
        let a = ResultKey::new(&[0.0], &params);
        let b = ResultKey::new(&[-0.0], &params);
        assert_ne!(a, b);
    }
}
