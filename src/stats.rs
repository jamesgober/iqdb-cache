//! Cache hit/miss accounting.

/// A point-in-time snapshot of a [`CachedIndex`](crate::CachedIndex)'s cache.
///
/// Returned by [`CachedIndex::cache_stats`](crate::CachedIndex::cache_stats).
/// `hits` and `misses` are monotonic counters over the cache's lifetime;
/// `len` and `capacity` describe its current occupancy. Use
/// [`hit_rate`](CacheStats::hit_rate) to turn the counters into a ratio for
/// tuning.
///
/// # Examples
///
/// ```
/// use iqdb_cache::CacheStats;
///
/// let stats = CacheStats {
///     hits: 75,
///     misses: 25,
///     len: 64,
///     capacity: 128,
/// };
/// assert_eq!(stats.lookups(), 100);
/// assert!((stats.hit_rate() - 0.75).abs() < f64::EPSILON);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheStats {
    /// Lookups served from the cache.
    pub hits: u64,
    /// Lookups that missed and fell through to the wrapped index.
    pub misses: u64,
    /// Entries currently held.
    pub len: usize,
    /// Maximum entries the cache will hold; `0` means caching is disabled.
    pub capacity: usize,
}

impl CacheStats {
    /// Total lookups observed: `hits + misses` (saturating).
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_cache::CacheStats;
    ///
    /// let stats = CacheStats { hits: 3, misses: 1, len: 4, capacity: 8 };
    /// assert_eq!(stats.lookups(), 4);
    /// ```
    #[inline]
    #[must_use]
    pub fn lookups(&self) -> u64 {
        self.hits.saturating_add(self.misses)
    }

    /// The fraction of lookups served from cache, in `0.0..=1.0`.
    ///
    /// Returns `0.0` when there have been no lookups, so the result is always
    /// finite and safe to display.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_cache::CacheStats;
    ///
    /// let warm = CacheStats { hits: 9, misses: 1, len: 10, capacity: 16 };
    /// assert!((warm.hit_rate() - 0.9).abs() < 1e-9);
    ///
    /// let cold = CacheStats { hits: 0, misses: 0, len: 0, capacity: 16 };
    /// assert_eq!(cold.hit_rate(), 0.0);
    /// ```
    #[inline]
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.lookups();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}
