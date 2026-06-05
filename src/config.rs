//! Cache configuration.

use core::time::Duration;

/// Default cache capacity: the number of distinct recent searches whose results
/// are kept resident when no explicit capacity is given.
pub(crate) const DEFAULT_CAPACITY: usize = 1024;

/// Tuning for a [`CachedIndex`](crate::CachedIndex) — the Tier-2 configured path.
///
/// Build one with [`CacheConfig::new`] and the chaining setters, then hand it to
/// [`CachedIndex::with_config`](crate::CachedIndex::with_config). Every setting
/// has a sensible default, so `CacheConfig::new()` alone is a valid config.
///
/// | Setting | Default | Meaning |
/// |---|---|---|
/// | [`capacity`](CacheConfig::capacity) | `1024` | Max distinct searches cached; `0` disables caching. |
/// | [`ttl`](CacheConfig::ttl) | none | Optional per-entry time-to-live; expired results are recomputed. |
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use iqdb_cache::{CacheConfig, CachedIndex};
///
/// let config = CacheConfig::new()
///     .capacity(4096)
///     .ttl(Duration::from_secs(30));
///
/// let cached = CachedIndex::with_config(iqdb_cache::doc_stub::stub_index(), config);
/// assert_eq!(cached.capacity(), 4096);
/// assert_eq!(cached.ttl(), Some(Duration::from_secs(30)));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheConfig {
    /// Maximum number of distinct cached searches.
    pub(crate) capacity: usize,
    /// Optional per-entry time-to-live.
    pub(crate) ttl: Option<Duration>,
}

impl CacheConfig {
    /// A configuration with the default capacity (1024) and no TTL.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_cache::CacheConfig;
    ///
    /// let config = CacheConfig::new();
    /// // Equivalent to `CachedIndex::new(..)`'s defaults.
    /// # let _ = config;
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            ttl: None,
        }
    }

    /// Sets the maximum number of distinct cached searches.
    ///
    /// A `capacity` of `0` disables caching: searches pass straight through.
    ///
    /// # Examples
    ///
    /// ```
    /// use iqdb_cache::CacheConfig;
    ///
    /// let config = CacheConfig::new().capacity(256);
    /// # let _ = config;
    /// ```
    #[must_use]
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets a per-entry time-to-live: a cached result older than `ttl` is
    /// treated as a miss and recomputed.
    ///
    /// TTL bounds staleness from changes the wrapper cannot observe (for
    /// example, the wrapped index mutated through another handle). Mutations
    /// *through* the wrapper already invalidate exactly, independent of TTL.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use iqdb_cache::CacheConfig;
    ///
    /// let config = CacheConfig::new().ttl(Duration::from_secs(60));
    /// # let _ = config;
    /// ```
    #[must_use]
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Clears any previously set TTL, so cached results never expire on time
    /// (only on mutation).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use iqdb_cache::CacheConfig;
    ///
    /// let config = CacheConfig::new().ttl(Duration::from_secs(60)).no_ttl();
    /// # let _ = config;
    /// ```
    #[must_use]
    pub fn no_ttl(mut self) -> Self {
        self.ttl = None;
        self
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::new()
    }
}
