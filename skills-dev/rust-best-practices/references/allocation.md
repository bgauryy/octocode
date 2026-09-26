# Allocation — malloc/calloc/realloc/free, the Rust way

Load when translating C/C++ allocation habits, allocating big or zeroed buffers, handing memory across FFI, or tempted by `std::alloc`/`MaybeUninit`. Why: safe Rust already calls the allocator correctly and frees deterministically; hand-rolled allocation is where Rust code picks up C's bugs (leaks, double free, uninitialized reads).

## C → Rust map (safe first)
| C | Safe Rust | Notes |
|---|---|---|
| `malloc(sizeof T)` | `Box::new(t)` | freed on drop; no `free` call |
| `malloc(n * sizeof T)` (uninit) | `Vec::with_capacity(n)` then `push`/`extend` | capacity only; `len` stays 0 — can't read garbage |
| `calloc(n, size)` | `vec![0u8; n]` / `vec![0.0; n]` | std uses `alloc_zeroed` for zero values → OS may hand back lazily-zeroed pages, often cheaper than malloc+memset |
| `realloc` | `v.reserve(k)` / `reserve_exact` / automatic growth on `push` | amortized doubling; `shrink_to_fit` to give memory back |
| `free` | scope end / `drop(x)` | deterministic, one owner |
| `memset(p, 0, n)` | `buf.fill(0)` / `buf.clear()` | for secrets use `zeroize` (plain writes may be optimized out) |
| `memcpy` | `dst.copy_from_slice(src)` / `clone_from_slice` | bounds-checked, compiles to `memcpy` |
| `alloca` / big stack array | none — heap it (`vec!`/`Box<[T]>`) | large stack arrays overflow threads (2 MiB default spawned stack) |
| arena / pool | `bumpalo`, `typed-arena`; reuse buffers via `clear()` | reuse patterns in `references/performance.md`, arenas in `references/memory.md` |

## Uninitialized memory (only with a measured reason)
- Prefer zeroing (`vec![0; n]` is usually as fast). If you must: `Box::<[T]>::new_uninit_slice(n)` / `Vec::spare_capacity_mut()` + `MaybeUninit::write`, then `assume_init`/`set_len` only after **every** element is written — reading uninit memory is UB even for `u8`.
- Never `Vec::set_len` past initialized elements; never `mem::zeroed()` for types with invalid all-zero patterns (references, `NonZero`, enums, `Box`).
- Raw `std::alloc::{alloc, alloc_zeroed, realloc, dealloc}` with `Layout` is for implementing containers/allocators only: check for null (`handle_alloc_error`), dealloc with the *same* layout, `// SAFETY:` on each call, Miri-test it.

## Across FFI
- Memory is freed by the allocator that made it: C-allocated → C `free` (or the library's `*_free`); Rust-allocated → back to Rust. Mixing is UB.
- Hand ownership out with `Box::into_raw` and take it back with `Box::from_raw` exactly once (export a `my_free` fn). `CString::into_raw`/`from_raw` for strings.
- napi: return owned `String`/`Vec<u8>`/`Buffer` and let napi-rs copy/transfer — no raw pointers to JS.

## Leaks and failure
- Leaks are "safe" but real: `Rc` cycles (use `Weak`), `mem::forget`, `Box::leak` (fine for true `'static` config, not per request), unbounded caches/channels.
- Allocation failure aborts by default. Untrusted sizes: cap first, then `Vec::try_reserve(n)?` to turn OOM into an error instead of a crash.

Next: for footprint and allocator choice, load `references/memory.md`; for CPU-side allocation cuts, `references/performance.md`; for `unsafe` review rules, `references/safety-and-security.md`.
