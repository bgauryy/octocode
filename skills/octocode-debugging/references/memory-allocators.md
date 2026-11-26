Load when native heap calls misbehave, RSS moves without live bytes changing, or a native allocation profiler or allocator debug check must be chosen per platform.

# Native allocators and allocation profilers

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
| Linux | `heaptrack ./prog` then `heaptrack_print`; Valgrind Massif ([Rust memory profiling](rust-memory.md#memory-allocations-are-not-rss)); jemalloc `MALLOC_CONF=prof:true` with a profiling build | ASan; glibc `MALLOC_CHECK_=3` with `LD_PRELOAD=libc_malloc_debug.so.0` (required since glibc 2.34; without it the variable has no effect), `MALLOC_PERTURB_` |
| macOS | `MallocStackLogging=1` then `leaks "$pid"`, `heap "$pid"`, `malloc_history "$pid" <address>`; Instruments Allocations | `MallocScribble=1`, `MallocGuardEdges=1`; ASan |
| Windows | WPR heap tracing, Visual Studio memory diagnostics | Page heap (`gflags`), ASan in MSVC |
| Rust | Heap profiling with `dhat` (in-process, opt-in) or the native tools above | Miri; ASan via nightly `-Zsanitizer=address` |
