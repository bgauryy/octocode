# Sources and crates

Load when an idiom or API claim needs an authoritative anchor, or when you choose a library or vet one already in `Cargo.toml`. Why: memory and registry popularity are not evidence; upstream source and the canon are. `rustup doc` serves the canon offline. Vetting rules (std first, upstream source, consent) are in SKILL.md.

## Canonical sources

| Open question | Source | URL |
|---|---|---|
| Public API shape, naming, `must_use`, conversions, traits | **Rust API Guidelines** | https://rust-lang.github.io/api-guidelines/ |
| Idioms, ownership, errors, language model | **The Book** | https://doc.rust-lang.org/book/ |
| Exact semantics, "is this defined behavior?" | **The Reference** | https://doc.rust-lang.org/reference/ |
| `unsafe`, aliasing, UB, sound raw-pointer abstractions | **The Rustonomicon** | https://doc.rust-lang.org/nomicon/ |
| Profiles, features, workspaces, publishing | **The Cargo Book** | https://doc.rust-lang.org/cargo/ |
| A lint, diagnostic, or codegen knob | **The rustc Book** + lint index | https://doc.rust-lang.org/rustc/ |
| Edition migration (2015→2018→2021→2024) | **The Edition Guide** | https://doc.rust-lang.org/edition-guide/ |
| Toolchain, channels, targets, MSRV | **The rustup Book** | https://rust-lang.github.io/rustup/ |
| `cargo doc`, doctests, intra-doc links | **The rustdoc Book** | https://doc.rust-lang.org/rustdoc/ |
| std API and guarantees | **std docs** | https://doc.rust-lang.org/std/ |
| CLI apps | **Command Line Book** | https://rust-cli.github.io/book/ |
| Embedded | **Embedded Book** · **Discovery** · **Embedonomicon** | https://docs.rust-embedded.org/book/ · https://docs.rust-embedded.org/discovery/ · https://docs.rust-embedded.org/embedonomicon/ |
| WebAssembly | **wasm-bindgen Guide** (rustwasm org archived 2025; its book is frozen) | https://wasm-bindgen.github.io/wasm-bindgen/ |
| Onboarding | **Rust by Example** · **Rustlings** | https://doc.rust-lang.org/rust-by-example/ · https://github.com/rust-lang/rustlings |

- The API Guidelines and the Reference settle disputes.
- Toolchain baseline: install via **rustup**; `cargo new`/`build`/`run`/`test`/`doc`/`publish`; add deps with `cargo add`; `Cargo.lock` pins versions; `rustup update` keeps the toolchain current.
- If the canon and this skill disagree, the canon wins: fix the skill via `octocode-skills`.

## The vetted crate stack

These are candidates when the repository has no established choice. Reuse the existing dependency and error/config conventions first.

| Need | Default | Notes |
|------|---------|-------|
| Errors in a **library** | `thiserror` | Typed variants callers can `match` |
| Errors in a **binary/app** | `anyhow` | One `Result`, `?`, `.context()` |
| Serialization | `serde` + format crate (`serde_json`, `toml`, …) | Derive-driven |
| Async runtime | `tokio` | `smol`, `async-std` are niche |
| CLI args | `clap` (derive API) | |
| Structured logging | `tracing` + `tracing-subscriber` | Prefer over `log` in async code |
| Data parallelism | `rayon` | |
| HTTP client | `reqwest` | `rustls` feature drops OpenSSL; prefer `rustls` over native TLS where the platform allows |
| Web server | `axum` | On tokio/tower/hyper |
| Regex | `regex` / `regex-automata` | Linear-time, no catastrophic backtracking |
| Date/time | `jiff` (recommended) or `time`/`chrono` | |
| Small/stack collections | `smallvec`, `arrayvec` | |
| Concurrent map | `dashmap` | Sharded |
| Non-crypto hashing | `ahash` / `rustc-hash` (`FxHashMap`) | Not for security |
| Iterator tools | `itertools` | `chunk_by`, `dedup`, `cartesian_product` |

- Check transitive weight with `cargo tree`. Log why a canon default did not fit.

## Repository evidence
When this skill runs inside Octocode, recheck these owners before applying a local example. `<repo>` is the checkout root.

| Practice | Source under `<repo>` | Deciding evidence |
|---|---|---|
| Crate boundaries and host ownership | `packages/octocode-native/ARCHITECTURE.md`; `packages/octocode-native/scripts/check-crate-boundaries.cjs` | Runtime is an `rlib`; hosts own conversion, not tool execution |
| Resolver, LTO, codegen units, shared metadata | `packages/octocode-native/Cargo.toml` | Explicit resolver 2; thin LTO; engine regression note |
| Request admission and cleanup | `packages/octocode-native/crates/runtime/src/runtime/lifecycle.rs` | `RequestAdmission`, `RequestGuard`, cancellation and blocking-task join |
| Contract generation and field effects | `packages/octocode-native/ARCHITECTURE.md`; `packages/octocode-native/crates/runtime/tests/contract_field_effects.rs` | One generator; generated wire types; effect coverage |
| Native artifact staging | `packages/octocode-native/scripts/native-addon-utils.cjs` | `stageFile`, signing before rename, subprocess load check |
| Test selection and page replay | `packages/octocode-native/package.json`; `packages/octocode-native/crates/runtime/tests/runtime_page_replay.rs` | Test-only features; snapshots and source-edit replay |

Manifest timing notes are historical observations, not new benchmark results. Verify the current toolchain and rerun the relevant workload before claiming a gain.

Next: how the error crate shapes signatures → `references/idioms.md`.
