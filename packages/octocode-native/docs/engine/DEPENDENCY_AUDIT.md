# Native Rust dependency audit

This audit covers every direct dependency declared by the runtime and engine crates after the first-class language cutover. Last usage sweep: 2026-09-20 (grep-verified per-dependency; the 2026-09-19 `cargo +nightly udeps` pass missed a since-removed unused `zeroize`). A passing unused-dependency scan does not establish design necessity by itself, so the tables also name the owned job.

## Engine crate

| Dependency | Owned job | Decision |
|---|---|---|
| `aho-corasick` | Literal prescan for secret detection and bounded search classification | Keep |
| `ast-grep-core` | Structural rewrite matching, metavariables, captures, and replacement generation | Keep, optional under `embedded-ast-grep-rewrite` |
| `ast-grep-config` | YAML rewrite rules, constraints, transforms, and rewriters | Keep, optional under `embedded-ast-grep-rewrite` |
| `grep-matcher` | Shared matcher traits used by direct ripgrep integrations | Keep |
| `grep-pcre2` | Explicit opt-in PCRE2 search lane | Keep |
| `grep-regex` | Default linear-time Rust-regex search lane | Keep |
| `grep-searcher` | In-process ripgrep traversal/search execution | Keep; its transitive `memmap2` is required |
| `ignore` | Gitignore-aware filesystem traversal | Keep |
| `napi`, `napi-derive` | Engine addon ABI and asynchronous Node bindings | Keep, optional under `napi-addon` |
| `oxc_allocator`, `oxc_ast`, `oxc_codegen`, `oxc_minifier`, `oxc_parser`, `oxc_semantic`, `oxc_span` | Rich JavaScript/TypeScript parsing, symbols, graph facts, references, and minification | Keep; complementary to Tree-sitter |
| `rayon` | Bounded parallel file scans | Keep |
| `regex`, `regex-syntax` | Generic matching and pre-validation of Rust-regex patterns | Keep |
| `serde`, `serde_json` | Typed DTOs and N-API/LSP/graph JSON transport | Keep |
| `serde_yaml_ng` | YAML ast-grep rule documents | Keep; this is not YAML source parsing |
| `tokio` | LSP process, I/O, synchronization, timeout, and async filesystem lifecycle | Keep |
| `tree-sitter` | Canonical parser API and query execution | Keep |
| `tree-sitter-asm`, `tree-sitter-c`, `tree-sitter-c-sharp`, `tree-sitter-cpp`, `tree-sitter-cuda`, `tree-sitter-go`, `tree-sitter-java`, `tree-sitter-javascript`, `tree-sitter-python`, `tree-sitter-rust`, `tree-sitter-scala`, `tree-sitter-typescript` | The 12 first-class grammar families | Keep; Assembly, C++, C#, CUDA, and Scala remain feature-gated but are enabled by `portable-default` (`tree-sitter-large-grammars` groups C++, C#, and CUDA) |
| `url` | Validated LSP and file-URI handling | Keep |
| `which` | Trusted language-server executable discovery | Keep |
| `napi-build` (build) | N-API build setup when the addon feature is enabled | Keep, optional |
| `criterion`, `proptest` (development) | Benchmarks and property tests | Keep |

Removed direct dependencies: `ast-grep-language`, `grep`, `lightningcss`, `crossbeam-epoch`, `memmap2`, and the five removed grammar crates. `crossbeam-epoch` remains transitively reachable through Rayon/Crossbeam internals; `memmap2` remains transitively reachable through `grep-searcher`. Neither is a redundant direct dependency.

## Runtime crate

| Dependency | Owned job | Decision |
|---|---|---|
| `serde`, `serde_json`, `serde_yaml_ng`, `toml` | Contracts, configuration, provider payloads, and rendered output | Keep |
| `regex`, `regress` | Linear native patterns and isolated ECMAScript-compatible regex execution | Keep |
| `url` | Provider endpoint and URI validation | Keep |
| `clap` | Native CLI parsing and help | Keep |
| `base64`, `sha2`, `hex` | Provider encoding, hashes, snapshots, and integrity receipts | Keep |
| `flate2`, `zip` | Managed LSP archive extraction | Keep |
| `reqwest`, `bytes`, `futures-util` | Bounded HTTP providers and streaming responses | Keep |
| `secrecy`, `keyring-core` | Credential secrecy and platform-store abstraction (`secrecy` supplies zeroization transitively) | Keep |
| `octocode-engine` | Internal search, syntax, security, graph, minification, and LSP algorithms | Keep with explicit feature set; runtime policy remains separate |
| `tokio`, `tokio-util` | Runtime lifecycle, cancellation, signals, and asynchronous tools | Keep |
| `napi`, `napi-derive` | Runtime addon ABI | Keep, optional under `napi-addon` |
| `napi-build` (build) | N-API build setup when the addon feature is enabled | Keep, optional |
| `libc` (Unix) | Process-group and low-level lifecycle controls | Keep, target-specific |
| `apple-native-keyring-store` (macOS) | Keychain credential store | Keep, target-specific |
| `zbus-secret-service-keyring-store` (Linux) | Secret Service credential store | Keep, target-specific |
| `windows-sys`, `windows-native-keyring-store` (Windows) | Job-object/process controls and credential store | Keep, target-specific |
| `tempfile`, `wiremock` (development) | Filesystem and provider integration tests | Keep |

## Footprint interpretation

The Darwin ARM64 release engine addon decreased from 35,520,496 bytes to 27,651,104 bytes (−7.50 MiB, −22.2%). Its Mach-O `__text` section decreased from 10,004,732 bytes to 6,996,712 bytes (−2.87 MiB, −30.1%). These are shipped-artifact measurements, not Cargo registry source size or build-cache size.

A same-source, same-profile N-API feature ablation on 2026-09-20 measured the pre-addition 10-family feature set at **27,877,344 bytes**. Enabling only Assembly produced **27,893,872 bytes** (+16,528 bytes, +0.06%); enabling only CUDA produced **34,994,048 bytes** (+7,116,704 bytes, +6.787 MiB, +25.53%); enabling both produced **35,010,576 bytes** (+7,133,232 bytes, +6.803 MiB, +25.59%). Every arm included `napi-addon`, `pcre2`, C++, C#, and Scala and used the release profile with symbol stripping. This is the authoritative grammar delta; generated parser source sizes are not binary-size measurements.

`cargo-bloat 0.12.1` could not attribute the mixed `cdylib`/`rlib` engine target because it selected the rlib and rejected it. The release receipt therefore uses exact addon bytes and platform section sizes; crate-attributed bloat remains a CI/tooling follow-up rather than an invented comparison.

## Duplicate-version review

The post-cutover `cargo tree --workspace --all-features --duplicates` report contains no duplicate Tree-sitter grammar family. Remaining version splits are transitive ownership boundaries: `base64` (Hyper versus runtime/Reqwest), `core-foundation` (system configuration versus current security framework), `syn` (current proc macros versus a transitive next-major macro), `winnow` (TOML/config parser generations), and test-only splits from Proptest/Tempfile versus ast-grep (`bit-set`, `bit-vec`, `getrandom`). OXC's `itertools` split is upstream-owned. Direct version pinning cannot collapse these without changing or patching upstream packages, so no feature or version override was added.
