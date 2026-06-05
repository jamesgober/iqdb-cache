//! Criterion benchmarks for `CachedIndex`.
//!
//! The headline number is the cache *hit*: a repeated search served from the
//! LRU versus the same search run against the wrapped index. The wrapped index
//! here is an intentionally heavy brute-force scan, so the gap reflects the work
//! a hit avoids.
//!
//! Run with `cargo bench`.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use iqdb_cache::CachedIndex;
use iqdb_index::{Index, IndexCore, IndexStats};
use iqdb_types::{DistanceMetric, Hit, IqdbError, Metadata, Result, SearchParams, VectorId};

/// A brute-force index whose `search` is deliberately non-trivial, so the
/// benchmark measures a realistic "work avoided on a hit".
struct BruteForce {
    dim: usize,
    rows: Vec<(VectorId, Arc<[f32]>)>,
}

impl IndexCore for BruteForce {
    fn insert(&mut self, id: VectorId, vector: Arc<[f32]>, _m: Option<Metadata>) -> Result<()> {
        self.rows.push((id, vector));
        Ok(())
    }
    fn delete(&mut self, id: &VectorId) -> Result<()> {
        match self.rows.iter().position(|(e, _)| e == id) {
            Some(pos) => {
                let _ = self.rows.remove(pos);
                Ok(())
            }
            None => Err(IqdbError::NotFound),
        }
    }
    fn search(&self, query: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
        let mut scored: Vec<Hit> = self
            .rows
            .iter()
            .map(|(id, v)| {
                let d: f32 = query
                    .iter()
                    .zip(v.iter())
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum();
                Hit::new(id.clone(), d)
            })
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
            index_type: "bruteforce",
            ..IndexStats::default()
        }
    }
}

impl Index for BruteForce {
    type Config = ();
    fn new(dim: usize, _m: DistanceMetric, _c: Self::Config) -> Result<Self> {
        Ok(BruteForce {
            dim,
            rows: Vec::new(),
        })
    }
}

fn build(dim: usize, n: usize) -> BruteForce {
    let mut idx = BruteForce::new(dim, DistanceMetric::Euclidean, ()).expect("valid dim");
    for i in 0..n {
        let v: Arc<[f32]> = (0..dim).map(|d| (i + d) as f32).collect();
        idx.insert(VectorId::from(i as u64), v, None)
            .expect("insert");
    }
    idx
}

fn bench_cache(c: &mut Criterion) {
    let dim = 64;
    let n = 10_000;
    let params = SearchParams::new(10, DistanceMetric::Euclidean);
    let query: Vec<f32> = (0..dim).map(|d| d as f32 + 0.5).collect();

    let mut group = c.benchmark_group("search_10k_dim64");

    // Baseline: every call runs the full scan (cache disabled).
    let uncached = CachedIndex::with_capacity(build(dim, n), 0);
    group.bench_function("uncached_scan", |b| {
        b.iter(|| black_box(uncached.search(black_box(&query), &params).expect("search")))
    });

    // Hit path: the same query, served from the LRU after one warm-up.
    let cached = CachedIndex::new(build(dim, n));
    let _warm = cached.search(&query, &params).expect("warm");
    group.bench_function("cache_hit", |b| {
        b.iter(|| black_box(cached.search(black_box(&query), &params).expect("search")))
    });

    group.finish();
}

criterion_group!(benches, bench_cache);
criterion_main!(benches);
