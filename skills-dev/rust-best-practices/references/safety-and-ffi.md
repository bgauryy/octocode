# Safety, security, and FFI

Load when you review `unsafe`, validate untrusted input, build a Node addon (napi-rs), a C-ABI library (`cdylib`), a language binding, or wrap a C library (tree-sitter grammars, `cc`-built deps). Why: UB or a panic across a boundary crashes the host process, not just your crate. UB and aliasing authority: **The Rustonomicon** (`references/sources-and-crates.md`); cite it, do not reason about UB from memory. Allocation-level `unsafe` and memory across FFI: `references/performance-and-memory.md`. napi TS types and npm packaging: `references/napi.md`.

## unsafe
- Default to `#![forbid(unsafe_code)]` at the crate root. Otherwise `#![deny(unsafe_code)]` with `#[allow]` on the few audited modules.
- The `// SAFETY:` comment says why the invariant holds here; for FFI it states null-ness, lifetime, alignment, and thread-affinity. No invariant → not ready to ship.
- Keep `unsafe` minimal, wrapped in a safe API that upholds the invariant; do not leak raw pointers/lifetimes across the boundary.
- Run **Miri** (`cargo +nightly miri test`) on any crate with `unsafe`: it catches UB, data races, and invalid aliasing.
- Prefer a vetted crate (`bytes`, `zerocopy`, `bytemuck`) over hand-rolled `transmute`/pointer casts.

## Untrusted input
- Validate and bound everything crossing a trust boundary (sizes, lengths, ranges, encodings); reject early with a typed error. Values from the other side of an FFI boundary (sizes, indices, UTF-8) are untrusted input.
- **DoS via unbounded resources** is the common real-world Rust vuln: cap request/body/file sizes, time out fetches and parses, bound in-flight work. An OOM from an attacker-sized input is a security bug.
- **Regex ReDoS:** use `regex` (linear-time). A PCRE-style engine (`pcre2`, `fancy-regex`) on untrusted patterns/inputs needs a backtracking bound.
- **SSRF / path traversal:** canonicalize URLs/paths for anything that fetches or reads for a caller; allowlist hosts/roots, do not blocklist. Filesystem: `cap-std` (capability dirs) or canonicalize-then-`starts_with(root)`; reject symlink escapes.
- Secrets: never log them; scrub tokens/keys from error chains and debug output. Wrap in `secrecy::SecretString` (redacted `Debug`, zeroized on drop); compare tokens/MACs with `subtle` (constant-time), never `==`.
- Deserialization: reject unknown fields only where the input contract is closed; preserve documented extension fields. Size-cap bytes *before* parsing; keep `serde_json`'s default recursion limit. Octocode wire types follow `references/parsing-and-codegen.md`.
- Crypto: never hand-roll. `rustls`, `ring`/`aws-lc-rs`, RustCrypto crates; randomness from `getrandom`/`rand::rngs::OsRng`.
- Arithmetic: debug panics on overflow, release wraps. Untrusted arithmetic uses `checked_*` / `saturating_*` / `try_into()`. Security-critical binaries can keep `overflow-checks = true` in `[profile.release]`.

## Hardening lints (opt in per crate, in `[lints.clippy]`)
`undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, `indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `unwrap_used`/`expect_used`, `mem_forget`. Strictest on parsers and code touching untrusted bytes.

## Supply chain
- `build.rs` and proc-macros run arbitrary code at build time: review new ones like code you execute. `cargo geiger` counts `unsafe` in the dep tree; `cargo vet` records who reviewed what.
- `cargo auditable build` embeds the dep list so shipped binaries stay scannable. `deny.toml` bans duplicates, yanked crates, unknown registries, and git sources.

## FFI: panic must not cross the boundary
- A Rust panic reaching a non-unwinding ABI aborts; a foreign exception entering Rust there is undefined behavior. `C-unwind` permits unwinding but requires a compatible caller. [Rustonomicon FFI](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding).
- Where the host must survive, catch unwinding Rust panics inside the export and return an error. `catch_unwind` cannot catch aborts or safely handle foreign exceptions; `panic = "abort"` terminates the host.
- Return `napi::Result` (`references/napi.md`); map core errors with `napi::Error::new(Status::InvalidArg, msg)`. Never `panic!`/`unwrap()` in exports; enforce with `unwrap_used = "deny"` (this repo does).

## FFI: napi-rs threads
- **Async rule:** any call > ~1ms is `#[napi]` on an `async fn` (napi's tokio runtime, for I/O), an `AsyncTask` (`Task` trait, libuv's pool, supports `AbortSignal`) for short jobs, or CPU work on `spawn_blocking` / a dedicated pool. Never block Node's event loop.
- **Don't flood libuv's shared pool.** libuv threads also serve Node's fs/DNS/crypto. Route CPU-heavy parse/search work to your own `rayon`/Tokio pool; keep libuv for the thin async boundary. Bound concurrency at the JS API.
- **`ThreadsafeFunction` payloads must be `'static` owned data.** Convert to `String`/`Buffer`/owned structs before crossing the thread boundary; never store borrowed `Object<'env>`/`Function<'env>`.
- Isolate Node dependencies in a binding crate so runtime libraries build without Node. For an existing single-crate addon, a feature gate is an alternative.

## FFI: crate types, linking, data
- `crate-type = ["cdylib", "rlib"]`: `cdylib` for the loadable `.node`/`.so`, `rlib` so Rust consumers and integration tests link the same code (this repo uses exactly this); drop `rlib` without a Rust consumer.
- Wrapping C (tree-sitter grammars, compression, crypto): isolate the `-sys` / `cc`-built crate so a Rust edit doesn't retrigger the C compile; feature-gate optional grammars/backends.
- Generate C headers with `cbindgen` when exposing a C ABI.
- Prefer copying owned data (`String`, `Vec<u8>`, `Buffer`) over lending pointers. A borrowed slice that outlives the Rust call is a use-after-free.
