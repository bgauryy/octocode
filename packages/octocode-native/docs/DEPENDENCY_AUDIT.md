# Native Rust dependency audit

Every direct dependency of the native crates and the job it owns. A passing unused-dependency scan does not prove that a dependency is needed, so each row names its job. Add a row when you add a dependency, and remove the row with it.

## Engine (`crates/engine`)

| Dependency | Owned job |
|---|---|
| `aho-corasick` | Literal prescans for secret detection and search classification |
| `grep-matcher`, `grep-regex`, `grep-pcre2`, `grep-searcher` | In-process text search: the Rust-regex lane, the opt-in PCRE2 lane (`regex:"pcre2"`), and per-file searching |
| `ignore` | Gitignore-aware traversal (the local walk policy) |
| `toml`, `globset` | Reading a Cargo workspace's `members`/`exclude` globs to find the language-server workspace root |
| `moka` | Bounded in-memory caches: parsed graph facts and language-server responses |
| `oxc_allocator`, `oxc_ast`, `oxc_ast_visit`, `oxc_parser`, `oxc_semantic`, `oxc_span` | JavaScript/TypeScript parsing, symbols, references, calls, and graph facts; complements Tree-sitter |
| `rayon` | Bounded parallel file scans |
| `regex`, `regex-syntax` | General matching and pre-validation of Rust-regex patterns |
| `serde`, `serde_json`, `serde-saphyr` | Typed DTOs, LSP and graph JSON, and YAML rule documents (not YAML source parsing) |
| `sha2`, `hex` | Content digests |
| `tokio` | Language-server processes, I/O, timeouts, and synchronization |
| `tree-sitter` and the grammars `-asm`, `-c`, `-c-sharp`, `-cpp`, `-cuda`, `-go`, `-java`, `-javascript`, `-python`, `-rust`, `-scala`, `-typescript` | The parser API and the 12 grammar families in the default build. Assembly, C++, C#, CUDA, and Scala are optional dependencies that the default `portable-default` feature enables (CUDA's size: [Footprint](#footprint-interpretation)) |
| `url` | LSP and file-URI handling |
| `which` | Language-server executable discovery |
| `libc` (Unix), `windows-sys` (Windows) | Language-server memory limits, process groups, and module paths |
| `criterion`, `proptest`, `tempfile` (development) | Benchmarks, property tests, and filesystem fixtures |

## Runtime (`crates/runtime`)

| Dependency | Owned job |
|---|---|
| `serde`, `serde_json`, `serde-saphyr`, `toml` | Contracts, configuration, provider payloads, rewrite rules, manifests, and output |
| `regex`, `regress` | Native patterns; `regress` runs the contract's ECMAScript patterns during validation |
| `url` | Provider endpoint and URI validation |
| `base64`, `sha2`, `hex`, `getrandom` | Encodings, hashes, snapshots, receipts, and random nonces |
| `aes-gcm` | Encrypted home credential files |
| `reqwest`, `bytes`, `futures-util` | Bounded HTTP providers and streaming responses |
| `secrecy`, `keyring-core` | Credential secrecy and the platform credential-store abstraction |
| `ignore`, `globset` | Local scopes and path filters for GitHub trees and history |
| `semver` | Registry version ranges in `artifactSearch` |
| `flate2` | Release-archive reading in `artifactSearch` |
| `html5ever` | Text extraction from captured URL sources |
| `imagesize` | Dimensions of captured PNG and JPEG images |
| `octocode-io`, `octocode-engine`, `octocode-github`, `octocode-contract-types` | Internal crates; the engine with its default features |
| `tokio`, `tokio-util` | Runtime lifecycle, cancellation, signals, and asynchronous tools |
| `libc` (Unix) | Process-group and low-level lifecycle controls |
| `apple-native-keyring-store` (macOS), `zbus-secret-service-keyring-store` (Linux), `windows-native-keyring-store` (Windows) | OS credential stores |
| `windows-sys` (Windows) | File-handle confinement and job-object process controls |
| `hyper`, `hyper-util`, `bitflags`, `fastrand`, `slab` (optional, `dev-unify` feature) | Unify development feature sets |
| `tempfile`, `wiremock` (development) | Filesystem and provider integration tests |
| `serde`, `serde_json` (build) | `build.rs` renders the configuration and tool-identity Rust from the `@octocodeai/config` contracts and embeds the tool contract |

## Other crates

- `octocode-github`: GitHub transport (`reqwest`, `bytes`, `futures-util`, `tokio`, `tokio-util`), `url`, `base64` (file contents), `sha2`/`hex` (cache keys), `getrandom` (retry jitter), `secrecy`, and payload serialization (`serde`, `serde_json`). Configuration, credential discovery, and keyrings stay in the runtime. Development: `tempfile`, `wiremock`.
- `octocode-contract-types`: the generated wire types' `serde`/`serde_json` derives and `regress` (their ECMAScript patterns).
- `octocode-io`: `reqwest` client policy, `getrandom`, `hex`, `libc`, and `tokio`. Development: `tempfile`.
- `octocode-cli`:
  - `clap` for arguments.
  - `flate2`, `zip`, `reqwest`, `futures-util` and `url` for managed language-server downloads and installs.
  - `hyper`, `hyper-util`, `http-body-util`, `bytes`, `tokio-util`, `getrandom`, `base64` and `subtle` for the private loopback server of `octocode config view`: one-time tokens, compared in constant time.
  - `serde`, `serde_json`, `serde-saphyr` and `toml` for MCP client configs and skill bundles.
  - `tempfile` to stage a skill install beside its destination, so a failed replace rolls back.
  - `junction` (Windows) for skill directory links; `libc` (Unix) for the warm server: detaching its session, its owner check, and file timestamps.
  - Development: `wiremock`.
- `octocode-runtime-napi`: `napi`, `napi-derive`, and `napi-build`. No other crate depends on N-API.

## Footprint interpretation

A same-source, same-profile N-API ablation on Darwin ARM64 (2026-09-20, release profile, stripped) measured the addon at 27,877,344 bytes without Assembly and CUDA. Enabling only Assembly added 16,528 bytes (+0.06%); enabling only CUDA added 7,116,704 bytes (+6.787 MiB, +25.53%). CUDA ships in `portable-default` regardless; it is the largest grammar. Turning it on by default (2026-10-18, Darwin ARM64, release, stripped) grew the CLI binary from 53,075,776 to 60,389,296 bytes and the addon from 48,012,848 to 55,203,072 bytes, about 6.9 MiB each. These are shipped-artifact sizes, not source or build-cache sizes.

## Duplicate versions

`cargo tree --workspace --all-features --duplicates` lists the version splits. They are transitive and no Tree-sitter grammar is duplicated; pinning cannot collapse them without patching upstream crates:

- `core-foundation` 0.9 and 0.10: 0.9 comes through `reqwest`'s `system-proxy` feature, which honors the OS proxy settings.
- `syn` 2 and 3: an upstream split between proc-macro crates.
- `base64` 0.22 and 0.23, `itertools` 0.13 and 0.15: development only, through `wiremock` and `criterion`.
