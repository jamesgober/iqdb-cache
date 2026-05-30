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
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>iqdb-cache</strong> is an in-process caching layer for hot vectors and search results. For large indexes that do not fit in RAM, a well-tuned cache turns most reads into memory reads.
    </p>
    <p>
        It wraps any `Index` as a `CachedIndex` and is disabled by default, so it is purely an opt-in optimization.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). Vector + result caching. LRU/LFU/FIFO/ARC. Off by default.
    </p>
    <blockquote>
        <strong>Status: pre-1.0, in active development.</strong> The public API is being designed across the 0.x series and frozen at <code>1.0.0</code>. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

<h2>What it does</h2>

- **Vector cache** &mdash; keep hot vectors in memory to avoid disk reads
- **Result cache** &mdash; optional query to top-k cache with TTL
- **Eviction policies** &mdash; LRU, LFU, FIFO, and ARC
- **Configurable** &mdash; size the cache or disable it entirely; off by default
- **Stats** &mdash; hit/miss counters for tuning


<br>

## Installation

```toml
[dependencies]
iqdb-cache = "0.1"
```

<br>

## Status

This is the <code>v0.1.0</code> scaffold: structure, tooling, and quality gates are in place; the implementation lands across the 0.x series per the <a href="./dev/ROADMAP.md"><code>ROADMAP</code></a> and <a href="./docs/API.md"><code>docs/API.md</code></a>.

<hr>
<br>

## Where It Fits

`iqdb-cache` is a Phase-5 embedded crate. It builds on:

- `iqdb-types` &mdash; core types
- `iqdb-index` &mdash; wraps any index
- `iqdb` &mdash; exposes caching via the database builder

It is unblocked once `iqdb-index` exists.

<br>

## Contributing

See <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for engineering standards and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

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
