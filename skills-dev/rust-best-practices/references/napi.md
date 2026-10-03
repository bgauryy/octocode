# napi-rs: types, binding crate, per-platform packages

Load when you choose Rust types for `#[napi]` signatures, read a generated `.d.ts`, debug a JS↔Rust value mismatch, lay out a napi binding crate, or ship a `.node` via per-platform npm packages. Type mapping verified against napi-rs source (`crates/backend/src/lib.rs` `PRIMITIVE_TYPES`, `typegen.rs` `KNOWN_TYPES`, `examples/napi`); packaging verified against `@napi-rs/cli` source (`create-npm-dirs.ts`, `pre-publish.ts`), `napi-rs/package-template`, and `oxc/napi/parser`. Panics, threads, and errors at the boundary: `references/safety-and-ffi.md`.

## Types: scalars
| Rust | TS | Watch |
|---|---|---|
| `u8 u16 u32 i8 i16 i32 f32 f64` | `number` | `u32`/`i32`/`f64` are the safe defaults |
| `i64` | `number` | lossy beyond ±2^53 — use for counts/timestamps only |
| `u64 usize isize u128 i128`, `BigInt` | `bigint` | **`usize` in a signature forces `bigint` on JS** — expose `u32`/`f64`, convert inside |
| `bool` | `boolean` | prefer an enum over boolean flags (`references/types-and-patterns.md`) |
| `String`, `&str`, `char`, `PathBuf`, `OsString` | `string` | UTF-8 conversion copies; `Utf16String`/`Latin1String` avoid re-encoding |
| `()` | `undefined` | |
| `serde_json::Value` (`serde-json` feature) | `any` | escape hatch — prefer typed structs |

## Types: containers & absence
| Rust | TS |
|---|---|
| `Option<T>` — argument / return / `#[napi(object)]` field | `T \| undefined \| null` / `T \| null` / optional `field?: T` |
| `Vec<T>` | `Array<T>` (copied element-by-element) |
| `HashMap<K, V>` / `BTreeMap` / `IndexMap` | `Record<K, V>` |
| `HashSet<T>` / `BTreeSet` | `Set<T>` |
| `Either<A, B>` … `Either26` | `A \| B …` |
| `Result<T>` (return) | `T` — `Err` becomes a thrown `Error` |
| `AsyncTask<T>`, `async fn -> T`, `Promise<T>` | `Promise<T>` |
| `Buffer`, `Uint8Array`, `Float64Array`… | `Buffer`, matching typed array (no per-element conversion) |
| `External<T>` | `ExternalObject<T>` — opaque Rust handle JS can't inspect |
| `Function`/`ThreadsafeFunction` | `(args) => R` callback |
| `Date` (`chrono` feature types too) | `Date` |

## Types: structs & enums
- `#[napi(object)] struct` → TS `interface`; converted **by value** on every crossing (all fields walked). Use for small DTOs.
- `#[napi] struct` + `#[napi] impl` → TS `class`; JS holds a reference, data stays in Rust. Use for stateful/large things; mark the constructor `#[napi(constructor)]` or a factory `#[napi(factory)]`.
- `#[napi] enum` (fieldless) → numeric TS `enum` (0,1,2… or explicit values). `#[napi(string_enum)]` → string literals (`string_enum = "lowercase"`, per-variant `#[napi(value = "…")]`).
- Enums **with data** → discriminated union: `#[napi(discriminant = "type")] enum Shape { Circle { r: f64 }, Rect { w: f64, h: f64 } }` → `{ type: 'Circle', r: number } | …`. Model JS unions this way instead of `Option`-soup objects.
- `#[napi(transparent)] struct Id(u32)` → exposes the inner type, so newtypes cost nothing on the JS side.
- Rename with `js_name`; fields become camelCase automatically.

