Load when GC time or pauses dominate, the managed heap must be read from GC logs or counters, or a heap snapshot must name what retains memory.

# Garbage collection and heap snapshots

## Garbage collection

| Runtime | GC log or events | Live counters | Knobs to check |
|---|---|---|---|
| Node / V8 | `--trace-gc`; `PerformanceObserver` with `entryTypes: ['gc']` | `process.memoryUsage()`, `v8.getHeapStatistics()`, `v8.getHeapSpaceStatistics()` | `--max-old-space-size`, `--max-semi-space-size` |
| JVM | `-Xlog:gc*:file=gc.log` | `jstat -gcutil "$pid" 1000`, `jcmd "$pid" GC.heap_info` | Collector (G1, ZGC, Parallel), `-Xmx`, `-XX:MaxRAMPercentage` |
| Go | `GODEBUG=gctrace=1` | `runtime.ReadMemStats`, `runtime/metrics` | `GOGC`, `GOMEMLIMIT` |
| .NET | `dotnet-trace` GC events | `dotnet-counters monitor -p "$pid" --counters System.Runtime` | Server vs workstation GC; heap hard limit (`System.GC.HeapHardLimit` in runtimeconfig.json, or `DOTNET_GCHeapHardLimit` with a hexadecimal value, .NET 6+) |
| Python | `gc.callbacks`, `gc.set_debug(gc.DEBUG_STATS)` | `gc.get_stats()`, `gc.get_count()` | `gc.set_threshold`; most objects are freed by reference counting, the cycle collector handles cycles |

| Pattern | Meaning | Next step |
|---|---|---|
| High GC share, heap after GC flat | Churn | Cut allocations at the top allocation sites: reuse buffers, pre-size collections, avoid intermediate copies and string building in loops |
| RSS climbs for many cycles, heap after a forced GC flat, old or large-object space used rising between collections | Churn on a host with spare RAM: the GC lets the heap grow before a full collection | Measure bytes allocated per operation, collected objects included ([Node](node-metrics.md#allocation-churn)), and cut the top sites; a lower heap limit only trades RSS for GC CPU |
| Heap after GC rises every cycle | Retention | Snapshot diff and retainer path |
| Heap near its limit, back-to-back full collections | GC thrash before an OOM | Find the retention; raising the limit only delays it if the growth is a leak |
| Long pauses at moderate heap | Large old generation, many pointers to scan, finalizers | Compare collectors and pause logs; pointer-free data is cheaper to scan |
| Runtime heap limit above the container limit | The kernel kills the process before the GC reacts | Set the heap limit (`GOMEMLIMIT`, `-XX:MaxRAMPercentage`, `--max-old-space-size`) below the cgroup limit with headroom for native memory |

Force a collection (`--expose-gc` + `gc()`, `System.gc()`, `runtime.GC()`, `gc.collect()`) only to measure, at the same point in each cycle. A rising heap between collections is normal: GC is lazy. Finalizers and cleanup callbacks run late or never; never rely on them to release scarce resources.

## Heap snapshots and retainers

- **Shallow size** is the object itself; **retained size** is what would be freed with it. Rank by retained size to find owners, by count to find what multiplies.
- The **dominator tree** shows which single object keeps a subgraph alive. A **retainer path** from a GC root names the anchor: a module-level map, a listener list, a timer, a cache, a closure.
- **Three-snapshot method**: warm up, snapshot A, run N cycles, snapshot B, run N more, snapshot C. Objects allocated between A and B that still exist in C are the leak candidates (in DevTools, open snapshot C and choose the filter "Objects allocated between Snapshot 1 and Snapshot 2").
- Snapshots pause the process and can need as much memory again as the heap. Take them on a target that can afford it.

| Runtime | Snapshot or live profile | Read with |
|---|---|---|
| Node | `v8.writeHeapSnapshot()`, `--heapsnapshot-signal=SIGUSR2`, `--heapsnapshot-near-heap-limit=1`; sampling: `--heap-prof` | Chrome DevTools Memory panel; [profile summary script](../scripts/profile-top.mjs) for `.heapprofile` ([Node profiling](node-profiling.md)) |
| JVM | `jcmd "$pid" GC.heap_dump /abs/path/heap.hprof`; histogram: `jcmd "$pid" GC.class_histogram` | Eclipse MAT or VisualVM; JFR allocation events for churn |
| Go | `/debug/pprof/heap` or `pprof.WriteHeapProfile` | `go tool pprof -inuse_space` (retention), `-alloc_space` (churn), `-base old.pprof new.pprof` for the diff ([Go](go.md)) |
| Python | `tracemalloc` snapshots and `compare_to`; `gc.get_referrers(obj)` for one object | [Allocation growth](python-async-memory.md#allocation-growth); Memray (if installed) also tracks native allocations |
| .NET | `dotnet-gcdump collect -p "$pid"` | Visual Studio or PerfView; diff two dumps |
| Native | [Allocation profilers](memory-allocators.md); no object graph | Allocation stacks still outstanding at the second checkpoint |
