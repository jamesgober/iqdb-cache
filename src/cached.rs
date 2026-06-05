//! The [`CachedIndex`] wrapper.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use iqdb_index::{IndexCore, IndexStats};
use iqdb_types::{DistanceMetric, Hit, Metadata, Result, SearchParams, VectorId};

use crate::key::ResultKey;
use crate::lru::LruCache;
use crate::stats::CacheStats;

/// Default cache capacity used by [`CachedIndex::new`]: the number of distinct
/// recent searches whose results are kept resident.
const DEFAULT_CAPACITY: usize = 1024;

/// A drop-in [`IndexCore`] wrapper that memoizes search results.
///
/// `CachedIndex` holds any `I: IndexCore` and forwards every call to it, with
/// one addition: identical [`search`](IndexCore::search) calls — same query
/// and same [`SearchParams`] — are served from an in-memory LRU cache instead
/// of re-running the search. Because it *is* an [`IndexCore`], it slots in
/// anywhere the wrapped index does, including behind `Box<dyn IndexCore>`.
///
/// ## Correctness
///
/// The cache never returns a stale result. Every mutation that can change the
/// search space — [`insert`](IndexCore::insert),
/// [`insert_batch`](IndexCore::insert_batch), and
/// [`delete`](IndexCore::delete) — invalidates the cache, so a search after a
/// write always recomputes against the current index. Operations that do not
/// change the result set ([`flush`](IndexCore::flush) and the read-only
/// accessors) leave the cache intact.
///
/// ## Opt-in
///
/// Caching is an optimization a caller chooses by wrapping an index; the
/// database leaves indexes unwrapped by default. Construct a cache that holds
/// a fixed number of recent searches with [`new`](CachedIndex::new) or
/// [`with_capacity`](CachedIndex::with_capacity). A capacity of `0` disables
/// caching entirely: every search passes straight through, which is useful for
/// A/B measuring the cache's effect without changing call sites.
///
/// ## Concurrency
///
/// `CachedIndex<I>` is `Send + Sync` whenever `I` is (which every `IndexCore`
/// is). Reads share the cache behind a [`Mutex`] held only for the lookup and
/// the insert — never across the wrapped search — so concurrent misses run the
/// underlying search in parallel rather than serializing on the lock.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use iqdb_cache::CachedIndex;
/// use iqdb_index::{Index, IndexCore, IndexStats};
/// use iqdb_types::{DistanceMetric, Hit, IqdbError, Metadata, Result, SearchParams, VectorId};
///
/// // A minimal index that returns one hit per search; enough to show the wrap.
/// struct Stub {
///     dim: usize,
///     metric: DistanceMetric,
///     ids: Vec<VectorId>,
/// }
///
/// impl IndexCore for Stub {
///     fn insert(&mut self, id: VectorId, _v: Arc<[f32]>, _m: Option<Metadata>) -> Result<()> {
///         self.ids.push(id);
///         Ok(())
///     }
///     fn delete(&mut self, id: &VectorId) -> Result<()> {
///         match self.ids.iter().position(|x| x == id) {
///             Some(pos) => { let _ = self.ids.remove(pos); Ok(()) }
///             None => Err(IqdbError::NotFound),
///         }
///     }
///     fn search(&self, _q: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
///         Ok(self.ids.iter().take(params.k).map(|id| Hit::new(id.clone(), 0.0)).collect())
///     }
///     fn len(&self) -> usize { self.ids.len() }
///     fn dim(&self) -> usize { self.dim }
///     fn metric(&self) -> DistanceMetric { self.metric }
///     fn flush(&mut self) -> Result<()> { Ok(()) }
///     fn stats(&self) -> IndexStats {
///         IndexStats { n_vectors: self.ids.len(), index_type: "stub", ..IndexStats::default() }
///     }
/// }
///
/// # fn main() -> Result<()> {
/// let stub = Stub { dim: 3, metric: DistanceMetric::Cosine, ids: vec![VectorId::from(1u64)] };
/// let mut cached = CachedIndex::new(stub);
///
/// let params = SearchParams::new(1, DistanceMetric::Cosine);
/// let first = cached.search(&[1.0, 0.0, 0.0], &params)?;  // miss: runs the search
/// let again = cached.search(&[1.0, 0.0, 0.0], &params)?;  // hit: served from cache
/// assert_eq!(first, again);
///
/// let stats = cached.cache_stats();
/// assert_eq!(stats.hits, 1);
/// assert_eq!(stats.misses, 1);
/// # Ok(())
/// # }
/// ```
pub struct CachedIndex<I> {
    /// The wrapped index every call forwards to.
    inner: I,
    /// The result cache, guarded for `&self` search access.
    cache: Mutex<LruCache<ResultKey, Box<[Hit]>>>,
    /// Configured capacity, mirrored here for `0`-means-disabled fast paths.
    capacity: usize,
    /// Lifetime count of cache hits.
    hits: AtomicU64,
    /// Lifetime count of cache misses.
    misses: AtomicU64,
}

