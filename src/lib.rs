//! # iqdb-cache
//!
//! An in-process caching layer for the HiveDB **iqdb** vector-database spine.
//! For indexes that do not fit in RAM, a well-tuned cache turns repeated reads
//! into memory reads. [`CachedIndex`] wraps any
//! [`IndexCore`](iqdb_index::IndexCore) and memoizes search results, while
//! staying a drop-in `IndexCore` itself — so it slots in anywhere the wrapped
//! index does, including behind `Box<dyn IndexCore>`.
//!
//! Caching is an opt-in optimization: a database is correct with no cache at
//! all (the default), and wrapping an index never changes the *results* a
//! search returns — only how fast a repeated search returns them.
//!
//! ## Tiers
//!
//! - **Tier 1 — the lazy path.** [`CachedIndex::new`] wraps an index with a
//!   sensible default capacity. That is the whole common case.
//! - **Tier 2 — the configured path.** [`CachedIndex::with_capacity`] sizes the
//!   cache (or disables it with `0`), and [`CachedIndex::with_config`] takes a
//!   [`CacheConfig`] to set capacity and an optional TTL together.
//! - **Tier 3 — the trait seam.** `CachedIndex<I>` implements
//!   [`IndexCore`](iqdb_index::IndexCore), so it composes with any index that
//!   does.
//!
//! ## Correctness
//!
//! The cache is invalidated on every mutation, so a search never observes a
//! stale result. See [`CachedIndex`] for the exact contract.
//!
//! ## Example
//!
//! ```
//! use iqdb_cache::CachedIndex;
//! use iqdb_index::IndexCore;
//! use iqdb_types::{DistanceMetric, SearchParams};
//!
//! // `stub_index()` stands in for a real `iqdb-flat` / `iqdb-hnsw` index.
//! let mut cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
//! let params = SearchParams::new(3, DistanceMetric::Cosine);
//!
//! let a = cached.search(&[1.0, 0.0, 0.0], &params).unwrap();
//! let b = cached.search(&[1.0, 0.0, 0.0], &params).unwrap();  // served from cache
//! assert_eq!(a, b);
//! assert_eq!(cached.cache_stats().hits, 1);
//! ```

#![deny(warnings)]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unused_must_use)]
#![deny(unused_results)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::unreachable)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod cached;
mod config;
mod key;
mod ordered;
mod policy;
mod stats;
mod sync;

pub use crate::cached::CachedIndex;
pub use crate::config::{CacheConfig, EvictionPolicy};
pub use crate::stats::CacheStats;

/// The version of this crate, taken from `Cargo.toml` at compile time.
///
/// Exposed so a consumer can report the exact `iqdb-cache` build it links
/// against — useful in diagnostics and version-skew checks across the iqdb
/// crate family.
///
/// # Examples
///
/// ```
/// let version = iqdb_cache::VERSION;
/// assert_eq!(version.split('.').count(), 3);
/// assert!(version.split('.').all(|part| !part.is_empty()));
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Documentation-only support: a tiny in-memory index used by the runnable
/// examples in this crate's rustdoc. Not part of the public API and exempt from
/// SemVer; do not depend on it.
#[doc(hidden)]
pub mod doc_stub {
    use std::sync::Arc;

    use iqdb_index::{Index, IndexCore, IndexStats};
    use iqdb_types::{DistanceMetric, Hit, IqdbError, Metadata, Result, SearchParams, VectorId};

    /// A minimal three-dimensional index that returns one zero-distance hit per
    /// stored id. Enough to demonstrate the cache wrapper, nothing more.
    pub struct DocStub {
        ids: Vec<VectorId>,
    }

    /// Builds a [`DocStub`] preloaded with a single vector.
    #[must_use]
    pub fn stub_index() -> DocStub {
        DocStub {
            ids: vec![VectorId::from(1u64)],
        }
    }

    impl IndexCore for DocStub {
        fn insert(&mut self, id: VectorId, _v: Arc<[f32]>, _m: Option<Metadata>) -> Result<()> {
            self.ids.push(id);
            Ok(())
        }
        fn delete(&mut self, id: &VectorId) -> Result<()> {
            match self.ids.iter().position(|x| x == id) {
                Some(pos) => {
                    let _removed = self.ids.remove(pos);
                    Ok(())
                }
                None => Err(IqdbError::NotFound),
            }
        }
        fn search(&self, _q: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
            Ok(self
                .ids
                .iter()
                .take(params.k)
                .map(|id| Hit::new(id.clone(), 0.0))
                .collect())
        }
        fn len(&self) -> usize {
            self.ids.len()
        }
        fn dim(&self) -> usize {
            3
        }
        fn metric(&self) -> DistanceMetric {
            DistanceMetric::Cosine
        }
        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
        fn stats(&self) -> IndexStats {
            IndexStats {
                n_vectors: self.ids.len(),
                index_type: "doc_stub",
                ..IndexStats::default()
            }
        }
    }

    impl Index for DocStub {
        type Config = ();
        fn new(_dim: usize, _metric: DistanceMetric, _config: Self::Config) -> Result<Self> {
            Ok(DocStub { ids: Vec::new() })
        }
    }
}
