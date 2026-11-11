# Memory and performance

Load when choosing an allocation or ownership strategy, reviewing memory safety, explaining a performance gap the complexity does not explain, or picking a profiler or sanitizer.

## Choose quickly

| Problem | Start with | Why |
|---|---|---|
| Small temporary objects share one lifetime | Arena or bump allocator | Cheap allocation, bulk release |
| Frequent allocation of fixed-size objects | Object pool or slab allocator | Less allocator overhead and fragmentation |
| Avoid copying large buffers | Borrowed views, spans, zero-copy | Saves copies; lifetime and mutability need care |
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
| Object pool | Reuse costly objects | Reset state fully before reuse |
| Garbage collection | Reclaim unreachable objects automatically | Pauses and memory headroom |
| Generational GC | Collect young objects often | Relies on most objects dying young |
| Reference counting | Reclaim when owning references reach zero | Cycles leak; atomic counts cost under contention |
| Copy-on-write | Defer copying until mutation | A hidden copy on first write |
| Memory mapping (`mmap`) | Map files or regions into the address space | Page faults on first touch; error handling via signals |
| Virtual memory | Isolate and translate process addresses | TLB misses |
| Demand paging | Load pages when touched | First-touch latency |
| Huge pages | Fewer translation misses on large mappings | Internal fragmentation, compaction stalls |
| NUMA-aware allocation | Place memory near the cores using it | Remote-node accesses cost more |
| Memory compaction | Reduce fragmentation | Moves objects; pauses |
| Escape analysis | Decide whether an object escapes its scope | Enables stack allocation or scalar replacement |

## References, pointers and ownership

| Concept | Meaning | Typical use |
|---|---|---|
| Pointer | Address of an object or region | Low-level memory access |
| Reference | Language-defined access to an object or value | APIs, object access |
| Raw pointer | Address without automatic lifetime management | Systems programming, FFI |
| Unique ownership | One owner manages the resource | Rust `Box`, C++ `unique_ptr` |
| Shared ownership | Several owners share lifetime responsibility | Rust `Rc`/`Arc`, C++ `shared_ptr` |
| Weak reference | Non-owning reference to managed data | Break ownership cycles |
| Borrowed reference | Temporary access without taking ownership | Rust borrowing |
| Immutable reference | Read-only access under language rules | Safe shared access |
| Mutable reference | Mutation through a reference | Controlled updates (Rust: exclusive) |
| Interior mutability | Mutation through an otherwise shared handle | Rust `Cell`, `RefCell`, `Mutex`, atomics |
| RAII | Release resources when the owning scope or object ends | C++ and Rust resource handling |
| Move semantics | Transfer ownership or resources | Avoid expensive copies |
| Value semantics | Independent values | Easier reasoning and isolation |
| Aliasing | Several references reach the same storage | Mutation and concurrency hazards |
| Lifetime analysis | Ensure references stay valid | Prevent dangling access |
| Pinning | Forbid moving an object | Self-referential structures, async runtimes |

### Memory safety problems

| Problem | Description | Caught by |
|---|---|---|
| Use-after-free | Access after the lifetime ends | ASan, ownership types |
| Dangling pointer | A pointer to invalid storage | Lifetime analysis, ASan |
| Double free | Releasing an allocation twice | ASan, unique ownership |
| Memory leak | Allocated memory never reclaimed | LeakSanitizer, heap profilers |
| Buffer overflow | Access beyond buffer bounds | ASan, bounds-checked access |
| Integer overflow | Arithmetic exceeds the type's range | UBSan (C signed overflow is undefined; Rust panics in debug builds and wraps in release by default) |
| Uninitialized read | Reading indeterminate data | MSan, Valgrind, definite-initialization rules |
| Reference cycle | Shared ownership keeps unused objects alive | Weak references, cycle collectors |
| Iterator invalidation | Mutation invalidates active iterators or references | Borrow checker, debug iterators |
| Data race | Unsynchronized concurrent access to shared mutable state | TSan, Go race detector, `Send`/`Sync` |

## Ownership and concurrency by language