impl<I: IndexCore> CachedIndex<I> {
    /// Wraps `inner` with a result cache of the default capacity (1024 recent
    /// searches).
    ///
    /// # Examples
    ///
    /// ```
    /// # use iqdb_cache::CachedIndex;
    /// # use iqdb_cache::doc_stub::stub_index;
    /// let cached = CachedIndex::new(stub_index());
    /// assert!(cached.is_enabled());
    /// ```
    #[must_use]
    pub fn new(inner: I) -> Self {
        Self::with_capacity(inner, DEFAULT_CAPACITY)
    }

    /// Wraps `inner` with a result cache that holds at most `capacity` recent
    /// searches.
    ///
    /// A `capacity` of `0` disables caching: searches pass straight through and
    /// nothing is stored.
    ///
    /// # Examples
    ///
    /// ```
    /// # use iqdb_cache::CachedIndex;
    /// # use iqdb_cache::doc_stub::stub_index;
    /// let cached = CachedIndex::with_capacity(stub_index(), 256);
    /// assert_eq!(cached.capacity(), 256);
    ///
    /// let bypass = CachedIndex::with_capacity(stub_index(), 0);
    /// assert!(!bypass.is_enabled());
    /// ```
    #[must_use]
    pub fn with_capacity(inner: I, capacity: usize) -> Self {
        Self {
            inner,
            cache: Mutex::new(LruCache::with_capacity(capacity)),
            capacity,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// The configured cache capacity. `0` means caching is disabled.
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Whether caching is active (`capacity > 0`).
    #[inline]
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.capacity > 0
    }

    /// Borrows the wrapped index.
    #[inline]
    #[must_use]
    pub fn get_ref(&self) -> &I {
        &self.inner
    }

    /// Unwraps the cache, returning the index it held.
    ///
    /// # Examples
    ///
    /// ```
    /// # use iqdb_cache::CachedIndex;
    /// # use iqdb_cache::doc_stub::stub_index;
    /// # use iqdb_index::IndexCore;
    /// let cached = CachedIndex::new(stub_index());
    /// let inner = cached.into_inner();
    /// assert_eq!(inner.dim(), 3);
    /// ```
    #[must_use]
    pub fn into_inner(self) -> I {
        self.inner
    }

    /// Drops every cached result, keeping the wrapped index untouched.
    ///
    /// Mutations invalidate automatically; call this only to force a cold cache
    /// (for example, after the wrapped index was changed through a handle other
    /// than this wrapper).
    pub fn clear_cache(&mut self) {
        match self.cache.get_mut() {
            Ok(cache) => cache.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }

    /// A snapshot of the cache's hit/miss counters and occupancy.
    #[must_use]
    pub fn cache_stats(&self) -> CacheStats {
        let len = self.lock_cache().len();
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            len,
            capacity: self.capacity,
        }
    }

    /// Locks the cache, recovering the guard if a previous holder panicked.
    ///
    /// A poisoned result cache is safe to keep using: a half-finished insert
    /// can at worst drop or duplicate a memoized entry, never corrupt a result,
    /// so recovery is preferable to propagating the panic.
    fn lock_cache(&self) -> std::sync::MutexGuard<'_, LruCache<ResultKey, Box<[Hit]>>> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Empties the cache through `&mut self` after a mutation.
    fn invalidate(&mut self) {
        // `&mut self` guarantees exclusive access, so no lock is contended.
        match self.cache.get_mut() {
            Ok(cache) => cache.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }
}

impl<I: IndexCore> IndexCore for CachedIndex<I> {
    fn insert(
        &mut self,
        id: VectorId,
        vector: std::sync::Arc<[f32]>,
        metadata: Option<Metadata>,
    ) -> Result<()> {
        let result = self.inner.insert(id, vector, metadata);
        if result.is_ok() {
            self.invalidate();
        }
        result
    }

    fn insert_batch(
        &mut self,
        items: Vec<(VectorId, std::sync::Arc<[f32]>, Option<Metadata>)>,
    ) -> Result<()> {
        // `insert_batch` is fail-fast and may apply partially, so any outcome
        // can have changed the search space: always invalidate.
        let result = self.inner.insert_batch(items);
        self.invalidate();
        result
    }

    fn delete(&mut self, id: &VectorId) -> Result<()> {
        let result = self.inner.delete(id);
        if result.is_ok() {
            self.invalidate();
        }
        result
    }

    fn search(&self, query: &[f32], params: &SearchParams) -> Result<Vec<Hit>> {
        if self.capacity == 0 {
            let _ = self.misses.fetch_add(1, Ordering::Relaxed);
            return self.inner.search(query, params);
        }

        let key = ResultKey::new(query, params);
        {
            let mut cache = self.lock_cache();
            if let Some(hits) = cache.get(&key) {
                let _ = self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(hits.to_vec());
            }
        }

        // Miss: run the search without holding the lock so concurrent misses
        // do not serialize on it.
        let hits = self.inner.search(query, params)?;
        let _ = self.misses.fetch_add(1, Ordering::Relaxed);
        {
            let mut cache = self.lock_cache();
            let _evicted = cache.put(key, hits.clone().into_boxed_slice());
        }
        Ok(hits)
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn metric(&self) -> DistanceMetric {
        self.inner.metric()
    }

    fn flush(&mut self) -> Result<()> {
        // Flush commits durable state without changing the searchable set, so
        // the cache stays valid.
        self.inner.flush()
    }

    fn stats(&self) -> IndexStats {
        self.inner.stats()
    }
}
