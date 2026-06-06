# iqdb-cache &mdash; API Reference

> Complete reference for **every** public item in `iqdb-cache` as of
> **v0.4.0**: what it is, its parameters and return shape, the traits it
> implements, and worked examples for each use case.
>
> **Status: pre-1.0.** The public surface is being designed across the 0.x
> series and frozen at `1.0.0`. Sections marked _(planned)_ describe the
> intended surface as it lands. The `doc_stub` module is documentation-only
> scaffolding and is **not** part of the public API.

## Table of Contents

- [Overview](#overview)
- [Crate constants](#crate-constants)
  - [`VERSION`](#version)
- [Configuration](#configuration)
  - [`CacheConfig`](#cacheconfig)
  - [`EvictionPolicy`](#evictionpolicy)
- [The cache wrapper](#the-cache-wrapper)
  - [`CachedIndex`](#cachedindex)
  - [Construction &mdash; `new` / `with_capacity` / `with_config`](#construction)
  - [Searching through the cache](#searching-through-the-cache)
  - [Mutation &amp; invalidation](#mutation--invalidation)
  - [Time-to-live](#time-to-live)
  - [Introspection &mdash; `capacity`, `ttl`, `policy`, `is_enabled`, `get_ref`, `into_inner`, `clear_cache`](#introspection)
- [Statistics](#statistics)
  - [`CacheStats`](#cachestats)
  - [`cache_stats`](#cache_stats)
- [Errors](#errors)
- [Feature flags](#feature-flags)
- [Trait implementation matrix](#trait-implementation-matrix)

---

## Overview

`iqdb-cache` is an in-process caching layer that sits between the database and
an index. Its one type, [`CachedIndex`](#cachedindex), wraps any
`I: iqdb_index::IndexCore` and memoizes search results: a repeated search &mdash;
same query, same `SearchParams` &mdash; is served from an in-memory LRU cache
instead of re-running against the index.

The wrapper is **transparent**. `CachedIndex<I>` is itself an `IndexCore`, so it
drops in wherever the wrapped index does, and it never changes *what* a search
returns &mdash; only how fast a repeat returns. Every mutation invalidates the
cache, so a search can never observe a stale result.

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(3, DistanceMetric::Cosine);

let cold = cached.search(&[1.0, 0.0, 0.0], &params).expect("search"); // miss
let warm = cached.search(&[1.0, 0.0, 0.0], &params).expect("search"); // hit
assert_eq!(cold, warm);
```

---

## Crate constants

### `VERSION`

```rust
pub const VERSION: &str;
```

The crate version from `Cargo.toml` at compile time, as a `major.minor.patch`
string. Useful in diagnostics and version-skew checks across the iqdb family.

```rust
let v = iqdb_cache::VERSION;
assert_eq!(v.split('.').count(), 3);
```

---

## Configuration

### `CacheConfig`

```rust
pub struct CacheConfig { /* private */ }
```

The Tier-2 tuning surface: capacity and an optional TTL, set together and handed
to [`CachedIndex::with_config`](#cachedindex-with_config). Built with a chaining
builder; every setting has a default, so `CacheConfig::new()` alone is valid.
Implements `Debug`, `Clone`, `PartialEq`, and `Eq`.

| Method | Default | Effect |
|---|---|---|
| `CacheConfig::new()` | &mdash; | Capacity `1024`, no TTL, LRU. |
| `.capacity(n: usize)` | `1024` | Max distinct cached searches; `0` disables caching. |
| `.ttl(d: Duration)` | none | Per-entry time-to-live; expired results are recomputed. |
| `.no_ttl()` | &mdash; | Clears a previously set TTL. |
| `.policy(p: EvictionPolicy)` | `Lru` | Which entry to evict when full. |

All setters take `self` and return `Self`.

```rust
use std::time::Duration;

use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};

let config = CacheConfig::new()
    .capacity(4096)
    .ttl(Duration::from_secs(30))
    .policy(EvictionPolicy::Arc);

let cached = CachedIndex::with_config(iqdb_cache::doc_stub::stub_index(), config);
assert_eq!(cached.capacity(), 4096);
assert_eq!(cached.ttl(), Some(Duration::from_secs(30)));
assert_eq!(cached.policy(), EvictionPolicy::Arc);
```

### `EvictionPolicy`

```rust
#[non_exhaustive]
pub enum EvictionPolicy { Lru, Lfu, Fifo, Arc }
```

Which entry an eviction discards when the cache is full. All four keep the cache
within capacity and never affect *correctness* — only the hit rate. `Default` is
`Lru`. Implements `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash` (and
`serde` with the feature). `#[non_exhaustive]`, so `match` on it needs a `_` arm.

| Variant | Evicts | Best for |
|---|---|---|
| `Lru` (default) | the least-recently-used entry | shifting query hot-sets; the strongest general default |
| `Lfu` | the least-frequently-used entry (ties: LRU) | stable, skewed workloads where a few queries dominate |
| `Fifo` | the oldest *inserted* entry, ignoring access | uniform reuse; the cheapest policy |
| `Arc` | adaptively, balancing recency and frequency | workloads that shift between the two |

```rust
use iqdb_cache::{CacheConfig, CachedIndex, EvictionPolicy};

assert_eq!(EvictionPolicy::default(), EvictionPolicy::Lru);

// Pick LFU for a workload with a stable hot-set.
let cached = CachedIndex::with_config(
    iqdb_cache::doc_stub::stub_index(),
    CacheConfig::new().policy(EvictionPolicy::Lfu),
);
assert_eq!(cached.policy(), EvictionPolicy::Lfu);
```

---

## The cache wrapper

### `CachedIndex`

```rust
pub struct CachedIndex<I> { /* private */ }
```

A drop-in [`IndexCore`] wrapper that memoizes search results in a bounded LRU
cache. Generic over the wrapped index `I`; every method requires
`I: IndexCore`.

**Guarantees**

- **Transparency.** `CachedIndex<I>` implements `IndexCore`, forwarding every
  method to `I`. The results of `search` are identical to the wrapped index's.
- **No stale reads.** `insert`, `insert_batch`, and `delete` invalidate the
  cache. `flush` and the read-only accessors do not (they cannot change the
  result set).
- **Bounded.** The cache never holds more than its configured capacity; the
  least-recently-used entry is evicted to make room.
- **`Send + Sync`** whenever `I` is (which every `IndexCore` is). Concurrent
  searches take a short lock for the cache lookup/insert only, never across the
  wrapped search.

[`IndexCore`]: https://docs.rs/iqdb-index

### Construction

#### `CachedIndex::new`

```rust
pub fn new(inner: I) -> Self
```

Wraps `inner` with a result cache of the default capacity (1024 recent
searches). This is the Tier-1 path: one call, no tuning.

**Parameters**

- `inner: I` &mdash; the index to wrap. Ownership moves into the cache;
  retrieve it later with [`into_inner`](#into_inner).

```rust
use iqdb_cache::CachedIndex;

let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
assert!(cached.is_enabled());
assert_eq!(cached.capacity(), 1024);
```

#### `CachedIndex::with_capacity`

```rust
pub fn with_capacity(inner: I, capacity: usize) -> Self
```

Wraps `inner` with a cache holding at most `capacity` distinct recent searches.

**Parameters**

- `inner: I` &mdash; the index to wrap.
- `capacity: usize` &mdash; the maximum number of cached searches. **`0`
  disables caching**: every search passes straight through and nothing is
  stored, which is useful for measuring the cache's effect without changing call
  sites.

```rust
use iqdb_cache::CachedIndex;

// A 256-entry cache.
let sized = CachedIndex::with_capacity(iqdb_cache::doc_stub::stub_index(), 256);
assert_eq!(sized.capacity(), 256);

// A disabled cache: pure passthrough.
let bypass = CachedIndex::with_capacity(iqdb_cache::doc_stub::stub_index(), 0);
assert!(!bypass.is_enabled());
```

<a id="cachedindex-with_config"></a>
#### `CachedIndex::with_config`

```rust
pub fn with_config(inner: I, config: CacheConfig) -> Self
```

Wraps `inner` from a [`CacheConfig`](#cacheconfig) — the Tier-2 path that sets
capacity and an optional TTL together. `new` and `with_capacity` are thin
shortcuts over this.

```rust
use std::time::Duration;

use iqdb_cache::{CacheConfig, CachedIndex};

let cached = CachedIndex::with_config(
    iqdb_cache::doc_stub::stub_index(),
    CacheConfig::new().capacity(512).ttl(Duration::from_secs(30)),
);
assert_eq!(cached.capacity(), 512);
assert_eq!(cached.ttl(), Some(Duration::from_secs(30)));
```

### Searching through the cache

`CachedIndex` implements [`IndexCore`], so you search it exactly like any index.
The first time a `(query, params)` pair is seen it is a **miss** (the wrapped
search runs and the result is stored); an identical later search is a **hit**
(served from the cache).

```rust
pub fn search(&self, query: &[f32], params: &SearchParams) -> iqdb_types::Result<Vec<Hit>>
```

A search is keyed on the query (compared bit-for-bit) and every `SearchParams`
field that affects the outcome: `k`, `ef`, `metric`, and `filter`. Two searches
share a cached result only when all of these match, so a hit can never serve the
result of a different query.

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(5, DistanceMetric::Cosine);

let _miss = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");
let _hit = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");

// A different `k` is a different key — a fresh miss, not a stale hit.
let other = SearchParams::new(7, DistanceMetric::Cosine);
let _miss2 = cached.search(&[1.0, 0.0, 0.0], &other).expect("search");

let stats = cached.cache_stats();
assert_eq!(stats.hits, 1);
assert_eq!(stats.misses, 2);
```

`search_batch` is inherited from `IndexCore`: it loops over `search`, so each
query in the batch benefits from the cache automatically.

### Mutation & invalidation

The mutating `IndexCore` methods forward to the wrapped index and then keep the
cache honest:

```rust
pub fn insert(&mut self, id: VectorId, vector: Arc<[f32]>, metadata: Option<Metadata>) -> Result<()>
pub fn insert_batch(&mut self, items: Vec<(VectorId, Arc<[f32]>, Option<Metadata>)>) -> Result<()>
pub fn delete(&mut self, id: &VectorId) -> Result<()>
```

- `insert` and `delete` invalidate the cache **only when they succeed** &mdash; a
  failed insert (for example a duplicate id) changes nothing, so the cache is
  kept.
- `insert_batch` is fail-fast and may apply partially, so it **always**
  invalidates.
- `flush` does **not** invalidate: it commits durable state without changing the
  searchable set.

```rust
use std::sync::Arc;

use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

let mut cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(10, DistanceMetric::Cosine);

let before = cached.search(&[0.0, 0.0, 0.0], &params).expect("search");
cached
    .insert(VectorId::from(7u64), Arc::from(&[0.0, 0.0, 0.0][..]), None)
    .expect("insert");
// The cache was cleared by the insert: the next search sees the new vector.
let after = cached.search(&[0.0, 0.0, 0.0], &params).expect("search");
assert_eq!(after.len(), before.len() + 1);
```

### Time-to-live

When a [`CacheConfig::ttl`](#cacheconfig) is set, each cached result carries the
moment it was stored. On a lookup, an entry whose age has reached the TTL is
treated as a **miss** and recomputed; the fresh result replaces it. With no TTL
(the default), the clock is never read.

TTL and mutation invalidation are independent guarantees:

- **Mutation invalidation** is exact and immediate — a write through the wrapper
  drops the whole cache, so a search after a write is never stale.
- **TTL** bounds the age of an entry against changes the wrapper *cannot* see —
  for example, the wrapped index mutated through a different handle, or an
  external data source behind it.

The time source is `clock-lib`. Production uses a monotonic system clock; the
crate's own tests inject a mock clock so expiry is verified without sleeping.

```rust
use std::time::Duration;

use iqdb_cache::{CacheConfig, CachedIndex};
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

// A 5-minute TTL: a result reused within 5 minutes is a hit; after that it is
// recomputed even if nothing was written through the wrapper.
let cached = CachedIndex::with_config(
    iqdb_cache::doc_stub::stub_index(),
    CacheConfig::new().ttl(Duration::from_secs(300)),
);
let params = SearchParams::new(1, DistanceMetric::Cosine);
let _ = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");
assert_eq!(cached.ttl(), Some(Duration::from_secs(300)));
```

### Introspection

#### `capacity`

```rust
pub fn capacity(&self) -> usize
```

The configured maximum number of cached searches. `0` means caching is
disabled.

#### `ttl`

```rust
pub fn ttl(&self) -> Option<Duration>
```

The configured per-entry time-to-live, or `None` when results expire only on
mutation.

#### `policy`

```rust
pub fn policy(&self) -> EvictionPolicy
```

The configured [`EvictionPolicy`](#evictionpolicy).

#### `is_enabled`

```rust
pub fn is_enabled(&self) -> bool
```

`true` when `capacity > 0`.

#### `get_ref`

```rust
pub fn get_ref(&self) -> &I
```

Borrows the wrapped index for read-only access without disturbing the cache.

#### `into_inner`

```rust
pub fn into_inner(self) -> I
```

Consumes the wrapper and returns the index it held.

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;

let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let index = cached.into_inner();
assert_eq!(index.dim(), 3);
```

#### `clear_cache`

```rust
pub fn clear_cache(&mut self)
```

Drops every cached result, leaving the wrapped index untouched. Mutations
already invalidate automatically; call this only to force a cold cache (for
example after the wrapped index was changed through another handle).

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

let mut cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(1, DistanceMetric::Cosine);
let _ = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");
let _ = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");

cached.clear_cache();
assert_eq!(cached.cache_stats().len, 0);
```

---

## Statistics

### `CacheStats`

```rust
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub len: usize,
    pub capacity: usize,
}
```

A point-in-time snapshot of a cache. `hits`, `misses`, and `evictions` are
monotonic counters over the cache's lifetime; `len` and `capacity` describe its
current occupancy. `evictions` counts entries the policy discarded to make room.
Implements `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq` (and
`Serialize`/`Deserialize` with the `serde` feature).

#### `CacheStats::lookups`

```rust
pub fn lookups(&self) -> u64
```

Total lookups observed: `hits + misses` (saturating).

#### `CacheStats::hit_rate`

```rust
pub fn hit_rate(&self) -> f64
```

The fraction of lookups served from cache, in `0.0..=1.0`. Returns `0.0` when
there have been no lookups, so the result is always finite.

```rust
use iqdb_cache::CacheStats;

let stats = CacheStats { hits: 90, misses: 10, evictions: 5, len: 64, capacity: 128 };
assert_eq!(stats.lookups(), 100);
assert!((stats.hit_rate() - 0.9).abs() < 1e-9);
```

### `cache_stats`

```rust
pub fn cache_stats(&self) -> CacheStats
```

A method on [`CachedIndex`](#cachedindex). Returns a fresh [`CacheStats`]
snapshot. (Note the distinct name: `IndexCore::stats` returns the wrapped
index's `IndexStats`; `cache_stats` returns the *cache's* counters.)

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(1, DistanceMetric::Cosine);
let _ = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");
let _ = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");

let stats = cached.cache_stats();
assert_eq!(stats.hits, 1);
assert_eq!(stats.misses, 1);
assert!(stats.hit_rate() > 0.0);
```

---

## Errors

`CachedIndex` introduces **no errors of its own**. Every fallible method
forwards the wrapped index's `iqdb_types::Result` verbatim. A search that
returns `Err` is not cached, so a later identical search re-runs against the
index.

---

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `serde` | off | Derives `serde::{Serialize, Deserialize}` for [`CacheStats`](#cachestats), so cache metrics can be emitted to logs or telemetry. |

The crate is std-only and has no required runtime dependencies beyond its two
first-party crates, `iqdb-types` and `iqdb-index`, which are always pulled.

---

## Trait implementation matrix

| Type | `Debug` | `Clone` | `Copy` | `PartialEq` / `Eq` | `IndexCore` | `serde` |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| `CachedIndex<I>` | — | — | — | — | ✅ (when `I: IndexCore`) | — |
| `CacheConfig` | ✅ | ✅ | — | ✅ | — | — |
| `EvictionPolicy` | ✅ | ✅ | ✅ | ✅ (+ `Hash`) | — | ✅ (feature) |
| `CacheStats` | ✅ | ✅ | ✅ | ✅ | — | ✅ (feature) |

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
