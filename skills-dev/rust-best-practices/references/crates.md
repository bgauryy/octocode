# Crates — the vetted stack

Load when choosing a library for a need or vetting a dependency already in `Cargo.toml`. Why: the ecosystem has converged on a small canon; picking off-canon costs maintenance and review trust.

## Before you add any crate
1. Can std or an already-present dependency do it? Prefer that.
2. Is it maintained (recent releases, open-issue triage), permissively licensed, and reasonable in transitive weight (`cargo tree`)?
3. Verify the actual API against upstream source with `octocode-research` — registry stars are not code evidence.
4. Adding a dependency needs consent (see SKILL lobby). Log why the canon default did not fit.

## The canon — reach here first

| Need | Default | Notes |
|------|---------|-------|
| Errors in a **library** | `thiserror` | Typed enum variants callers can `match`; implements `std::error::Error` |
| Errors in a **binary/app** | `anyhow` | One `Result`, `?` everywhere, `.context()` for breadcrumbs |
| Serialization | `serde` + format crate (`serde_json`, `toml`, …) | Derive-driven; the standard |
| Async runtime | `tokio` | Default unless embedded/single-purpose (`smol`, `async-std` are niche) |
| CLI args | `clap` (derive API) | Subcommands, help, validation from structs |
| Structured logging | `tracing` + `tracing-subscriber` | Async-aware spans; prefer over `log` in async code |
| Data parallelism | `rayon` | `.par_iter()`; benchmark on small workloads |
| HTTP client | `reqwest` | tokio-based; `rustls` feature to drop OpenSSL |
| Web server | `axum` | On tokio/tower/hyper; the current default |
| Regex | `regex` / `regex-automata` | Linear-time, no catastrophic backtracking |
| Date/time | `jiff` (new, recommended) or `time`/`chrono` | `jiff` has the cleanest API in 2026 |
| Small/stack collections | `smallvec`, `arrayvec` | Avoid heap for small N |
| Concurrent map | `dashmap` | Sharded concurrent `HashMap` |
| Faster hashing (non-crypto) | `ahash` / `rustc-hash` (`FxHashMap`) | For internal maps, not security |
| Iterator power tools | `itertools` | `chunk_by`, `dedup`, `cartesian_product`, … |

## Testing & bench canon
- `criterion` — statistical microbenchmarks (`cargo bench`).
- `insta` — snapshot tests (great for parsers/serializers/CLI output).
- `proptest` / `quickcheck` — property-based testing.
- `rstest` — parameterized fixtures/cases.
- `mockall` — trait mocking, only when a real fake is impractical.

## Decision heuristics
- **Library vs app is the top fork for errors:** libraries expose typed errors (`thiserror`) so callers can react; apps collapse to `anyhow`. A library that returns `anyhow::Error` forces the collapse on every consumer — avoid.
- **Feature-gate heavy optionals** (`default-features = false`, opt in) to keep compile time and attack surface down — see `references/build-and-deps.md`.
- **`rustls` over native TLS** for portability and static binaries where the platform allows.

Next: for how the chosen error crate shapes function signatures, load `references/idioms.md`; for feature/version wiring, `references/build-and-deps.md`.
