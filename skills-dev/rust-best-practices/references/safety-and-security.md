# Safety & security — unsafe, untrusted input, release gates

Load when reviewing `unsafe`, validating untrusted input, or gating a release. Why: Rust's memory safety is only as strong as its `unsafe` blocks and its input handling; a resource-exhaustion or ReDoS bug is a safety bug even in safe code.

## unsafe
- Default to `#![forbid(unsafe_code)]` at the crate root where feasible; drop to `#![deny(unsafe_code)]` with `#[allow]` on the few audited modules otherwise.
- Every `unsafe` block gets a `// SAFETY:` comment stating the invariant that makes it sound and why it holds here. No invariant → not ready to ship.
- Keep `unsafe` minimal and wrapped in a safe API that upholds the invariant; don't leak raw pointers/lifetimes across the boundary.
- Run **Miri** (`cargo +nightly miri test`) on any crate with `unsafe` to catch UB, data races, and invalid aliasing the compiler can't.
- Prefer a vetted crate (`bytes`, `zerocopy`, `bytemuck`) over hand-rolled `transmute`/pointer casts.
- The authority on sound `unsafe` and aliasing rules is **The Rustonomicon** (`references/canonical-sources.md`) — cite it, don't reason about UB from memory.

## Untrusted input (safe code can still be unsafe)
- Validate and bound everything crossing a trust boundary: sizes, lengths, ranges, encodings. Reject early with a typed error.
- **DoS via unbounded resources** is the common real-world Rust vuln: cap request/body/file sizes; set timeouts on fetches and parses; bound channel capacity and in-flight work. An OOM from an attacker-sized input is a security bug.
- **Regex ReDoS:** use the `regex` crate (linear-time, backtracking-free). If you must use a PCRE-style engine (`pcre2`, `fancy-regex`) on untrusted patterns/inputs, treat catastrophic backtracking as in-scope and bound it.
- **SSRF / path traversal:** validate and canonicalize URLs/paths for anything that fetches or reads on behalf of a caller; allowlist hosts/roots rather than blocklisting.
- Never log secrets; scrub tokens/keys from error chains and debug output before they leave the process.

## Integer & arithmetic
- Debug builds panic on overflow; release wraps by default. For untrusted arithmetic use `checked_*` / `saturating_*` / `try_into()` explicitly — don't rely on the panic.

## Release gate (CI)
Block merge on all of:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test` (unit + integration + doc)
- `cargo audit` (RUSTSEC advisories)
- `cargo deny check` (licenses, bans, advisories, duplicates)
- Miri on unsafe-bearing crates; fuzz targets (`cargo fuzz`) for parsers/decoders handling untrusted bytes.

Next: for the dependency-audit tooling in depth, load `references/build-and-deps.md`; for the bounded-buffer patterns that also help performance, `references/performance.md`.
