//! A small brute-force index shared by the examples.
//!
//! This is *not* part of `iqdb-cache` — it stands in for a real index crate
//! (`iqdb-flat`, `iqdb-hnsw`, `iqdb-ivf`) so each example can wrap something
//! concrete. It implements the same `iqdb_index::IndexCore` contract those
//! crates do: Euclidean top-`k` over `Arc<[f32]>` rows.
#![allow(dead_code)]

use std::sync::Arc;

use iqdb_index::{Index, IndexCore, IndexStats};
use iqdb_types::{DistanceMetric, Hit, IqdbError, Metadata, Result, SearchParams, VectorId};

/// A flat (exhaustive) Euclidean index.
pub struct FlatIndex {
    dim: usize,
    rows: Vec<(VectorId, Arc<[f32]>)>,
}

impl FlatIndex {
    /// A fresh `dim`-dimensional index.
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            rows: Vec::new(),
        }
    }
}

fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

impl IndexCore for FlatIndex {
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
                let _ = self.rows.remove(pos);
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
            .map(|(id, v)| Hit::new(id.clone(), euclidean(query, v)))
            .collect();
        scored.sort_by(|a, b| a.distance.total_cmp(&b.distance));
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
            index_type: "flat",
            ..IndexStats::default()
        }
    }
}

impl Index for FlatIndex {
    type Config = ();
    fn new(dim: usize, _metric: DistanceMetric, _config: Self::Config) -> Result<Self> {
        if dim == 0 {
            return Err(IqdbError::InvalidConfig {
                reason: "dim must be greater than zero",
            });
        }
        Ok(FlatIndex::new(dim))
    }
}
