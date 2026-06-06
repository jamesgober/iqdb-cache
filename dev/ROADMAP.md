# iqdb-cache -- Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.

---

## v0.1.0 -- Scaffold (DONE)

Compiles, CI green, structure correct, no domain logic.

- [x] Manifest, README, CHANGELOG, REPS, license, CI, lints in place.
- [x] API surface sketched in `docs/API.md`.

---

## v0.2.0 -- `CachedIndex` wrapper + LRU vector cache (DONE)

The drop-in `IndexCore` wrapper and the bounded, arena-backed LRU result cache,
with mutation-exact invalidation. Searches are memoized; every write invalidates.

Exit criteria:
- [x] Every public item has rustdoc + a runnable example.
- [x] Core invariants property-tested (transparency, no-stale-after-mutation,
  capacity bound) against a brute-force reference index.

---

## v0.3.0 -- result cache with TTL + invalidation (DONE)

Per-entry time-to-live via `clock-lib` (mockable, so TTL is tested
deterministically). `CacheConfig` builder + `with_config`. Mutation invalidation
stays exact; TTL bounds staleness from changes the wrapper cannot observe.

Exit criteria:
- [x] New surface tested (deterministic TTL unit tests + public-config
  integration suite) and benchmarked (`cache_hit_ttl`).

---

## v0.4.0 -- LFU/FIFO/ARC policies + stats + feature freeze (DONE)

`EvictionPolicy` (Lru/Lfu/Fifo/Arc) via `CacheConfig::policy`, all four built on
a shared arena `OrderedMap`. `CacheStats::evictions` counter. Per-policy
transparency + capacity property tests; per-policy hit benchmark.

Exit criteria:
- [x] No `todo!`/`unimplemented!`. **Feature freeze declared:** the public
  feature set (wrapper, TTL, four policies, stats) is complete; the run to 1.0
  is hardening, not new surface.

---

## v0.5.0 -- concurrency (loom) + API freeze (DONE)

`loom` model checks for the shared-cache path (two threads, every interleaving),
via a `--cfg loom` sync shim (`src/sync.rs`). `cargo audit` + `cargo deny` clean.

Exit criteria:
- [x] Public API frozen (recorded below). `cargo audit` + `cargo deny` clean.

### Frozen public API (1.x compatible from here)

The surface below is committed: no breaking changes until 2.0. Only additive,
non-breaking changes within 1.x. The `doc_stub` module is `#[doc(hidden)]` and
explicitly **not** part of the stable API.

- `iqdb_cache::VERSION: &str`
- `CachedIndex<I>` (where `I: iqdb_index::IndexCore`):
  - `new`, `with_capacity`, `with_config`
  - `capacity`, `ttl`, `policy`, `is_enabled`, `get_ref`, `into_inner`,
    `clear_cache`, `cache_stats`
  - `impl IndexCore for CachedIndex<I>` (transparent forwarding + memoization)
- `CacheConfig`: `new`, `capacity`, `ttl`, `no_ttl`, `policy`, `Default`
- `EvictionPolicy` (`#[non_exhaustive]`): `Lru` (default), `Lfu`, `Fifo`, `Arc`
- `CacheStats` (fields `hits`, `misses`, `evictions`, `len`, `capacity`):
  `lookups`, `hit_rate`

---

## v0.6.0 -> v0.9.x -- Alpha / Beta -> RC

- 0.6.x-0.7.x: integrate against real consumers; MINOR-compatible additions only.
- 0.8.x (beta): bug fixes; broader testing; final benchmarks.
- 0.9.x (rc): critical fixes + doc polish.

**v0.6.0 (DONE)** -- alpha / consumer integration. A `consumer_simulation` suite
drives the cache the way the real index crates will (realistic index, hot-set
query stream, mid-run read/write mix) and asserts transparency + a useful hit
rate under every policy; three runnable `examples/` ship. No API change.

**0.7-0.9 folded into 1.0 (move recorded per the anti-deferral rule).** Reason:
the crate already satisfies the Definition of Done -- feature-frozen at 0.4,
API-frozen at 0.5, `loom` + `cargo audit` + `cargo deny` clean, every public
item documented with a runnable example, hot paths benchmarked, and now
validated against a realistic consumer workload. A separate beta and rc cadence
would add version ceremony, not substance: there are no outstanding bugs, no
deferred hard tasks, and no further surface to stabilize. 1.0 follows directly.

---

## v1.0.0 -- Stable (DONE)

- [x] Definition of Done (DIRECTIVES section 7) satisfied.
- [x] Public API frozen until 2.0 (recorded under v0.5.0 above).
- [x] Release note written (`docs/release/v1.0.0.md`). Publish + tag: owner.

---

## Out of scope for 1.0

- Distributed/shard-aware caching -- different problem, lives elsewhere.
- Being a store -- it wraps an index.
