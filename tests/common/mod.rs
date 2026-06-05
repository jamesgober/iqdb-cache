//! Shared test fixtures: a brute-force reference index used to validate that
//! `CachedIndex` is a transparent wrapper.
#![allow(dead_code)]

use std::sync::Arc;

use iqdb_index::{Index, IndexCore, IndexStats};
use iqdb_types::{DistanceMetric, Hit, IqdbError, Metadata, Result, SearchParams, VectorId};

/// A simple, correct, brute-force index over Euclidean distance.
///
/// It exists only as a known-good oracle in tests: every `CachedIndex<MockIndex>`
/// must return exactly what a bare `MockIndex` in the same state returns.
#[derive(Clone, Default)]
pub struct MockIndex {
    dim: usize,
    rows: Vec<(VectorId, Arc<[f32]>)>,
}

impl MockIndex {
    /// A fresh `dim`-dimensional index.
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            rows: Vec::new(),
        }
    }
}

fn euclidean_sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y) * (x - y)).sum()
}

impl IndexCore for MockIndex {
    fn insert(&mut self, id: VectorId, vector: Arc<[f32]>, _m: Option<Metadata>) -> Result<()> {
        if vector.len() != self.dim {
            return Err(IqdbError::DimensionMismatch {
                expected: self.dim,
                found: vector.len(),
            });
        }
        if self.rows.iter().any(|(existing, _)| existing == &id) {
            return Err(IqdbError::Duplicate);
        }
        self.rows.push((id, vector));
        Ok(())
    }

    fn delete(&mut self, id: &VectorId) -> Result<()> {
        match self.rows.iter().position(|(existing, _)| existing == id) {
            Some(pos) => {
                let _removed = self.rows.remove(pos);
                Ok(())
            }
            None => Err(IqdbError::NotFound),
        }
    }

    fn search(&self, query: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
        if query.len() != self.dim {
            return Err(IqdbError::DimensionMismatch {
                expected: self.dim,
                found: query.len(),
            });
        }
        let mut scored: Vec<Hit> = self
            .rows
            .iter()
            .map(|(id, v)| Hit::new(id.clone(), euclidean_sq(query, v)))
            .collect();
        // Stable best-first ordering: distance, then id, so ties are deterministic.
        scored.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| format!("{:?}", a.id).cmp(&format!("{:?}", b.id)))
        });
        scored.truncate(params.k);
        Ok(scored)
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn metric(&self) -> DistanceMetric {
        DistanceMetric::Euclidean
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn stats(&self) -> IndexStats {
        IndexStats {
            n_vectors: self.rows.len(),
            index_type: "mock",
            ..IndexStats::default()
        }
    }
}

impl Index for MockIndex {
    type Config = ();
    fn new(dim: usize, _metric: DistanceMetric, _config: Self::Config) -> Result<Self> {
        if dim == 0 {
            return Err(IqdbError::InvalidConfig {
                reason: "dim must be greater than zero",
            });
        }
        Ok(MockIndex::new(dim))
    }
}