| Language | Memory management | Concurrency mechanism |
|---|---|---|
| C | Manual `malloc`/`free` | pthreads, C11 atomics |
| C++ | RAII, smart pointers, manual control | `std::thread`, mutexes, atomics |
| Rust | Ownership, borrowing, lifetimes | `Send`, `Sync`, `Arc`, `Mutex`, channels |
| Java | Garbage collection | Threads, virtual threads, concurrent collections |
| C# | Garbage collection, `IDisposable` | Tasks, async/await, locks |
| Go | Garbage collection | Goroutines, channels, mutexes |
| Python | CPython: reference counting plus a cycle collector | Threads, processes, asyncio |
| JavaScript | Garbage collection | Event loop, workers, `SharedArrayBuffer` with `Atomics` |

Garbage collection manages reclamation; it does not prevent data races. Thread-safe reference counting (`Arc`, `shared_ptr`) makes the count safe, not the object it points to.

## Memory hierarchy and latency

Rough orders of magnitude for planning; measure the target hardware before relying on them.

| Level or operation | Typical latency | Note |
|---|---|---|
| Register | Under 1 ns | Smallest, fastest |
| L1 cache hit | About 1 ns | Per core |
| L2 cache hit | About 3–5 ns | Larger, per core or cluster |
| L3 cache hit | About 10–40 ns | Often shared across cores |
| DRAM access | About 50–100 ns | Much larger; NUMA remote access costs more |
| NVMe SSD random read | About 10–100 µs | Persistent |
| Round trip in one datacenter | About 0.1–0.5 ms | Network |
| HDD seek | About 2–10 ms | Sequential reads are far cheaper than random |
| Round trip across a continent or ocean | About 50–150 ms | Bound by the speed of light in fibre |

Cache lines are 64 bytes on most x86-64 processors; Apple silicon reports 128 (`sysctl hw.cachelinesize`). In C++ use `std::hardware_destructive_interference_size` for padding rather than a literal.

| Optimization | Why it matters |
|---|---|
| Cache locality | Fewer expensive memory accesses |
| Spatial locality | Access nearby addresses |
| Temporal locality | Reuse recently accessed data |
| Cache-line alignment | Avoid unnecessary cache-line sharing |
| Data-oriented design | Lay data out for the common access pattern |
| AoS vs SoA | Choose object-oriented or field-oriented layout |
| Prefetching | Load data before it is needed |
| SIMD / vectorization | Process several elements per instruction |
| Zero-copy | Avoid unnecessary data transfers |
| NUMA locality | Minimize remote-node memory access |
| TLB optimization | Fewer address-translation misses |
| Branch prediction | Predictable branches avoid pipeline flushes |
| Batching | Amortize dispatch, syscall and synchronization costs |
| Memory alignment | Respect hardware and ABI alignment |

## Debugging and profiling tools

| Tool | Primary purpose |
|---|---|
| AddressSanitizer (ASan) | Memory corruption, use-after-free, overflows |
| LeakSanitizer (LSan) | Leaks at exit (bundled with ASan on Linux) |
| UndefinedBehaviorSanitizer (UBSan) | Signed overflow, invalid shifts, misaligned access and other undefined behaviour |
| ThreadSanitizer (TSan) | Data races |
| MemorySanitizer (MSan) | Uninitialized reads (Clang, Linux) |
| Valgrind / Memcheck | Leaks and invalid accesses without recompiling (Linux) |
| perf | Linux CPU and hardware-counter profiling |
| heaptrack / Massif | Heap usage and allocation profiling |
| Flame graphs | Visualize CPU or allocation hotspots |
| Instruments, `sample` | macOS CPU, allocation and hang profiling |
| Java Flight Recorder | JVM profiling and diagnostics |
| async-profiler | JVM CPU, allocation and lock profiling |
| Go pprof and `-race` | CPU, heap, mutex and goroutine profiles; the race detector |
| py-spy | Sampling profiler for running Python processes |
| Node `--cpu-prof`, `--inspect` | V8 CPU profiles and the inspector |
| Rust Miri | Detect certain undefined behaviour in Rust, including unsafe code |
| C++ static analyzers | Lifetime, ownership and correctness issues (Clang Static Analyzer, clang-tidy) |

Sanitizers find only the bugs the run executes: pair them with tests and fuzzing that reach the risky paths.
