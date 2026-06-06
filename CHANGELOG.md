# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

---

## [0.5.0] - 2026-06-05

Concurrency model-checking and the **API freeze**. The shared-cache path is now
verified with `loom`, and the public surface is committed: no breaking changes
until 2.0.

### Added

- `loom` model checks (`tests/loom_iqdb_cache.rs`) for concurrent `search`
  through `&self`, exploring every interleaving of two threads against one
  cache — consistent results, no deadlock, exact lookup accounting.
- `src/sync.rs`: a `--cfg loom` shim that swaps the cache `Mutex` and the
  counters for loom's instrumented types while leaving normal builds on `std`.

### Changed

- **Public API frozen** for the 1.x series (recorded in `dev/ROADMAP.md`). Only
  additive, non-breaking changes from here until 2.0.
- Mutation invalidation now clears through the cache lock rather than
  `Mutex::get_mut`, so the same path is exercised under loom.
- Declared the `loom`/`docsrs` build cfgs via `[lints.rust]` `check-cfg`.

### Security

- `cargo audit` and `cargo deny check` pass (advisories, bans, licenses, and
  sources all clean).

---

## [0.4.0] - 2026-06-05

Eviction policies and an eviction counter — and the **feature freeze**. The
cache now offers four eviction strategies behind one configuration knob, and the
public feature set is complete: the run to 1.0 is hardening, not new surface.

### Added

- `EvictionPolicy` (`Lru`, `Lfu`, `Fifo`, `Arc`) selectable via
  `CacheConfig::policy`, with `CachedIndex::policy` to read it back. LRU is the
  default.
  - **LRU / FIFO** share one arena-backed recency list (LRU promotes on access,
    FIFO does not).
  - **LFU** evicts the least-frequently-used entry, tie-broken by
    least-recently-used, with `O(1)` eviction via a min-frequency pointer.
  - **ARC** (Adaptive Replacement Cache) balances recency and frequency with
    ghost lists, adapting to the workload; occupancy stays within capacity.
- `CacheStats::evictions`: a lifetime counter of entries discarded by the policy.
- Property tests that every policy stays transparent (matches a reference index)
  and bounded under capacity pressure, an eviction-counter integration test, and
  a `policy_hit` benchmark across all four policies.

### Changed

- Reworked the cache internals onto a shared `OrderedMap` primitive (an
  arena-backed linked hash map) that all four policies compose. The default-LRU
  hit path is within ~5% of 0.3 (~250 ns vs ~238 ns at 10k/dim-64).
- **Feature freeze:** no `todo!`/`unimplemented!` anywhere; the public feature
  set is frozen. 0.5 freezes the API surface; 0.6+ is stabilization.

---

## [0.3.0] - 2026-06-05

Result-cache time-to-live. Cached results can now be given an expiry, so a
search served from cache is bounded in age — not just invalidated on mutation.
The time source is `clock-lib`, so TTL behaviour is verified deterministically
with a mock clock rather than `sleep`.

### Added

- `CacheConfig`: a Tier-2 builder for `capacity` and an optional `ttl`, plus
  `CachedIndex::with_config` to construct from it and `CachedIndex::ttl` to read
  the configured TTL back.
- Per-entry TTL on the result cache: an entry older than the configured `ttl` is
  treated as a miss and recomputed. Mutations still invalidate exactly,
  independent of TTL; with no TTL (the default) the clock is never consulted.
- Deterministic TTL unit tests using `clock-lib`'s `ManualClock` (expiry,
  boundary, and never-expire-without-TTL), an integration suite over the public
  `CacheConfig` surface, and a `cache_hit_ttl` benchmark.

### Changed

- Added the `clock-lib` 1.0 dependency (the iQDB time standard) for monotonic,
  mockable time.

---

## [0.2.0] - 2026-06-05

The first functional release: the `CachedIndex` wrapper and a bounded LRU result
cache, with mutation-exact invalidation. Caching stays a transparent, opt-in
optimization &mdash; a wrapped index returns exactly what the bare index does,
verified against a brute-force reference.

### Added

- `CachedIndex<I>`: a drop-in `iqdb_index::IndexCore` wrapper that memoizes
  search results. Identical searches (same query and `SearchParams`) are served
  from the cache; every `insert` / `insert_batch` / `delete` invalidates it so a
  search never observes a stale result.
- `CachedIndex::new` (default 1024-entry cache) and `CachedIndex::with_capacity`
  (sized, or `0` to disable), plus `capacity`, `is_enabled`, `get_ref`,
  `into_inner`, `clear_cache`, and `cache_stats`.
- A bounded, arena-backed LRU cache with amortized `O(1)` lookup, insert, and
  eviction, and zero `unsafe`.
- `CacheStats` (`hits`, `misses`, `len`, `capacity`) with `lookups` and
  `hit_rate`; optional `serde` derives behind the `serde` feature.
- `VERSION` crate constant.
- Property tests proving transparency, no-stale-after-mutation, and the capacity
  bound against a reference index; a concurrency test covering shared-`&self`
  searches; and a `criterion` benchmark of the hit path versus an uncached scan.
- `docs/API.md`: a complete reference for the public surface, with examples.

### Changed

- Dropped the no-op `std` feature; the crate is std-only (it builds on
  `iqdb-index`, which is) and now defaults to no optional features.
- Wired the first-party dependencies `iqdb-types` and `iqdb-index` (both 1.0).
- Added `Matt Callahan` to the crate authors.

---

## [0.1.0] - 2026-05-30

Initial scaffold and repository bootstrap. No domain logic yet &mdash; this release establishes the structure, tooling, and quality gates the implementation will be built on.

### Added

- `Cargo.toml` with crate metadata, Rust 2024 edition, MSRV 1.87.
- Dual `Apache-2.0 OR MIT` license files.
- `README.md`, `CHANGELOG.md`, and a documentation skeleton.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml` CI matrix; `deny.toml`, `clippy.toml`, `rustfmt.toml`.
- `dev/DIRECTIVES.md` and `dev/ROADMAP.md` (committed engineering standards + plan).
[Unreleased]: https://github.com/jamesgober/iqdb-cache/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/jamesgober/iqdb-cache/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/iqdb-cache/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/iqdb-cache/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/iqdb-cache/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/iqdb-cache/releases/tag/v0.1.0
