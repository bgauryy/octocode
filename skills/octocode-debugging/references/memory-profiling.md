# Memory profiling: GC, heap, references, allocators and layout

Load when memory grows, GC time or pauses dominate, RSS disagrees with heap numbers, a native program leaks or corrupts memory, or data structures cost more memory than expected. Run it inside the [profiling loop](profiling.md#the-profiling-loop): coarse counters first, then the one capture that separates the rivals.

## Name the memory first

| Number | Counts | Read it with |
|---|---|---|
| RSS (resident set) | Pages in RAM: heap, stacks, code, mapped files, shared libraries | Linux `/proc/$pid/smaps_rollup`, `pmap -x "$pid"`; macOS `vmmap --summary "$pid"`, `footprint "$pid"` |
| Virtual size | Reserved address space | Rarely the problem: reservations cost no RAM until touched |
| Managed heap used / committed | Live objects plus garbage not yet collected / what the runtime reserved | The runtime's GC log or counters (below) |
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

## Garbage collection

| Runtime | GC log or events | Live counters | Knobs to check |
|---|---|---|---|
| Node / V8 | `--trace-gc`; `PerformanceObserver` with `entryTypes: ['gc']` | `process.memoryUsage()`, `v8.getHeapStatistics()`, `v8.getHeapSpaceStatistics()` | `--max-old-space-size`, `--max-semi-space-size` |
| JVM | `-Xlog:gc*:file=gc.log` | `jstat -gcutil "$pid" 1000`, `jcmd "$pid" GC.heap_info` | Collector (G1, ZGC, Parallel), `-Xmx`, `-XX:MaxRAMPercentage` |
| Go | `GODEBUG=gctrace=1` | `runtime.ReadMemStats`, `runtime/metrics` | `GOGC`, `GOMEMLIMIT` |
| .NET | `dotnet-trace` GC events | `dotnet-counters monitor -p "$pid" --counters System.Runtime` | Server vs workstation GC, `GCHeapHardLimit` |
| Python | `gc.callbacks`, `gc.set_debug(gc.DEBUG_STATS)` | `gc.get_stats()`, `gc.get_count()` | `gc.set_threshold`; most objects are freed by reference counting, the cycle collector handles cycles |

| Pattern | Meaning | Next step |
|---|---|---|
| High GC share, heap after GC flat | Churn | Cut allocations at the top allocation sites: reuse buffers, pre-size collections, avoid intermediate copies and string building in loops |
| RSS climbs for many cycles, heap after a forced GC flat, old or large-object space used rising between collections | Churn on a host with spare RAM: the GC lets the heap grow before a full collection | Measure bytes allocated per operation, collected objects included ([Node](node-profiling.md#allocation-churn)), and cut the top sites; a lower heap limit only trades RSS for GC CPU |
| Heap after GC rises every cycle | Retention | Snapshot diff and retainer path |
| Heap near its limit, back-to-back full collections | GC thrash before an OOM | Find the retention; raising the limit only delays it if the growth is a leak |
| Long pauses at moderate heap | Large old generation, many pointers to scan, finalizers | Compare collectors and pause logs; pointer-free data is cheaper to scan |
| Runtime heap limit above the container limit | The kernel kills the process before the GC reacts | Set the heap limit (`GOMEMLIMIT`, `-XX:MaxRAMPercentage`, `--max-old-space-size`) below the cgroup limit with headroom for native memory |

Force a collection (`--expose-gc` + `gc()`, `System.gc()`, `runtime.GC()`, `gc.collect()`) only to measure, at the same point in each cycle. A rising heap between collections is normal: GC is lazy. Finalizers and cleanup callbacks run late or never; never rely on them to release scarce resources.

## Heap snapshots and retainers

- **Shallow size** is the object itself; **retained size** is what would be freed with it. Rank by retained size to find owners, by count to find what multiplies.
- The **dominator tree** shows which single object keeps a subgraph alive. A **retainer path** from a GC root names the anchor: a module-level map, a listener list, a timer, a cache, a closure.
- **Three-snapshot method**: warm up, snapshot A, run N cycles, snapshot B, run N more, snapshot C. Objects allocated between A and B that still exist in C are the leak candidates (DevTools "Objects allocated between snapshots").
- Snapshots pause the process and can need as much memory again as the heap. Take them on a target that can afford it.

| Runtime | Snapshot or live profile | Read with |
|---|---|---|
| Node | `v8.writeHeapSnapshot()`, `--heapsnapshot-signal=SIGUSR2`, `--heapsnapshot-near-heap-limit=1`; sampling: `--heap-prof` | Chrome DevTools Memory panel; [profile summary script](../scripts/profile-top.mjs) for `.heapprofile` ([Node profiling](node-profiling.md)) |
| JVM | `jcmd "$pid" GC.heap_dump /abs/path/heap.hprof`; histogram: `jcmd "$pid" GC.class_histogram` | Eclipse MAT or VisualVM; JFR allocation events for churn |
| Go | `/debug/pprof/heap` or `pprof.WriteHeapProfile` | `go tool pprof -inuse_space` (retention), `-alloc_space` (churn), `-base old.pprof new.pprof` for the diff ([Go](go.md)) |
| Python | `tracemalloc` snapshots and `compare_to`; `gc.get_referrers(obj)` for one object | [Python profiling](python-profiling.md); Memray (if installed) also tracks native allocations |
| .NET | `dotnet-gcdump collect -p "$pid"` | Visual Studio or PerfView; diff two dumps |
| Native | Allocation profilers (below); no object graph | Allocation stacks still outstanding at the second checkpoint |

## References and pointers

| Kind | Keeps the target alive | Typical failure | How it shows |
|---|---|---|---|
| Strong reference | Yes | Forgotten owner: listener, cache, global map, closure capture, pooled thread-local | Retainer path ends at that owner |
| Weak reference (`WeakRef`, `WeakMap`, Java `WeakReference`, Python `weakref`, Rust `Weak`) | No | Used as a cache: entries vanish at the next GC | Low hit rate that varies with GC timing |
| Soft reference (JVM) | Until memory pressure | Heap stays full, then empties in a burst | Long GC cycles before the clear |
| Phantom / `FinalizationRegistry` / `Cleaner` | No; notifies after collection | Cleanup that runs late or never | Native resources outlive their wrappers |
| Reference counting (`Rc`, `Arc`, `shared_ptr`, Swift ARC, CPython) | While the count is above zero | Cycles never reach zero (CPython's cycle collector reclaims them; `Rc`, `shared_ptr` and ARC do not) | Count stays above zero after the last user is gone; fix with a weak back-reference |
| Raw pointer or borrowed reference | No | Dangling: the target was freed or moved | Use-after-free, garbage values, crashes far from the bug |

Native pointer bugs and the tool that catches each:

| Bug | Tool |
|---|---|
| Use-after-free, double free, heap or stack overflow | AddressSanitizer (`-fsanitize=address -g`), about 2× slower |
| Leak at exit | LeakSanitizer (with ASan on Linux), macOS `leaks --atExit -- ./prog` |
| Uninitialized read | MemorySanitizer (Clang, Linux) or Valgrind Memcheck |
| Undefined behaviour in arithmetic, alignment, casts | UBSan (`-fsanitize=undefined`) |
| Iterator or reference invalidated by container growth | Debug containers (`_GLIBCXX_DEBUG`, libc++ hardening), ASan |
| Unsafe Rust aliasing and provenance | Miri (`cargo +nightly miri test`) |

Sanitizers catch only the paths the run executes; drive them with the failing input and tests or fuzzing that reach the risky code.

## Lists: linked nodes vs contiguous storage

| Structure | Memory cost | Profile signature | Watch |
|---|---|---|---|
| Linked list, tree or graph of separately allocated nodes | Allocator header plus pointers per node | Cache misses and pointer-chasing loads in CPU samples; GC scan time | Prefer arrays, or nodes in an arena addressed by index |
| Array, vector, slice | Dense; spare capacity up to the growth factor | Large `memcpy`/realloc frames during growth | Growth moves elements: pointers, iterators and references into it become invalid |
| Go slice | Header shares a backing array | Small slices retaining a huge array | `append` to a subslice can overwrite the parent's elements; copy out (`slices.Clone`) to release the large array |
| Boxed element lists (`ArrayList<Integer>`, Python `list` of ints) | One object per element plus a pointer | Object count far above the element count | Primitive arrays, `array`, NumPy, typed arrays |
| JS arrays | Packed elements while types stay uniform | Sudden slowdown after holes or mixed types | `TypedArray` for numbers; avoid `delete arr[i]` |
| Substrings and slices of large strings or buffers | Can retain the parent (V8 sliced strings, Go substrings, `Buffer.slice`) | Retainer path from a small string to a large one | Copy the slice when you keep it longer than the parent |

## malloc, calloc, realloc, free

| Call | Behaviour | Common bug |
|---|---|---|
| `malloc(n)` | Uninitialized memory, or `NULL` on failure | Reading before writing; unchecked `NULL`; `malloc(count * size)` overflowing to a small buffer |
| `calloc(count, size)` | Zeroed memory; common libcs check `count × size` for overflow | Assuming it costs a `memset`: large blocks from fresh pages are already zero, so the cost appears on first touch |
| `realloc(p, n)` | May move the block | Pointers into the old block dangle; `p = realloc(p, n)` leaks `p` on failure (assign to a temporary first); size `0` is undefined in C23 |
| `free(p)` | Returns the block to the allocator, not necessarily to the OS | Double free; freeing a pointer the allocator did not return; use after free |
| `aligned_alloc`, `posix_memalign` | Aligned blocks | Size rules differ; release with `free` on POSIX. MSVC has `_aligned_malloc`, released with `_aligned_free` |
| Pairing | `malloc`↔`free`, `new`↔`delete`, `new[]`↔`delete[]`, a library's own release function for its objects | Mixing allocators or C runtimes across library boundaries corrupts the heap |

Allocator behaviour that moves RSS without a leak:

- glibc keeps per-thread arenas: RSS can grow with thread count (`MALLOC_ARENA_MAX` limits arenas; `malloc_trim(0)` releases free whole pages from all arenas). Fragmentation keeps pages resident after frees.
- Linux overcommit lets `malloc` succeed and the OOM killer act later, on first touch.
- jemalloc, mimalloc and tcmalloc change both RSS behaviour and the profiling hooks; profile with the production allocator.

| Platform | Allocation profiling | Debug checks |
|---|---|---|
| Linux | `heaptrack ./prog` then `heaptrack_print`; Valgrind Massif ([Rust profiling](rust-profiling.md#memory-allocations-are-not-rss)); jemalloc `MALLOC_CONF=prof:true` with a profiling build | ASan; glibc `MALLOC_CHECK_=3` with `LD_PRELOAD=libc_malloc_debug.so` (glibc 2.34+), `MALLOC_PERTURB_` |
| macOS | `MallocStackLogging=1` then `leaks "$pid"`, `heap "$pid"`, `malloc_history "$pid" <address>`; Instruments Allocations | `MallocScribble=1`, `MallocGuardEdges=1`; ASan |
| Windows | WPR heap tracing, Visual Studio memory diagnostics | Page heap (`gflags`), ASan in MSVC |
| Rust | Heap profiling with `dhat` (in-process, opt-in) or the native tools above | Miri; ASan via nightly `-Zsanitizer=address` |

## Structs and object layout

Fields are aligned to their natural alignment and the struct size is rounded up to its largest alignment, so field order changes size:

```c
struct Bad  { char a; double b; char c; };  /* 24 bytes on x86-64: 7 + 7 bytes padding */
struct Good { double b; char a; char c; };  /* 16 bytes */
```

Check with `sizeof`/`offsetof` or `pahole -C Bad ./binary` (needs debug info) before reordering.

| Language | Layout rule | Inspect | Memory lever |
|---|---|---|---|
| C, C++ | Declaration order with padding | `sizeof`, `offsetof`, `pahole` | Order fields large to small; bitfields for flags; packing (`__attribute__((packed))`, `#pragma pack`) makes misaligned access slow or faulting on some CPUs |
| Rust | Default `repr` may reorder fields; `#[repr(C)]` keeps declaration order for FFI | `std::mem::size_of::<T>()`, `-Zprint-type-sizes` (nightly) | `Box` large enum variants; niches keep `Option<Box<T>>` pointer-sized; references to packed fields are rejected |
| Go | Declaration order with padding | `unsafe.Sizeof`, the `fieldalignment` analyzer | Order fields; pointer-free types are not scanned by the GC; `go build -gcflags=-m` shows what escapes to the heap |
| Java | Object header (about 12–16 bytes) plus aligned fields; compact headers on recent JDKs shrink it | JOL (Java Object Layout) | Primitives over boxes; fewer small objects |
| C# | `struct` is a value, `class` a heap object; boxing allocates | `Unsafe.SizeOf<T>()`, `StructLayout` | Value types in hot arrays; avoid boxing in generic paths |
| Python | Each instance carries a `__dict__` unless `__slots__` is declared | `sys.getsizeof` (shallow only), `tracemalloc` | `__slots__` for many small objects; arrays for numbers |
| JavaScript | Objects share hidden classes (shapes) when built the same way | DevTools heap snapshot shallow sizes | Same constructor and property order; avoid `delete` on hot objects |

Large structs passed or returned by value show up as `memcpy` in CPU samples; pass a reference or pointer. Split hot fields from cold ones and use struct-of-arrays when a loop reads one field across many items. Pad per-thread counters to the cache line to avoid false sharing.

## Practices

1. Profile with production's allocator, build flags, heap limit and container limit; a debug build or a different allocator changes the answer.
2. Measure per operation: bytes allocated per request, retained bytes per cycle. Totals without a denominator do not compare.
3. Fix at the owner: release the reference, unsubscribe, bound the cache, close the handle. Forced GC, a larger heap or manual frees without an established owner hide the cause.
4. In hot paths, pre-size and reuse buffers, stream instead of loading whole inputs, and avoid per-item allocation.
5. Bound every cache, queue and map keyed by request-scoped ids; use weak keys for metadata attached to objects you do not own.
6. Run sanitizers on native code in CI and on the failing input; Miri for unsafe Rust.
