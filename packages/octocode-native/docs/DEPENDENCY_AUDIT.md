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
| `regex` | General matching and pre-validation of Rust-regex patterns |
| `regex-syntax` (build, development) | Deriving the secret detector's prescan literals from the pattern table at build time (`build.rs`), and generating pattern samples in its tests |
| `serde`, `serde_json` | Typed DTOs, LSP and graph JSON, and structural rule objects |
| `sha2`, `hex` | Content digests |
| `tokio` | Language-server processes, I/O, timeouts, and synchronization |
| `tree-sitter` and the grammars `-asm`, `-c`, `-c-sharp`, `-cpp`, `-cuda`, `-go`, `-java`, `-javascript`, `-python`, `-rust`, `-scala`, `-typescript` | The parser API and the 12 grammar families in the default build. Assembly, C++, C#, CUDA, and Scala are optional dependencies that the default `portable-default` feature enables (CUDA's size: [Footprint](#footprint-interpretation)) |
| `url` | LSP and file-URI handling |
| `which` | Language-server executable discovery |
| `libc` (Unix), `windows-sys` (Windows) | Language-server memory limits, process groups, and module paths |
| `criterion`, `proptest`, `tempfile` (development) | Benchmarks, property tests, and filesystem fixtures |
| `octocode-io` | The memory budget and memo the LSP discovery keeps its scans in, shared with every other in-process store (`memory`) |
| `octocode-test-support` (development) | The fixture grammar the fact-admission tests read (`tests/fact_admission.rs`) |

## Runtime base (`crates/runtime-base`)

| Dependency | Owned job |
|---|---|
| `serde`, `serde_json` | Contracts, configuration, provider payloads, and output |
| `regex`, `regress` | Native patterns; `regress` runs the contract's ECMAScript patterns during validation |
| `url` | Provider endpoint and URI validation |
| `base64`, `hex`, `getrandom` | Encodings, snapshots, receipts, and random nonces |
| `aes-gcm` | Encrypted home credential files |
| `reqwest`, `bytes`, `futures-util` | Bounded HTTP providers and streaming responses |
| `secrecy`, `keyring-core` | Credential secrecy and the platform credential-store abstraction |
| `globset` | Path filters for GitHub trees and history |
| `semver` | Registry version ranges in the artifact providers |
| `flate2` | Release-archive reading in the artifact providers |
| `octocode-io`, `octocode-engine`, `octocode-github`, `octocode-contract-types` | Internal crates; the engine with its default features |
| `tokio` | Provider I/O, request lifecycle, and synchronization |
| `libc` (Unix) | Process-group and low-level lifecycle controls |
| `apple-native-keyring-store` (macOS), `zbus-secret-service-keyring-store` (Linux), `windows-native-keyring-store` (Windows) | OS credential stores |
| `windows-sys` (Windows) | File-handle confinement and job-object process controls |
| `tempfile` (optional, `test-support`; development) | Fixture directories of the tests' shared fixtures (`test_support`), enabled only by dev builds |
| `sha2`, `wiremock` (development) | Digests and provider mocks in tests |
| `serde`, `serde_json` (build) | `build.rs` renders the configuration and tool-identity Rust from the `@octocodeai/config` contracts and embeds the tool contract |
| `octocode-test-support` (development) | The shared wiremock mounts (`mock_http`) of the provider tests |

## Tools shared (`crates/tools-shared`)

| Dependency | Owned job |
|---|---|
| `libc` (Unix) | Process-group, file-lock and low-level process controls |
| `windows-sys` (Windows) | Process-group, file-lock and low-level process controls |
| `sha2` (development) | Digests in tests |
| `tempfile` (development) | Filesystem fixtures |
| `base64`, `getrandom`, `hex` (optional) | Encodings, digests, and random nonces |
| `bytes`, `futures-util`, `reqwest` (optional) | Bounded HTTP and streamed responses |
| `crc32fast`, `imagesize`, `zune-jpeg`, `zune-png` (optional) | Dimensions of captured PNG and JPEG images, and padding small images for the classification provider |
| `encoding_rs` (optional) | Decoding the charset a captured page declares |
| `flate2` (optional) | Compressed state and archive reading |
| `globset`, `ignore` (optional) | Gitignore-aware walks and path globs |
| `html5ever` (optional) | Text extraction from captured URL sources |
| `memchr` (optional) | Byte scans over source text |
| `regex` (optional) | Native patterns |
| `secrecy` (optional) | Credential secrecy |
| `serde`, `serde_json`, `toml` (optional) | Payloads, manifests, and output |
| `tokio` (optional) | Asynchronous I/O, processes, timeouts, and synchronization |
| `url` (optional) | URL parsing and validation |
| `wiremock` (optional) | Mock HTTP servers in tests |
| `octocode-contract-types`, `octocode-engine`, `octocode-github`, `octocode-io`, `octocode-runtime-base` | Internal crates; the engine with its default features |
| `octocode-test-support` (development) | Fixtures for tests: the wiremock mounts and the conformance grammar |

