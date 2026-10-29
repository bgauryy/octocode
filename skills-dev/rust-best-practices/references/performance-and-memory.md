Load when a hot path is slow or allocation-heavy, RSS or peak memory is high, a process OOMs, types look bloated, a structure holds millions of items, or you translate C allocation habits, allocate big/zeroed buffers, hand memory across FFI, or reach for `std::alloc`/`MaybeUninit`. Why: Rust's defaults hide allocation and layout costs that only a profile reveals.

# Performance and memory

## Measure first
- Never optimize on intuition.
- Microbenchmarks: `criterion`. Whole program: `cargo flamegraph`, `samply`, `perf`.
- Allocation counts/peaks: `dhat-rs` (drop-in global allocator, works in tests), `dhat`, `heaptrack`, Instruments/`samply` (macOS). Peak RSS: `/usr/bin/time -l` (macOS) / `-v` (Linux).
- Type sizes: `std::mem::size_of::<T>()`; pin hot types: `const _: () = assert!(size_of::<Node>() <= 32);`. Nightly `-Zprint-type-sizes` lists all layouts. Binary size (not heap): `cargo bloat --release --crates`.
- Baseline, change one thing, re-measure, keep only what moves the number. Tracked comparison: `octocode-eval-benchmark`.

## Cut allocations (biggest lever)
- Pre-size: `Vec::with_capacity(n)`, `String::with_capacity(n)`.
- Reuse buffers across a loop (`buf.clear()`), not one allocation per iteration. Fill an existing `String` or `Vec` with `clone_from` so its capacity survives.
- Known maximum length: `arrayvec::ArrayVec` (no heap). Mostly short, with a few long: `smallvec::SmallVec` skips the heap for the short case and checks inline-versus-heap on every operation. A large inline `N` is slower to move than `Vec`. Benchmark it ([heap allocations](https://nnethercote.github.io/perf-book/heap-allocations.html)).
- `mem::take`/`mem::replace` move out without cloning; `write!` into a pre-sized `String` beats repeated `format!`/`+`.

## Let the compiler work
- `#[inline]` only on tiny hot functions that cross crate boundaries.
- Hot paths: generics (monomorphization) over `Box<dyn Trait>`; `dyn` where code size or open extension matters more.
- Non-crypto maps (`ahash`/`FxHashMap`) only when hashing dominates. Deterministic output: `BTreeMap`/`BTreeSet`, or `HashMap` + explicit sort.
- CPU-bound data: `rayon` `.par_iter()`. Profile: thread overhead can exceed the gain on small workloads. Async I/O concurrency is `tokio` tasks, not CPU parallelism.
- Last resorts: manual field order only after profiling a layout problem; PGO for a measured workload. Use `target-cpu=native` only when deployment CPUs support the build machine's instructions; portable releases need an explicit supported CPU baseline. Release flags: `references/build-profiles.md`.

## C → Rust map
| C | Rust | Note |
|---|---|---|
| `malloc(sizeof T)` / `free` | `Box::new(t)` / scope end, `drop(x)` | one owner, no `free` call |
| `malloc(n * sizeof T)` | `Vec::with_capacity(n)` + `push`/`extend` | `len` stays 0; no garbage reads |
| `calloc(n, size)` | `vec![0u8; n]` | initializes `n` bytes to zero; allocator and compiler choices determine how |
| `realloc` | `reserve` / `reserve_exact` / growth on `push` | `shrink_to_fit` requests reduced capacity; the allocator may retain excess ([std](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.shrink_to_fit)) |
| `memset(p, 0, n)` | `buf.fill(0)` | `clear()` changes length, not retained bytes; secrets need `zeroize` because plain writes may be optimized out |
| `memcpy` | `copy_from_slice` for `Copy` elements; `clone_from_slice` for `Clone` elements | length-checked; generated code depends on the element type and optimizer |
| `alloca`, big stack array | `vec!` / `Box<[T]>` | thread stack size varies by platform and configuration; deep recursion → explicit work-stack |

## Shrink types
- `clippy::large_enum_variant`: enum layout reflects its variants plus alignment and any discriminant or niche. If a large variant inflates hot values, consider boxing that variant and measure the allocation cost against the smaller enum.
- Niches are free: `Option<Box<T>>`, `Option<&T>`, `Option<NonZeroU32>` add no bytes. Use `NonZero*` for ids.
- Right-size ints (`u32` index for 4-billion-bounded ids). Let the compiler reorder fields; `#[repr(C)]` only for FFI.
- Strings: immutable `Box<str>` (2 words) over `String` (3); shared `Arc<str>` over `Arc<String>`; short `compact_str`/`smol_str` (inline ≤24 B); repeated → intern to `u32` (`lasso`).
- Long-lived collections: `shrink_to_fit()` or `into_boxed_slice()` after building; `Vec` doubling can leave ~50% slack.

## Ownership shape
- Many short-lived allocations with one lifetime (AST, per-request scratch): an arena (`bumpalo`, `typed-arena`). One reset, no per-object free, `!Sync`. It is not a `#[global_allocator]`.
- Share, don't copy: `Arc<T>` read-mostly across threads, `Arc::make_mut` copy-on-write, `bytes::Bytes` for a shared byte buffer (a shallow clone, not a `String`), `Cow` at API edges. Do not `Arc` a value that is never shared.
- Zero-copy parse: `#[serde(borrow)] &'a str` fields; `serde_json::from_slice` over a buffer you own.

## Bound what grows
- Stream with `BufReader` and bound line/record size. A `.take(limit)` read can stop before EOF: detect overflow (for example, read up to checked `limit + 1`) and return an explicit limit error or continuation, never claim the prefix is complete. Large read-only files: `memmap2` (unsafe contract: the file must not be truncated underneath).
- Every cache has a cap and eviction (`lru`, `moka`). Channels are bounded (`tokio::sync::mpsc::channel(n)`, `crossbeam::bounded`). An unbounded channel can exhaust memory and abort the process.
- Leaks are safe but real: `Rc` cycles, `mem::forget`, `Box::leak` (only for true `'static` config, never per request).
- Allocation failure aborts by default. Untrusted sizes: cap first, then `Vec::try_reserve(n)?`.

## Uninitialized and raw memory (measured reason only)
- Prefer zeroing. Else `Box::<[T]>::new_uninit_slice(n)` / `Vec::spare_capacity_mut()` + `MaybeUninit::write`; `assume_init`/`set_len` only after **every** element is written — reading uninit memory is UB even for `u8`.
- Never `mem::zeroed()` for types with invalid all-zero patterns (references, `NonZero`, enums, `Box`).
- Raw `std::alloc::{alloc, alloc_zeroed, realloc, dealloc}` + `Layout` is for containers/allocators only: check null (`handle_alloc_error`), dealloc with the *same* layout.

## Across FFI
- The allocator that made memory frees it: C-allocated → C `free` / the library's `*_free`; Rust-allocated → Rust. Mixing is UB.
- Hand out with `Box::into_raw`; take back with `Box::from_raw` exactly once (export a `my_free` fn). Strings: `CString::into_raw`/`from_raw`.

## Global allocator (last lever)
The system allocator stays until a profile shows `malloc` hot. Then benchmark `tikv-jemallocator` (the maintained jemalloc binding; the older `jemallocator` crate is finished) or `mimalloc` on peak RSS and throughput. There is no universal winner: ripgrep enables jemalloc only for 64-bit musl. `#[global_allocator]` does not replace `malloc` inside libc. A library never sets the global allocator.
