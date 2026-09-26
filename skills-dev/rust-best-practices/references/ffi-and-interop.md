# FFI & interop — native addons, cdylib, unsafe boundaries

Load when building a Node addon (napi-rs), a C-ABI library (`cdylib`), a language binding, or wrapping a C library (tree-sitter grammars, `cc`-built deps) — for the boundary's safety rules. napi TS types live in `references/napi-types.md`; npm packaging and release in `references/napi-packaging.md`. Why: the FFI boundary is where Rust's guarantees stop — panics, threads, and lifetimes all need explicit handling or they become UB, deadlocks, or crashes.

## Panic must not cross the boundary
- Unwinding across an FFI boundary is undefined behavior. Guard every `extern "C"` / exported entry point: either `std::panic::catch_unwind` at the boundary and convert to an error code, or build with `panic = "abort"`.
- napi-rs converts a Rust `Result::Err` into a thrown JS error for you — return `napi::Result`, don't `panic!`/`unwrap()`. Enforce with `unwrap_used = "deny"` (this repo does).

## napi-rs (Node addons)
- **Async rule:** any call > ~1ms should be `#[napi]` on an `async fn` or wrap CPU work in `spawn_blocking` / a dedicated pool — never block Node's event loop.
- **Don't flood libuv's shared pool.** libuv threads also serve Node's fs/DNS/crypto. Route CPU-heavy parse/search work to your own `rayon`/Tokio pool; keep libuv for the thin async boundary. Bound concurrency at the JS API.
- **`ThreadsafeFunction` payloads must be `'static` owned data** — convert to `String`/`Buffer`/owned structs before crossing the thread boundary; never store borrowed `Object<'env>`/`Function<'env>`.
- Feature-gate the addon (`#[cfg(feature = "napi-addon")]`) so the core crate still builds as a plain `rlib`/CLI without Node — keeps tests and reuse decoupled.

## Crate types & linking
- `crate-type = ["cdylib", "rlib"]` — `cdylib` for the loadable `.node`/`.so`, `rlib` so Rust consumers and integration tests link the same code. (This repo uses exactly this.)
- Wrapping C (tree-sitter grammars, compression, crypto): isolate the `-sys` / `cc`-built crate so a Rust edit doesn't retrigger the C compile; feature-gate optional grammars/backends (see `references/workspace-manifest.md`).
- Generate C headers with `cbindgen` when exposing a C ABI; keep the `unsafe extern` surface tiny and wrapped in a safe Rust API.

## Data across the boundary
- Prefer copying owned data (`String`, `Vec<u8>`, `Buffer`) over lending pointers; a borrowed slice that outlives the Rust call is a use-after-free.
- Validate and bound every value arriving from the other side (sizes, indices, UTF-8) — it is untrusted input (`references/safety-and-security.md`).
- Every `unsafe` FFI block gets a `// SAFETY:` note stating the invariant (null-ness, lifetime, alignment, thread-affinity) and why it holds.

Next: for `#[napi]` export shape, packaging, and release, load `references/napi-packaging.md`; for TS types, `references/napi-types.md`; for the pool/threading side, `references/performance.md`; for input-bounds and unsafe review, `references/safety-and-security.md`; for size/speed profiles, `references/build-profiles.md`.
