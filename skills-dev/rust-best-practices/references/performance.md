# Performance — measure, then cut allocations

Load when a hot path is slow or allocation-heavy. Why: the common Rust performance mistakes are not exotic — they're needless allocations and clones hiding in code that looks idiomatic.

## Measure first (non-negotiable)
- Always benchmark and profile in `--release`; debug numbers are meaningless. The single most important habit is refusing to optimize on intuition.
- Microbenchmarks: `criterion`. Whole-program profile: `cargo flamegraph`, `samply`, `perf`. Allocation profile: `dhat`, `heaptrack`, or `dhat-rs`.
- Establish a baseline number, change one thing, re-measure, keep only what moves the number. Use `octocode-eval-benchmark` for a tracked comparison.

## Kill allocations (the biggest lever)
- Pre-size collections: `Vec::with_capacity(n)`, `String::with_capacity(n)` when the size is known/estimable.
- Reuse buffers across a loop instead of allocating per iteration (`buf.clear(); ...`).
- Take `&str`/`&[T]`, return `Cow<'_, str>` for "usually borrow, sometimes own" so the common path never allocates.
- `smallvec::SmallVec` / `arrayvec::ArrayVec` for small-N collections — stay on the stack, spill to heap only if they grow.
- `mem::take`/`mem::replace` move a value out without cloning; `write!` into a pre-sized `String` beats repeated `format!`/`+`.
- Avoid `.clone()` in hot loops; borrow, move, or index. `.collect()` into an intermediate `Vec` you immediately iterate again is often removable.

## Let the compiler do the work
- Iterators are zero-cost — chains compile to loops as good as or better than hand-written, with bounds checks elided when provable. Prefer them; don't reintroduce manual indexing "for speed."
- `#[inline]` on tiny hot functions crossing crate boundaries; trust the compiler otherwise.
- Prefer generics (monomorphization) over `Box<dyn Trait>` in hot paths to avoid vtable indirection; use `dyn` where code size or open extension matters more.
- Non-crypto maps: swap `std` hasher for `ahash`/`FxHashMap` when hashing dominates.

## Parallelism
- `rayon` turns `.iter()` into `.par_iter()` for CPU-bound data parallelism — near-free to try. But profile: for small workloads the thread overhead can exceed the gain.
- For async I/O concurrency use `tokio` tasks; don't confuse it with CPU parallelism (offload CPU-heavy work with `spawn_blocking` or rayon). See `references/gotchas.md` for blocking-in-async.

## Large graphs & data structures
Intern repeated strings, store graphs as indices, avoid whole-structure clones, and iterate instead of recursing on user-sized data — detail in `references/memory.md`. Deterministic output: `BTreeMap`/`BTreeSet`, or `HashMap` + explicit sort when hash speed matters.

## Layout & last resorts
- Struct field ordering / `#[repr(C)]` for cache behavior only after profiling shows a layout problem.
- Consider PGO / target-cpu=native for shipped hot binaries; document the build so it's reproducible.

Next: for the async-specific footguns behind slow "concurrent" code, load `references/gotchas.md`; for the profile flags that make release builds fast, `references/build-profiles.md`.
