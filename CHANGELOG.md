<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>CHANGELOG</b>
</h1>
<p>
  All notable changes to <code>iqdb-cache</code> will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

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
[Unreleased]: https://github.com/jamesgober/iqdb-cache/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jamesgober/iqdb-cache/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/iqdb-cache/releases/tag/v0.1.0
