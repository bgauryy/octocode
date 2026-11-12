# Runtime performance and memory

Load for latency, throughput, allocation, peak memory, layout or binary-size work. Establish correctness first. A suspected cost is a hypothesis until measured.

## Measure the deployed workload

1. Record the input/corpus, expected output, toolchain, target, features, optimization, allocator and cache/process state. Use release-equivalent settings for production claims; use the actual dev configuration when investigating dev behavior.
2. Pick the metric that answers the question: wall time, CPU, throughput, allocations, live/peak heap, RSS or binary size. These are not interchangeable. Include the serving process and children when a thin CLI delegates work.
3. Repeat a stable baseline, inspect noise and profile the relevant process. Change one factor, verify identical behavior, then compare under the same conditions. Do not quote universal speedup factors.

| Question | Candidate evidence |
|---|---|
| Where is CPU time spent? | Existing profiler; `samply`, `perf`/flamegraph or platform tools as supported. |
| What allocates or remains live? | Allocation profiler such as DHAT, heaptrack or Instruments. Sampling CPU alone does not measure heap allocation. |
| What is peak process memory? | Platform process accounting; state whether it includes children and allocator-retained pages. |
| Is a layout too large? | `size_of`, collection counts and measured access patterns on the supported target. |
| Is code size the issue? | Shipped artifact size and code-size tools; distinguish symbols from executable code. |

## Optimize the measured cost

- Prefer improving algorithms, access patterns or unnecessary work when the profile supports it. Do not assume allocations dominate every workload.
- Reserve when a trustworthy size estimate helps. `Vec` growth is unspecified; `clear()` retains capacity, and `shrink_to_fit()` is a request that may leave excess capacity. Repeated shrinking can create more allocations.
- Reuse buffers where ownership permits. `clone_from` can reuse storage depending on the type. `mem::take` transfers the old allocation away; it does not leave that capacity available in the original value.
- Consider inline storage only for measured small-value distributions. `SmallVec` can spill; `ArrayVec` has a fixed capacity whose overflow must follow the input contract. Large inline values cost stack space and movement.
- Use `Arc`, `Cow`, borrowed parsing or arenas when the ownership/lifetime contract fits. Check retention: one small view can retain a large backing allocation. Arena reset/drop semantics vary by allocator and stored type.
- Box a large enum variant only when the reduced layout/access cost outweighs allocation and indirection. Default Rust field layout and word size are not portable ABI promises; rely only on documented niche/layout guarantees.
- Use deterministic output deliberately: ordered collections or explicit sorting. A faster hasher is a candidate only when hashing is material and its collision-resistance policy fits the threat model; “non-cryptographic” does not mean safe for adversarial keys.
- Compare static/dynamic dispatch, parallel execution, inlining, LTO, target CPU and allocator changes rather than assuming they are faster. Avoid oversubscribing CPU pools. Portable releases must respect their CPU baseline.

## Memory correctness

| Operation | Obligation |
|---|---|
| `Vec::with_capacity(n)` | Capacity is not initialized length; initialize before reading. Validate untrusted sizes before allocation. |
| `clear`, zeroing or reuse | `clear` does not erase retained bytes. Follow the project's secret-erasure policy; ordinary writes may be optimized away. |
| `MaybeUninit`, spare capacity, `set_len` | Prove initialized, valid elements and correct cleanup on every partial-failure path. Read no uninitialized value, including bytes. |
| Raw allocation/deallocation | Correct layout, alignment, provenance, allocation-failure handling and allocator pairing. Prefer existing safe storage. |
| File mapping | Honor the mapping API's unsafe contract, including concurrent truncation/mutation risks. |
| Deep recursive data | Consider stack use during traversal and destruction, not just construction. |

Use fallible allocation where recoverable allocation failure is part of the contract. It does not make every allocation in the process recoverable. Bound retained caches, queued work and in-flight work through the owner's policy; report explicit refusal or continuation rather than silently dropping evidence.

For raw memory and FFI proofs, read [safety](safety-and-ffi.md). For build/optimization flags, read [profiles](build-profiles.md).

Sources: [Rust Performance Book: profiling](https://nnethercote.github.io/perf-book/profiling.html), [heap allocations](https://nnethercote.github.io/perf-book/heap-allocations.html), [Vec guarantees](https://doc.rust-lang.org/std/vec/struct.Vec.html#guarantees), [MaybeUninit](https://doc.rust-lang.org/std/mem/union.MaybeUninit.html).
