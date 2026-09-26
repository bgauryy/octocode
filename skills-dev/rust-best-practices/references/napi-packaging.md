# napi-rs — binding design and per-platform npm packages

Load when writing `#[napi]` exports, laying out a napi binding crate, or shipping a `.node` via per-platform npm packages. Why: the napi boundary is slow to cross and easy to ship wrong; the platform-package pattern is standardized by `@napi-rs/cli` — deviate only with a reason. Verified against `@napi-rs/cli` source (`create-npm-dirs.ts`, `pre-publish.ts`), `napi-rs/package-template`, and `oxc/napi/parser`.

## Crate layout
- Thin binding crate over a pure core: oxc keeps `napi/parser` (`oxc_parser_napi`) apart from `crates/*`; it only converts JS ↔ core types. Core stays testable without Node.
- Binding `Cargo.toml`: `crate-type = ["cdylib", "lib"]` (drop `lib` if no Rust consumer), `[lib] test = false, doctest = false`, `napi`/`napi-derive` + `[build-dependencies] napi-build` with `build.rs` → `napi_build::setup()`.
- Allocator in a cdylib: oxc gates `mimalloc-safe` behind an `allocator` feature and adds `local_dynamic_tls` on Linux — a `dlopen`ed `.node` can't rely on initial-exec TLS.

## Export shape
- `#[napi(object)]` for plain data (copied field-by-field each crossing); `#[napi]` struct + `impl` for a handle JS holds (state stays in Rust).
- **Batch across the boundary.** Every call and every object/field conversion costs; return one result per job, not per item. Big trees: return a JSON string or a `Buffer` (oxc's raw transfer) and decode in JS.
- `Buffer` / `Uint8Array` for bytes; `String` for text; `Option<T>` → `undefined`; `Either<A, B>` for unions. Tighten TS with `#[napi(ts_args_type = …, ts_return_type = …)]` or `dtsHeaderFile`.
- Blocking work: `AsyncTask` (`Task` trait, runs on libuv's pool, supports `AbortSignal`) for short jobs; `async fn` on napi's tokio runtime for I/O; long CPU jobs → own rayon pool. Details in `references/ffi-and-interop.md`.
- Errors: return `napi::Result`, map core errors with `napi::Error::new(Status::InvalidArg, msg)`; never `panic!`/`unwrap` in exports.

## Per-platform package pattern (`@napi-rs/cli` v3)
Root `package.json`:
```jsonc
{ "name": "my-lib", "main": "index.js", "types": "index.d.ts",
  "files": ["index.js", "index.d.ts"],
  "napi": { "binaryName": "my-lib", "packageName": "@my-lib/binding",   // optional scope for platform pkgs
            "targets": ["x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-unknown-linux-gnu",
                        "x86_64-unknown-linux-musl", "aarch64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] },
  "scripts": { "build": "napi build --platform --release", "artifacts": "napi artifacts",
               "prepublishOnly": "napi prepublish -t npm" } }
```
Each generated `npm/<platform-arch-abi>/package.json` (from `napi create-npm-dirs`) has: `name: <pkg>-<platform>-<arch>[-abi]`, `os: [platform]`, `cpu: [arch]`, **`libc: ["glibc"|"musl"]` on Linux**, `main` + `files` = the single `<binaryName>.<triple>.node`, and copied `engines`/`license`/`repository`/`publishConfig`.
- **`optionalDependencies` are written by `napi prepublish`** as exact version pins; the template commits none. Hand-kept pins across N packages drift.
- The generated `index.js` loader tries the local `./<name>.<triple>.node` (dev) → `require('<pkg>-<triple>')`, detects musl, honors `NAPI_RS_NATIVE_LIBRARY_PATH`, falls back to WASI, and throws an aggregated error pointing at npm's optional-deps bug. Keep it; wrap it with an idiomatic `src/index.ts`.
- **Missing `libc`** makes managers that honor it (pnpm, Yarn Berry, recent npm) install *both* gnu and musl packages on Linux — double download, and a wrong pick if detection fails.
- Shipping a CLI executable too (esbuild/biome pattern): put it in the same platform package, resolve it with `require.resolve('<pkg>-<triple>/<bin>')` from a JS launcher, and verify the exec bit survives `npm pack`.
- Known user failure: lockfile made on another OS or `--omit=optional` → platform package absent. The loader error must say "reinstall without --omit=optional / delete lockfile".

## Release pipeline
- `napi create-npm-dirs` → `napi artifacts` → `napi prepublish -t npm`; no hand-written copy/version-sync scripts (oxc/rolldown release in ~4 lines).
- Gate: `publint` after build (oxc), `npm pack --dry-run` size check, and **load-test each `.node` on its target** (QEMU for foreign arches, alpine for musl) — a wrong-glibc binary only fails at the user's `require()`.
- Order and timing: platform packages before the root; wait until npm serves them (its malware scan can delay visibility up to ~90 min — rolldown polls `wait-for-npm-packages`).
- Version-gated auto-publish: compare local `package.json` with the registry's latest and publish only on change (oxc/rolldown/swc).
- Provenance: `permissions: id-token: write` + `npm publish --provenance` — users can't inspect a `.node`, so attest its build.
- Matrix: zig cross-compile (`napi build --use-napi-cross` / `-x`) or digest-pinned napi-rs docker images cover many Linux targets from one runner; per-target runners are sturdier for a small fixed set. Optional `wasm32-wasip1-threads` fallback only if browser/edge consumers exist.

Next: for the exact TS ↔ Rust type mapping, load `references/napi-types.md`; for panics, threads, and crate types at the boundary, `references/ffi-and-interop.md`; for binding-vs-core crate boundaries, `references/crate-boundaries.md`.
