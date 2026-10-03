# napi-rs: types, binding crate, per-platform packages

Load when you type `#[napi]` signatures, read a generated `.d.ts`, debug a JS↔Rust value mismatch, lay out a binding crate, or ship a `.node` via per-platform npm packages. Sources: napi-rs `PRIMITIVE_TYPES`/`KNOWN_TYPES`, `@napi-rs/cli`, `napi-rs/package-template`, `oxc/napi/parser`. Panics, threads, and boundary errors: `references/safety-and-ffi.md`.

## Types: scalars
| Rust | TS | Watch |
|---|---|---|
| `u8 u16 u32 i8 i16 i32 f32 f64` | `number` | `u32`/`i32`/`f64` are the safe defaults |
| `i64` | `number` | lossy beyond ±2^53; counts/timestamps only |
| `u64 usize isize u128 i128`, `BigInt` | `bigint` | **`usize` in a signature forces `bigint` on JS**: expose `u32`/`f64`, convert inside |
| `bool` | `boolean` | |
| `String`, `&str`, `char`, `PathBuf`, `OsString` | `string` | copies; `Utf16String`/`Latin1String` skip re-encoding |
| `()` | `undefined` | |
| `serde_json::Value` (`serde-json` feature) | `any` | escape hatch; prefer typed structs |

## Types: containers & absence
| Rust | TS |
|---|---|
| `Option<T>` argument / return / `#[napi(object)]` field | `T \| undefined \| null` / `T \| null` / `field?: T` |
| `Vec<T>`; `HashMap`/`BTreeMap`/`IndexMap`; `HashSet`/`BTreeSet` | `Array<T>` (copied per element); `Record<K, V>`; `Set<T>` |
| `Either<A, B>` … `Either26` | `A \| B …` |
| `Result<T>` (return) | `T`; `Err` throws an `Error` |
| `AsyncTask<T>`, `async fn -> T`, `Promise<T>` | `Promise<T>` |
| `Buffer`, `Uint8Array`, `Float64Array`… | matching typed array (no per-element conversion) |
| `External<T>` | `ExternalObject<T>`, an opaque Rust handle |
| `Function`/`ThreadsafeFunction`; `Date` (and `chrono` types) | `(args) => R`; `Date` |

## Types: structs & enums
- `#[napi(object)] struct` → TS `interface`, converted **by value** on every crossing: small DTOs only.
- `#[napi] struct` + `#[napi] impl` → TS `class`, data stays in Rust: stateful or large things (`#[napi(constructor)]`/`#[napi(factory)]`).
- Fieldless `#[napi] enum` → numeric TS `enum`; `#[napi(string_enum)]` → string literals (`string_enum = "lowercase"`, per-variant `#[napi(value = "…")]`).
- Enums **with data** → discriminated union: `#[napi(discriminant = "type")] enum Shape { Circle { r: f64 } }` → `{ type: 'Circle', r: number }`; use it instead of `Option`-soup objects.
- `#[napi(transparent)] struct Id(u32)` exposes the inner type. Rename with `js_name`; fields become camelCase.

## Own the `.d.ts` (generated, never mirrored)
- The generated `.d.ts` is the ABI truth. Add helper types via `napi.dtsHeaderFile` (oxc: `src-js/header.d.ts`) or `ts_args_type` / `ts_return_type` / `ts_type`, never a hand mirror.
- Type the error channel, not `Promise<unknown>` (rolldown `BindingResult<T>`). An idiomatic `src/index.ts` imports types *from* the binding.
- Drift guard: commit the generated `.d.ts`/loader, regenerate in CI, `git diff --exit-code`.
- `#[napi(strict)]` throws on wrong argument types; still validate NaN, negatives, and > u32 for sizes or indices.
- Dual-package hazard: oxc/rolldown ship ESM-only; a kept `require` path re-exports ESM state so the `.node` loads once.

## Binding crate
- `Cargo.toml` (crate type: `references/safety-and-ffi.md`): `[lib] test = false, doctest = false` (oxc `napi/parser`), `napi`/`napi-derive`, `[build-dependencies] napi-build`, `build.rs` → `napi_build::setup()`.
- Allocator: gate `mimalloc-safe` behind a feature with `local_dynamic_tls` on Linux; a `dlopen`ed `.node` can't use initial-exec TLS.
- **Batch across the boundary**: one result per job; for big trees return a JSON string or a `Buffer` and decode in JS.

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
`napi create-npm-dirs` makes each `npm/<platform-arch-abi>/package.json`: `name: <pkg>-<platform>-<arch>[-abi]`, `os`, `cpu`, **`libc` on Linux**, the one `<binaryName>.<triple>.node`, copied `engines`/`license`/`repository`/`publishConfig`.
- **`napi prepublish` writes `optionalDependencies`** as exact pins; commit none, since hand-kept pins drift.
- Keep the generated `index.js` loader (local `.node` → `require('<pkg>-<triple>')`, musl detection, `NAPI_RS_NATIVE_LIBRARY_PATH`, WASI fallback); wrap it in `src/index.ts`.
- **Missing `libc`** makes pnpm, Yarn Berry, and recent npm install both gnu and musl packages.
- A CLI executable shares the platform package (esbuild/biome), resolved by `require.resolve('<pkg>-<triple>/<bin>')`; check the exec bit survives `npm pack`.
- A foreign-OS lockfile or `--omit=optional` drops the platform package; the loader error must say "reinstall without --omit=optional / delete lockfile".

## Release pipeline
- `napi create-npm-dirs` → `napi artifacts` → `napi prepublish -t npm`; no hand-written copy/version-sync scripts.
- Gate: `publint`, `npm pack --dry-run` size, and **load-test each `.node` on its target** (QEMU, alpine for musl); a wrong-glibc binary fails only at `require()`.
- Publish platform packages first and wait until npm serves them (malware scan: up to ~90 min; rolldown `wait-for-npm-packages`). Publish only when local `package.json` differs from the registry's latest.
- Provenance: `permissions: id-token: write` + `npm publish --provenance`.
- Matrix: zig cross-compile (`napi build --use-napi-cross` / `-x`) or digest-pinned napi-rs images cover many Linux targets from one runner; per-target runners suit a small set. Add `wasm32-wasip1-threads` only for browser/edge consumers.