## AST search (`crates/tool-ast`)

| Dependency | Owned job |
|---|---|
| `ignore` | Gitignore-aware walks and path globs |
| `memchr` | Byte scans over source text |
| `regex` | Native patterns |
| `serde`, `serde_json` | Payloads, manifests, and output |
| `tempfile` (development) | Filesystem fixtures |
| `octocode-contract-types`, `octocode-engine`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## Clasify (`crates/tool-clasify`)

| Dependency | Owned job |
|---|---|
| `futures-util` | Bounded HTTP and streamed responses |
| `hex` | Encodings, digests, and random nonces |
| `serde`, `serde_json` | Payloads, manifests, and output |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `tempfile` (development) | Filesystem fixtures |
| `octocode-contract-types`, `octocode-engine`, `octocode-github`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## Code graph (`crates/tool-graph`)

| Dependency | Owned job |
|---|---|
| `flate2` | Compressed state and archive reading |
| `getrandom`, `hex` | Encodings, digests, and random nonces |
| `memchr` | Byte scans over source text |
| `serde`, `serde_json`, `toml` | Payloads, manifests, and output |
| `tempfile` (development) | Filesystem fixtures |
| `wiremock` (development) | Mock HTTP servers in tests |
| `octocode-contract-types`, `octocode-engine`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |
| `octocode-test-support` (development) | Fixtures for tests: the wiremock mounts and the conformance grammar |

## Local tools (`crates/tool-local`)

| Dependency | Owned job |
|---|---|
| `flate2` | Compressed state and archive reading |
| `hex` | Encodings, digests, and random nonces |
| `regex` | Native patterns |
| `secrecy` | Credential secrecy |
| `serde`, `serde_json` | Payloads, manifests, and output |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `sha2` (development) | Digests in tests |
| `tempfile` (development) | Filesystem fixtures |
| `wiremock` (development) | Mock HTTP servers in tests |
| `octocode-contract-types`, `octocode-engine`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## LSP search (`crates/tool-lsp`)

| Dependency | Owned job |
|---|---|
| `futures-util`, `reqwest` | Bounded HTTP and streamed responses |
| `globset`, `ignore` | Gitignore-aware walks and path globs |
| `hex` | Encodings, digests, and random nonces |
| `memchr` | Byte scans over source text |
| `regex` | Native patterns |
| `serde`, `serde_json`, `toml` | Payloads, manifests, and output |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `url` | URL parsing and validation |
| `tempfile` (development) | Filesystem fixtures |
| `octocode-contract-types`, `octocode-engine`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## Package tools (`crates/tool-packages`)

| Dependency | Owned job |
|---|---|
| `base64`, `hex` | Encodings, digests, and random nonces |
| `flate2` | Compressed state and archive reading |
| `futures-util`, `reqwest` | Bounded HTTP and streamed responses |
| `regex` | Native patterns |
| `secrecy` | Credential secrecy |
| `semver` | Registry version ranges |
| `serde`, `serde_json` | Payloads, manifests, and output |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `url` | URL parsing and validation |
| `sha2` (development) | Digests in tests |
| `wiremock` (development) | Mock HTTP servers in tests |
| `octocode-contract-types`, `octocode-engine`, `octocode-github`, `octocode-io`, `octocode-runtime-base`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## GitHub tools (`crates/tool-github`)

| Dependency | Owned job |
|---|---|
| `libc` (Unix) | Process-group, file-lock and low-level process controls |
| `tempfile` (development) | Filesystem fixtures |
| `base64`, `hex` (optional) | Encodings, digests, and random nonces |
| `bytes`, `futures-util` (optional) | Bounded HTTP and streamed responses |
| `globset` (optional) | Gitignore-aware walks and path globs |
| `regex` (optional) | Native patterns |
| `serde`, `serde_json` (optional) | Payloads, manifests, and output |
| `tokio` (optional) | Asynchronous I/O, processes, timeouts, and synchronization |
| `url` (optional) | URL parsing and validation |
| `wiremock` (optional) | Mock HTTP servers in tests |
| `octocode-contract-types`, `octocode-engine`, `octocode-github`, `octocode-io`, `octocode-runtime-base`, `octocode-test-support`, `octocode-tools-shared` | Internal crates; the engine with its default features |

## Runtime facade (`crates/runtime`)

