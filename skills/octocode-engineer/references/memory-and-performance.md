Load when choosing an allocation or ownership strategy, reviewing memory safety, explaining a performance gap the complexity does not explain, or picking a profiler or sanitizer.

Read next: [references and ownership](memory-ownership.md) for an ownership model or language comparison; [safety problems and tools](memory-safety-tools.md) for a memory-safety bug, sanitizer or profiler; [hierarchy and latency](memory-hierarchy.md) for cache and hardware costs; [leaks](memory-leaks.md) when a garbage-collected process grows.

# Memory and performance

## Choose quickly

| Problem | Start with | Why |
|---|---|---|
| Small temporary objects share one lifetime | Arena or bump allocator | Cheap allocation, bulk release |
| Frequent allocation of fixed-size objects | Object pool or slab allocator | Less allocator overhead and fragmentation |
| Avoid copying large buffers | Borrowed views, spans, zero-copy | Saves copies; the view must not outlive its buffer, and writes to the buffer show through |
| A resource has several owners | Reference counting / shared ownership | Lives until the last owning reference is released |
| Back-references create cycles | Weak references | Back-references do not extend lifetime |
| High memory use and poor cache behaviour | Contiguous data, SoA, compact representation | Better locality, less pointer chasing |
| Latency spikes | Profile GC, page faults, locks and scheduling first | The pause source decides the fix |

## Allocation and reclamation

| Technique | Purpose | Watch |
|---|---|---|
| Stack allocation | Automatic, scoped storage | Size limits; never return a reference to it |
| Heap allocation | Dynamic object lifetimes | Allocator cost and fragmentation |
| Arena allocator | Fast bulk allocation and release | Individual objects cannot be freed early |
| Memory pool | Reuse similarly sized allocations | Pool sizing |
| Slab allocator | Cache fixed-size object allocations | Per-size caches (kernel object caches) |
| Buddy allocator | Manage variable-sized blocks | Power-of-two rounding wastes memory |
| Free list | Track reusable blocks | Fragmentation, corruption on double free |
| Bump allocator | Very fast sequential allocation | Frees only all at once |
| Object pool | Reuse costly objects, such as connections | Reset state fully before reuse |
| Garbage collection | Reclaim unreachable objects automatically | Pauses and memory headroom |
| Generational GC | Collect young objects often | Relies on most objects dying young |
| Reference counting | Reclaim when owning references reach zero | Cycles leak; atomic counts cost under contention |
| Copy-on-write | Defer copying until mutation | A hidden copy on first write |
| Memory mapping (`mmap`) | Map files or regions into the address space | Page faults on first touch; I/O errors arrive as `SIGBUS` ([I/O models](io-models-async.md#io-models)) |
| Virtual memory | Isolate and translate process addresses | TLB misses |
| Demand paging | Load pages when touched | First-touch latency |
| Huge pages | Fewer translation misses on large mappings | Internal fragmentation, compaction stalls |
| NUMA-aware allocation | Place memory near the cores using it | Remote-node accesses cost more |
| Memory compaction | Reduce fragmentation | Moves objects; pauses |
| Escape analysis | Decide whether an object escapes its scope | Enables stack allocation or scalar replacement |
