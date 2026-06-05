<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>iqdb-cache</b>
    <br>
    <sub><sup>iQDB IN-PROCESS CACHE</sup></sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/iqdb-cache"><img alt="Crates.io" src="https://img.shields.io/crates/v/iqdb-cache"></a>
    <a href="https://crates.io/crates/iqdb-cache"><img alt="Downloads" src="https://img.shields.io/crates/d/iqdb-cache?color=%230099ff"></a>
    <a href="https://docs.rs/iqdb-cache"><img alt="docs.rs" src="https://img.shields.io/docsrs/iqdb-cache"></a>
    <a href="https://github.com/jamesgober/iqdb-cache/actions"><img alt="CI" src="https://github.com/jamesgober/iqdb-cache/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.87%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>iqdb-cache</strong> is an in-process caching layer for search results. For large indexes that do not fit in RAM, a well-tuned cache turns a repeated query into a memory read instead of a fresh scan.
    </p>
    <p>
        It wraps any <code>IndexCore</code> as a <code>CachedIndex</code> &mdash; itself a drop-in <code>IndexCore</code> &mdash; and is purely an opt-in optimization: a database is correct with no cache at all, and wrapping an index never changes <em>what</em> a search returns, only how fast a repeat returns.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.87+</strong> (Rust 2024 edition). LRU result cache. Mutation-exact invalidation. Optional TTL. Off by default.
    </p>
    <blockquote>
        <strong>Status: pre-1.0, in active development.</strong> The public API is being designed across the 0.x series and frozen at <code>1.0.0</code>. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

<h2>What it does</h2>

- **Transparent wrapper** &mdash; `CachedIndex<I>` implements `IndexCore`, so it slots in anywhere the wrapped index does, including behind `Box<dyn IndexCore>`
- **Result memoization** &mdash; identical searches (same query, same `SearchParams`) are served from an in-memory cache instead of re-running
- **Mutation-exact invalidation** &mdash; every `insert` / `insert_batch` / `delete` clears the cache, so a search **never** observes a stale result
- **Optional TTL** &mdash; give entries an expiry to bound staleness from changes the wrapper can't see; off by default, and verified deterministically with a mock clock
- **Bounded LRU** &mdash; an arena-backed least-recently-used cache with amortized `O(1)` lookup, insert, and eviction; the footprint never exceeds the configured capacity
- **Off by default** &mdash; size the cache, or disable it with capacity `0` for a pure passthrough to A/B the cache's effect without touching call sites
- **Hit/miss stats** &mdash; `CacheStats` exposes lifetime hit and miss counters plus a `hit_rate` for tuning
- **Zero `unsafe`** &mdash; the whole crate is `#![forbid(unsafe_code)]`

<br>

## Installation

```toml
[dependencies]
iqdb-cache = "0.3"
```

<br>

## Quick start

Wrap any index and let repeated searches come from memory:

```rust
use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams};

// `stub_index()` stands in for a real `iqdb-flat` / `iqdb-hnsw` index.
let cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(3, DistanceMetric::Cosine);

let cold = cached.search(&[1.0, 0.0, 0.0], &params).expect("search");
let warm = cached.search(&[1.0, 0.0, 0.0], &params).expect("search"); // served from cache
assert_eq!(cold, warm);

let stats = cached.cache_stats();
assert_eq!(stats.hits, 1);
assert_eq!(stats.misses, 1);
```

Size the cache, or disable it entirely:

```rust
use iqdb_cache::CachedIndex;

// Hold the 4096 most-recent distinct searches.
let sized = CachedIndex::with_capacity(iqdb_cache::doc_stub::stub_index(), 4096);
assert_eq!(sized.capacity(), 4096);

// Capacity 0 is a pure passthrough — useful for measuring the cache's effect.
let bypass = CachedIndex::with_capacity(iqdb_cache::doc_stub::stub_index(), 0);
assert!(!bypass.is_enabled());
```

A write invalidates the cache, so the next search reflects it — never a stale result:

```rust
use std::sync::Arc;

use iqdb_cache::CachedIndex;
use iqdb_index::IndexCore;
use iqdb_types::{DistanceMetric, SearchParams, VectorId};

let mut cached = CachedIndex::new(iqdb_cache::doc_stub::stub_index());
let params = SearchParams::new(10, DistanceMetric::Cosine);

let before = cached.search(&[0.0, 0.0, 0.0], &params).expect("search");
cached
    .insert(VectorId::from(42u64), Arc::from(&[0.0, 0.0, 0.0][..]), None)
    .expect("insert");
let after = cached.search(&[0.0, 0.0, 0.0], &params).expect("search");

// The new vector is visible immediately; the cached result was discarded.
assert_eq!(after.len(), before.len() + 1);
```

Give entries a time-to-live to bound staleness from changes made behind the wrapper's back — through a `CacheConfig` (the Tier-2 path):

```rust
use std::time::Duration;

use iqdb_cache::{CacheConfig, CachedIndex};

let config = CacheConfig::new()
    .capacity(4096)
    .ttl(Duration::from_secs(300)); // results reused within 5 min are hits

let cached = CachedIndex::with_config(iqdb_cache::doc_stub::stub_index(), config);
assert_eq!(cached.ttl(), Some(Duration::from_secs(300)));
```

<br>

## Errors

`CachedIndex` introduces no errors of its own: every fallible call forwards the
wrapped index's `iqdb_types::Result` unchanged. A search that errors is not
cached, so a later identical search re-runs against the index.

<br>

## Status

<code>v0.3.0</code> &mdash; the `CachedIndex` wrapper, the bounded LRU result cache
with mutation-exact invalidation, and an optional per-entry TTL (via `clock-lib`,
so expiry is tested deterministically with a mock clock). Every core invariant is
property-tested against a brute-force reference index (the cache is transparent; a
write is never stale), concurrent reads are covered, and the hit path is
benchmarked: on the reference machine a 10k-vector / dim-64 search costs **~234&nbsp;µs**
uncached versus **~238&nbsp;ns** from cache — a ~985&times; speedup — with a TTL
adding ~29&nbsp;ns for the clock read. Additional eviction policies (LFU / FIFO /
ARC) and `loom` concurrency model-checks land across the rest of the 0.x series
per the <a href="./dev/ROADMAP.md"><code>ROADMAP</code></a>. The full surface is
documented in <a href="./docs/API.md"><code>docs/API.md</code></a>.

<hr>
<br>

## Where It Fits

`iqdb-cache` sits above the index family and below the database. It builds on:

- `iqdb-types` &mdash; core types (`VectorId`, `Hit`, `SearchParams`, `DistanceMetric`, `Filter`)
- `iqdb-index` &mdash; the `IndexCore` trait it wraps
- `iqdb` &mdash; exposes caching via the database builder

It is unblocked today: its first-party dependencies (`iqdb-types`, `iqdb-index`, and `clock-lib` for TTL) are all stable at 1.0.

<br>

## Standards

Built to the iQDB Rust standard. See <a href="./REPS.md"><code>REPS.md</code></a> (Rust Efficiency &amp; Performance Standards) and <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for the engineering law and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

<br>

<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> &mdash; <a href="./LICENSE-APACHE">LICENSE-APACHE</a></li>
        <li><b>MIT License</b> &mdash; <a href="./LICENSE-MIT">LICENSE-MIT</a></li>
    </ul>
    <p>at your option.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