| Dependency | Owned job |
|---|---|
| `base64`, `hex` | Encodings, digests, and random nonces |
| `futures-util` | Bounded HTTP and streamed responses |
| `ignore` | Gitignore-aware walks and path globs |
| `regex` | Native patterns |
| `secrecy` | Credential secrecy |
| `serde`, `serde_json`, `toml` | Payloads, manifests, and output |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `url` | URL parsing and validation |
| `libc` (Unix) | Process-group, file-lock and low-level process controls |
| `windows-sys` (Windows) | Process-group, file-lock and low-level process controls |
| `toml` (build) | `build.rs` reads the path dependencies of the manifests the build identity hashes |
| `sha2` (development) | Digests in tests |
| `tempfile` (development) | Filesystem fixtures |
| `wiremock` (development) | Mock HTTP servers in tests |
| `getrandom`, `hyper`, `hyper-util` (optional, `dev-unify`) | Feature unification for development builds: the features the test selection's dev-dependencies turn on |
| `octocode-contract-types`, `octocode-engine`, `octocode-github`, `octocode-io`, `octocode-runtime-base`, `octocode-tool-ast`, `octocode-tool-clasify`, `octocode-tool-github`, `octocode-tool-graph`, `octocode-tool-local`, `octocode-tool-lsp`, `octocode-tool-packages`, `octocode-tools-shared` | Internal crates; the engine with its default features |
| `octocode-test-support` (development) | Fixtures for tests: the wiremock mounts and the conformance grammar |

## Test support (`crates/test-support`)

| Dependency | Owned job |
|---|---|
| `base64`, `hex` | Encodings, digests, and random nonces |
| `serde`, `serde_json`, `toml` | Payloads, manifests, and output |
| `sha2` | Digests in tests |
| `tokio` | Asynchronous I/O, processes, timeouts, and synchronization |
| `wiremock` | Mock HTTP servers in tests |
| `octocode-engine` | Internal crates; the engine with its default features |

## Other crates

- `octocode-github`: GitHub transport (`reqwest`, `bytes`, `futures-util`, `tokio`), `url`, `base64` (file contents), `sha2`/`hex` (cache keys), `secrecy`, payload serialization (`serde`, `serde_json`), and the contract types it reports (`octocode-contract-base`: error codes and client waits). Configuration, credential discovery, and keyrings stay in the runtime base. Development: `tempfile`, `wiremock`, and `octocode-test-support` (the shared wiremock mounts, `mock_http`).
- `octocode-contract-base`: the `serde` derives of the generated wire types the crates below `octocode-contract-types` name.
- `octocode-contract-types`: the generated wire types' `serde`/`serde_json` derives, `regress` (their ECMAScript patterns), and `octocode-contract-base`, whose types it re-exports.
- `octocode-io`: `reqwest` client policy, `getrandom`, `hex`, `libc`, and `tokio`. Development: `tempfile`.
- `octocode-cli`:
  - `clap` for arguments.
  - `flate2`, `zip`, `reqwest`, `futures-util` and `url` for managed language-server downloads and installs.
  - `hyper`, `hyper-util`, `http-body-util`, `bytes`, `tokio-util`, `getrandom`, `base64` and `subtle` for the private loopback server of `octocode config view`: one-time tokens, compared in constant time.
  - `rmcp` (`server`, `transport-io`) for `octocode mcp`, the stdio MCP server: protocol framing and both protocol eras; tool listing and execution stay in this crate and the runtime.
  - `serde`, `serde_json` and `toml` for MCP client configs and skill bundles.
  - `tempfile` to stage a skill install beside its destination, so a failed replace rolls back.
  - `junction` (Windows) for skill directory links; `libc` (Unix) for the warm server: detaching its session, its owner check, and file timestamps.
  - Development: `wiremock`.

## Footprint interpretation

A same-source, same-profile ablation on Darwin ARM64 (2026-09-20, release profile, stripped) measured the runtime library artifact at 27,877,344 bytes without Assembly and CUDA. Enabling only Assembly added 16,528 bytes (+0.06%); enabling only CUDA added 7,116,704 bytes (+6.787 MiB, +25.53%). CUDA ships in `portable-default` regardless; it is the largest grammar. Turning it on by default (2026-10-18, Darwin ARM64, release, stripped) grew the CLI binary from 53,075,776 to 60,389,296 bytes, about 6.9 MiB. These are shipped-artifact sizes, not source or build-cache sizes.

## Duplicate versions

`cargo tree --workspace --all-features --duplicates` lists the version splits. They are transitive and no Tree-sitter grammar is duplicated; pinning cannot collapse them without patching upstream crates:

- `core-foundation` 0.9 and 0.10: 0.9 comes through `reqwest`'s `system-proxy` feature, which honors the OS proxy settings.
- `syn` 2 and 3: an upstream split between proc-macro crates.
- `base64` 0.22 and 0.23, `itertools` 0.13 and 0.15: development only, through `wiremock` and `criterion`.
