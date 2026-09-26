# napi types — TypeScript ↔ Rust mapping

Load when choosing Rust types for `#[napi]` signatures, reading a generated `.d.ts`, or debugging a JS↔Rust value mismatch. Why: the Rust type you pick *is* the TS type users get — `usize` silently becomes `bigint`, `i64` silently loses precision. Mapping verified against napi-rs source (`crates/backend/src/lib.rs` `PRIMITIVE_TYPES`, `typegen.rs` `KNOWN_TYPES`, `examples/napi`).

## Scalars
| Rust | TS | Watch |
|---|---|---|
| `u8 u16 u32 i8 i16 i32 f32 f64` | `number` | `u32`/`i32`/`f64` are the safe defaults |
| `i64` | `number` | lossy beyond ±2^53 — use for counts/timestamps only |
| `u64 usize isize u128 i128`, `BigInt` | `bigint` | **`usize` in a signature forces `bigint` on JS** — expose `u32`/`f64`, convert inside |
| `bool` | `boolean` | prefer an enum over boolean flags (`references/types-and-structs.md`) |
| `String`, `&str`, `char`, `PathBuf`, `OsString` | `string` | UTF-8 conversion copies; `Utf16String`/`Latin1String` avoid re-encoding |
| `()` | `undefined` | |
| `serde_json::Value` (`serde-json` feature) | `any` | escape hatch — prefer typed structs |

## Containers & absence
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

## Structs & enums
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

Next: for export shape and platform packaging, load `references/napi-packaging.md`; for panics/threads at the boundary, `references/ffi-and-interop.md`.
