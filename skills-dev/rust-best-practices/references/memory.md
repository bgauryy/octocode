# Memory — footprint, layout, and allocation strategy

Load when RSS/peak memory is high, a process OOMs, types look bloated, or a data structure holds millions of items. Why: Rust has no GC to blame — every byte is a choice of type, container, or ownership, and most waste is fixable without `unsafe`. CPU-side allocation cuts live in `references/performance.md`.

## Measure first
- Allocation counts/peaks: `dhat-rs` (drop-in global allocator, works in tests), `heaptrack`, or Instruments/`samply` on macOS. Peak RSS: `/usr/bin/time -l` (macOS) / `-v` (Linux).
- Type sizes: `std::mem::size_of::<T>()`; pin hot types with a compile-time guard: `const _: () = assert!(size_of::<Node>() <= 32);`. Nightly `-Zprint-type-sizes` lists all layouts.
- Binary size (not heap): `cargo bloat`.

## Shrink types
- `clippy::large_enum_variant`: an enum is as big as its largest variant — `Box` the big one.
- Niches are free: `Option<Box<T>>`, `Option<&T>`, `Option<NonZeroU32>` cost no extra bytes. Use `NonZero*` for ids.
- Right-size ints (`u32` index, not `usize`, for 4-billion-bounded ids) and let the compiler reorder fields (don't `#[repr(C)]` unless FFI needs it).
- Immutable strings: `Box<str>` (2 words) over `String` (3); shared: `Arc<str>` over `Arc<String>` (one indirection). Short strings: `compact_str`/`smol_str` (inline ≤24 B). Repeated strings: intern to `u32` (`lasso`).
- Long-lived collections: `shrink_to_fit()` or `into_boxed_slice()` after building; `Vec` doubling can leave ~50% slack.

## Pick the right ownership shape
- Graphs/trees: index-based storage (`Vec<Node>` + `u32` ids, `slotmap`, `petgraph`) instead of `Rc<RefCell<…>>` webs — smaller, cache-friendly, no leaks. `Rc` cycles leak; break back-edges with `Weak`.
- Many short-lived allocations with one lifetime (AST, per-request scratch): arena (`bumpalo`, `typed-arena`) — one free, no fragmentation (oxc's allocator is this pattern).
- Share, don't copy: `Arc<T>` for read-mostly data across threads, `Arc::make_mut` for copy-on-write, `bytes::Bytes` for zero-copy buffer slicing, `Cow` at API edges.
- Zero-copy parse: `#[serde(borrow)] &'a str` fields; `serde_json::from_slice` over a buffer you own.

## Bound what grows
- Stream, don't slurp: `BufReader` + line/record iteration; cap `read_to_end` with `.take(limit)`. Large read-only files: `memmap2` (unsafe contract: the file must not be truncated underneath).
- Every cache needs a cap and eviction (`lru`, `moka`); an unbounded `HashMap` cache is a slow leak.
- Bounded channels (`tokio::sync::mpsc::channel(n)`, `crossbeam::bounded`) so producers feel backpressure.
- Big arrays go on the heap (`vec![0; n]`/`Box`), not the stack; deep recursion → explicit work-stack.

## Allocator (last lever, measure it)
Multithreaded, alloc-heavy binaries often gain from `mimalloc` or `tikv-jemallocator` (`#[global_allocator]`) — both speed and fragmentation. Benchmark peak RSS *and* throughput; a library crate must never set the global allocator for its users.

Next: for CPU-side allocation cuts and profiling, load `references/performance.md`; for resource-exhaustion as a security bug, `references/safety-and-security.md`.
