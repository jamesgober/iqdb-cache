//! Synchronization primitives, swappable for `loom` model checking.
//!
//! Under `--cfg loom` these resolve to loom's instrumented `Mutex` and
//! `AtomicU64`, so the model checker in `tests/loom_iqdb_cache.rs` can explore
//! every thread interleaving of the shared result cache. In a normal build they
//! are the plain `std` types with no overhead. `Ordering` is `std`'s in both
//! cases (loom re-exports it unchanged).
//!
//! Only the *contended* state is swapped — the cache `Mutex` and the hit / miss
//! / eviction counters. The clock handle stays a `std::sync::Arc`: it is a
//! read-only, stateless time source, not part of the concurrency model.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::AtomicU64;
#[cfg(loom)]
pub(crate) use loom::sync::{Mutex, MutexGuard};

#[cfg(not(loom))]
pub(crate) use std::sync::atomic::AtomicU64;
#[cfg(not(loom))]
pub(crate) use std::sync::{Mutex, MutexGuard};

pub(crate) use std::sync::atomic::Ordering;
