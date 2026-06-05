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

## v0.4.0 -- LFU/FIFO/ARC policies + stats + feature freeze

Exit criteria:
- [ ] No `todo!`/`unimplemented!`. Feature freeze declared.

---

## v0.5.0 -- concurrency (loom) + API freeze

Exit criteria:
- [ ] Public API frozen (recorded here). `cargo audit` + `cargo deny` clean.

---

## v0.6.0 -> v0.9.x -- Alpha / Beta -> RC

- 0.6.x-0.7.x: integrate against real consumers; MINOR-compatible additions only.
- 0.8.x (beta): bug fixes; broader testing; final benchmarks.
- 0.9.x (rc): critical fixes + doc polish.

---

## v1.0.0 -- Stable

- [ ] Definition of Done (DIRECTIVES section 7) satisfied.
- [ ] Public API frozen until 2.0.
- [ ] Release note written; published to crates.io; tag pushed.

---

## Out of scope for 1.0

- Distributed/shard-aware caching -- different problem, lives elsewhere.
- Being a store -- it wraps an index.
