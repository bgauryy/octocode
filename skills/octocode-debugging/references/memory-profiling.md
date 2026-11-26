Load when memory grows, GC time or pauses dominate, RSS disagrees with heap numbers, a native program leaks or corrupts memory, or data structures cost more memory than expected.

# Memory profiling: GC, heap, references, allocators and layout

Run it inside the [profiling loop](profiling.md#the-profiling-loop): coarse counters first, then the one capture that separates the rivals.

| Then, when the question is | Load |
|---|---|
| GC share, pauses, heap limits, heap snapshots or retainer paths | [Garbage collection and heap snapshots](memory-gc.md) |
| Which kind of reference keeps an object alive; use-after-free, leaks at exit, sanitizers | [References, pointers and sanitizers](memory-references.md) |
| `malloc`-family bugs, allocator arenas and fragmentation, native allocation profilers per platform | [Native allocators and allocation profilers](memory-allocators.md) |
| Linked versus contiguous collections, boxed elements, retained slices, struct padding | [Collection and object layout](memory-layout.md) |

## Name the memory first

| Number | Counts | Read it with |
|---|---|---|
| RSS (resident set) | Pages in RAM: heap, stacks, code, mapped files, shared libraries | Linux `/proc/$pid/smaps_rollup`, `pmap -x "$pid"`; macOS `vmmap --summary "$pid"`, `footprint "$pid"` |
| Virtual size | Reserved address space | Rarely the problem: reservations cost no RAM until touched |
| Managed heap used / committed | Live objects plus garbage not yet collected / what the runtime reserved | The runtime's [GC log or counters](memory-gc.md#garbage-collection) |
| Native heap | `malloc`, `new`, allocator arenas, buffers owned by native libraries | Allocation profiler or allocator statistics |
| Mapped and shared | File mappings, shared memory, shared library pages | `smaps` per mapping; summing processes double-counts shared pages |
| Allocator reserve and fragmentation | Freed memory the allocator keeps or cannot return | RSS minus live allocated bytes |

When RSS grows and the managed heap after GC is flat, the growth is garbage the runtime has not collected yet, native memory, buffers, allocator retention, JIT code or mappings, not live managed objects. Compare RSS with heap used (garbage included) at the same moment: if they rise together and a forced collection brings RSS back, it is uncollected garbage. On macOS, `vmmap --summary` lists V8's heap pages as `Memory Tag 255`.

## Churn, retention or peak

| Question | Evidence | Capture |
|---|---|---|
| Churn: allocating too much per operation | Allocation rate, GC frequency, GC share of CPU, allocator frames in CPU samples | Allocation sampling over a fixed number of operations, read per operation (for Node, [profile summary](../scripts/profile-top.mjs) `--ops`) |
| Retention: memory not released | Heap after GC, sampled at the same lifecycle point, rising across equivalent cycles | Heap snapshots or live-allocation profiles at cycles 1, k and 2k of at least ten equivalent cycles; diff them |
| Peak: memory too high at one moment | High-water mark, OOM kills, container limit hits | A time series across the spike plus the allocation tree at the peak (Massif, heaptrack) |

Cumulative allocation (`alloc_space`, an allocation timeline) answers churn; live or in-use views answer retention; one large snapshot proves neither. Prove a leak with a slope over equivalent cycles after cleanup, and a cache by its plateau: record the heap after GC at the same lifecycle point of each cycle, one row per cycle with `cycle` and the value, and run:

```bash
node "$skill/scripts/compare-runs.mjs" --file "$out/cycles.csv" --growth cycle
```

`growing` means the later half still rises by more than its noise (the leak signature), `plateau` that the series rose and then stopped (a cache filling), `flat` or `falling` neither; `perCycle` and `latePerCycle` are the fitted slopes in the value's unit per cycle. Fewer than 6 cycles give `insufficient-cycles`; prefer ten or more.

## Practices

1. Profile with production's allocator, build flags, heap limit and container limit; a debug build or a different allocator changes the answer.
2. Measure per operation: bytes allocated per request, retained bytes per cycle. Totals without a denominator do not compare.
3. Fix at the owner: release the reference, unsubscribe, bound the cache, close the handle. Forced GC, a larger heap or manual frees without an established owner hide the cause.
4. In hot paths, pre-size and reuse buffers, stream instead of loading whole inputs, and avoid per-item allocation.
5. Bound every cache, queue and map keyed by request-scoped ids; use weak keys for metadata attached to objects you do not own.
6. Run sanitizers on native code in CI and on the failing input; Miri for unsafe Rust.