## Own the `.d.ts` (generated, never mirrored)
- The generated binding `.d.ts` is the ABI truth. Never hand-maintain a parallel mirror; add helper/narrowing types via `napi.dtsHeaderFile` (oxc: `src-js/header.d.ts`) or per-item `ts_args_type` / `ts_return_type` / `ts_type`.
- Type the error channel instead of `Promise<unknown>` — rolldown's header defines `BindingResult<T> = { errors: BindingError[] } | T`.
- Layer an idiomatic `src/index.ts` that imports types *from* the binding (rolldown `src/api/*`, oxc `wrap()` + `@oxc-project/types`).
- Drift guard: commit generated `.d.ts`/loader, regenerate in CI, `git diff --exit-code` — every Rust signature change becomes a reviewed TS change.
- `#[napi(strict)]` throws on wrong JS argument types instead of coercing; still validate numbers (NaN, negatives, > u32) before using them as sizes/indices.
- Dual-package hazard: oxc/rolldown ship ESM-only. If you keep a `require` path, the ESM facade owns JS-level state and CJS re-exports it so the `.node` and wrapper state load once.

## Binding crate
- Keep a thin binding crate over a pure core. oxc keeps `napi/parser` (`oxc_parser_napi`) apart from `crates/*`; it only converts JS ↔ core types, so the core stays testable without Node.
- Binding `Cargo.toml`: `crate-type = ["cdylib", "lib"]` (drop `lib` if no Rust consumer), `[lib] test = false, doctest = false`, `napi`/`napi-derive`, and `[build-dependencies] napi-build` with `build.rs` → `napi_build::setup()`.
- Allocator in a cdylib: oxc gates `mimalloc-safe` behind an `allocator` feature and adds `local_dynamic_tls` on Linux. A `dlopen`ed `.node` can't rely on initial-exec TLS.
- **Batch across the boundary.** Every call and every object/field conversion costs. Return one result per job, not per item. For big trees, return a JSON string or a `Buffer` (oxc's raw transfer) and decode in JS.

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
Each generated `npm/<platform-arch-abi>/package.json` (from `napi create-npm-dirs`) has `name: <pkg>-<platform>-<arch>[-abi]`, `os: [platform]`, `cpu: [arch]`, **`libc: ["glibc"|"musl"]` on Linux**, `main` + `files` = the single `<binaryName>.<triple>.node`, and copied `engines`/`license`/`repository`/`publishConfig`.
- **`napi prepublish` writes `optionalDependencies`** as exact version pins; the template commits none. Hand-kept pins across N packages drift.
- The generated `index.js` loader tries the local `./<name>.<triple>.node` (dev) → `require('<pkg>-<triple>')`, detects musl, honors `NAPI_RS_NATIVE_LIBRARY_PATH`, falls back to WASI, and throws an aggregated error that points at npm's optional-deps bug. Keep it; wrap it with an idiomatic `src/index.ts`.
- **Missing `libc`** makes managers that honor it (pnpm, Yarn Berry, recent npm) install *both* gnu and musl packages on Linux: double download, and a wrong pick if detection fails.
- Shipping a CLI executable too (esbuild/biome pattern): put it in the same platform package, resolve it with `require.resolve('<pkg>-<triple>/<bin>')` from a JS launcher, and verify the exec bit survives `npm pack`.
- Known user failure: a lockfile made on another OS, or `--omit=optional`, leaves the platform package absent. The loader error must say "reinstall without --omit=optional / delete lockfile".

## Release pipeline
- `napi create-npm-dirs` → `napi artifacts` → `napi prepublish -t npm`. No hand-written copy/version-sync scripts (oxc/rolldown release in ~4 lines).
- Gate: `publint` after build (oxc), `npm pack --dry-run` size check, and **load-test each `.node` on its target** (QEMU for foreign arches, alpine for musl). A wrong-glibc binary fails only at the user's `require()`.
- Publish platform packages before the root. Wait until npm serves them: its malware scan can delay visibility up to ~90 min (rolldown polls `wait-for-npm-packages`).
- Version-gated auto-publish: compare local `package.json` with the registry's latest and publish only on change (oxc/rolldown/swc).
- Provenance: `permissions: id-token: write` + `npm publish --provenance`. Users can't inspect a `.node`, so attest its build.
- Matrix: zig cross-compile (`napi build --use-napi-cross` / `-x`) or digest-pinned napi-rs docker images cover many Linux targets from one runner; per-target runners are sturdier for a small fixed set. Add a `wasm32-wasip1-threads` fallback only if browser/edge consumers exist.
