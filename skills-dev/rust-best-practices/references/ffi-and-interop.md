# FFI & interop — native addons, cdylib, unsafe boundaries

Load when building a Node addon (napi-rs), a C-ABI library (`cdylib`), a language binding, or wrapping a C library (tree-sitter grammars, `cc`-built deps). Why: the FFI boundary is where Rust's guarantees stop — panics, threads, and lifetimes all need explicit handling or they become UB, deadlocks, or crashes.

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
- Wrapping C (tree-sitter grammars, compression, crypto): isolate the `-sys` / `cc`-built crate so a Rust edit doesn't retrigger the C compile; feature-gate optional grammars/backends (see `references/build-and-deps.md`).
- Generate C headers with `cbindgen` when exposing a C ABI; keep the `unsafe extern` surface tiny and wrapped in a safe Rust API.

## Data across the boundary
- Prefer copying owned data (`String`, `Vec<u8>`, `Buffer`) over lending pointers; a borrowed slice that outlives the Rust call is a use-after-free.
- Validate and bound every value arriving from the other side (sizes, indices, UTF-8) — it is untrusted input (`references/safety-and-security.md`).
- Every `unsafe` FFI block gets a `// SAFETY:` note stating the invariant (null-ness, lifetime, alignment, thread-affinity) and why it holds.

## TypeScript integration — ship the generated surface, don't mirror it
The cohort (oxc, rolldown, lightningcss) converges on one shape: **commit the napi-generated binding + `.d.ts` as the single source of ABI truth, then layer idiomatic TS on top of it** — nobody hand-maintains a parallel `.d.ts` that mirrors the generated one.
- **Don't discard the generated loader.** `napi build` emits an `index.js`/`binding.cjs` that already does the full `require('@scope/pkg-<platform>')` cascade, three-way musl detection (`ldd` read → `process.report.sharedObjects` → `ldd --version`), a `NAPI_RS_NATIVE_LIBRARY_PATH` override, a version-check gate, and the canonical npm optional-deps error. Hand-rolling a platform resolver reinvents this and usually covers less. The one legitimate reason to hand-roll: **two addons in one npm package** (off napi-rs's one-addon-per-package path) — then generate a loader *per addon*, don't hand-write both.
- **Inject hand types via `napi.dtsHeaderFile`, not a separate mirror.** oxc uses `dtsHeaderFile: "src-js/header.d.ts"`; the generated `.d.ts` stays the ABI and the header adds narrowing. This removes a large hand-maintained `.d.ts` and any bespoke ABI-diff script.
- **Type the error/result channel.** Don't return `Promise<unknown>`. rolldown's `napi.dtsHeader` defines `BindingResult<T> = { errors: BindingError[] } | T` plus `MaybePromise<T>`/`Nullable<T>`; napi-rs generates `Promise<T>` for `async fn`/`AsyncTask` automatically.
- **Layer an idiomatic API over the binding** (`src/index.ts` importing types *from* the generated binding), rather than a parallel d.ts — rolldown's `src/api/*`, oxc's `wrap()` facade + separate `@oxc-project/types`.
- **ABI drift guard:** commit the generated `.cjs`/`.d.ts`, regenerate in CI, `git diff --exit-code`. Simpler and strictly more coverage than an AST callable-surface diff (which typically ignores return types / enum shapes).
- **Dual-package hazard:** oxc/rolldown ship ESM-only to avoid it. If you must keep a `require` condition (CJS bin launchers), make the ESM facade the sole owner of JS-level state and have CJS re-export it, so the single `.node` and any wrapper state load once.

## Distribution & release automation
- Ship prebuilt per-platform packages via `optionalDependencies` with **exact** version pins; the loader picks by `os`/`cpu`/`libc`. Exact pins turn an ABI mismatch into a clean loader failure instead of silently selecting a wrong binary. Include only the grammars/features that platform tier needs.
- **Use `@napi-rs/cli`'s `create-npm-dirs` → `artifacts` → `pre-publish` instead of hand-written copy + version-sync + a manual publish runbook.** `pre-publish` generates the root's `optionalDependencies` pins from one source at publish time and encodes platform-first ordering (oxc/rolldown do the whole release in ~4 lines). Hand-maintaining those pins across N platform packages is the drift smell — not the assembly script itself.
- **Version-gated auto-publish:** diff local `package.json` against `unpkg.com/<pkg>@latest` and publish only on change (oxc/rolldown/swc) — merging a version bump *is* the release, and unchanged re-publishes are blocked.
- **npm provenance:** `permissions: id-token: write` + `npm publish --provenance` (oxc/rolldown/rspack). Table stakes for an opaque `.node` — consumers can't inspect the binary, so attest its build.
- **Propagation-wait between binding and root publish.** npm's malware scan can hide a just-published binding for up to ~90 min; if the root that pins it ships first, a consumer `npm install` lands in a window where the binary 404s. rolldown gates the root publish on a `wait-for-npm-packages` poll.
- **Scale the matrix with zig cross-compile** (`napi build --target … --use-napi-cross` / `-x` via cargo-zigbuild) or pinned napi-rs docker images (digest-pinned) — one x64 Linux runner covers gnu/musl/arm/s390x. Matching-runner native builds (one runner per target) are legitimately more robust for a small fixed target set, but don't scale.
- **Load-test the `.node` on the target arch pre-publish** (QEMU for foreign arches, musl/alpine images) — a cross-compiled or wrong-glibc binary fails only at `require()` on the user's machine; presence-checking files doesn't catch it. napi-rs's own `test-release.yaml` is the reference.
- Optional: a `wasm32-wasip1-threads` build (emnapi, same matrix) as a browser/edge/WebContainer fallback — only if those consumers exist; the marginal cost is one matrix row.
- Size-tune the shipped `cdylib`: benchmark `opt-level = "s"`/`"z"` vs `3`, `lto = "fat"`, `strip = "symbols"` — grammar-heavy addons often shrink a lot for little runtime cost.

Next: for the pool/threading side, load `references/performance.md`; for input-bounds and unsafe review, `references/safety-and-security.md`; for feature-gating and profiles, `references/build-and-deps.md`.
