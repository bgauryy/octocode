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

## Leaks in long-lived managed runtimes

A garbage-collected process leaks references, not bytes. Ask first: **what repeats in one process** (a session, request, tool call, reconnect or test file), and what outlives it? One-time setup is not a leak. Something that grows with every repetition is.

| Step | How (Node shown; the same shape works for JVM, Go and Python) | Done when |
|---|---|---|
| 1. Repeat the unit | Drive N create, use and dispose cycles of the real unit in one process (not one long run) | N ≥ 10 cycles, with real work in each |
| 2. Count references | After each cycle: listeners per event on `process` and the stdio streams, `process.getActiveResourcesInfo()` by type, timers, child processes | Flat counts. Any per-cycle step is a leak with an exact owner. |
| 3. Measure the heap after GC | `--expose-gc`, `gc()` twice, then `heapUsed`, sampled at cycles 1, k and 2k | Flat, or a plateau. A cache warming up stops growing; a leak keeps its slope. |
| 4. Diff snapshots | `v8.writeHeapSnapshot()` at two cycles; aggregate self size and count by constructor or closure name; rank the delta | The growing names point at an owner (handler, registry or provider closures, not "strings") |
| 5. Walk retainers | BFS from the GC roots over strong edges (skip weak and shortcut edges), then follow parents back from samples of the grown objects | One chain names the anchor: a module-level collection, a timer, a global |
| 6. Fix at the anchor | Return and call the unsubscribe; delete on dispose; cap with LRU; make timers clearable | The probe from step 2 or 3 is flat, and a regression test fails without the fix |

Gotchas seen in practice:

- **Discarded unsubscribe.** `onSomething(cb)` on a module-level `Set` with its return value ignored keeps every disposed owner, and everything it references, alive. Search for subscription calls whose result is unused.
- **Timers capture context.** On Node 24+ each timer keeps the async-context frame it was created in (AsyncLocalStorage values). `AbortSignal.timeout(20_000)` on a 50 ms call pins that context, often a whole session, for 20 s, and it cannot be cleared. Use a clearable deadline (an `AbortController` plus `setTimeout`, cleared in `finally`).
- **Workers piped into stdio.** `new Worker()` without `stdout: true, stderr: true` pipes into the parent's streams and adds an `error` listener there per live worker. `MaxListenersExceededWarning` is the symptom; `--trace-warnings` plus a hook on `EventEmitter.prototype.on` that records stacks finds the source.
- **Registration and teardown asymmetry.** Cleanup that runs after other disposers is skipped when one of them throws. Release process-wide handlers (signals, `uncaughtException`, stream errors) in `finally`.
- **Per-key maps keyed by unique ids** (request ids, derived `<session>:<n>` ids, file paths, cwds) are never matched by a cleanup keyed on the parent id. Prefix-match the cleanup, or cap the map.
- **Keep what is cheap to keep.** Store a hash instead of the full text for dedupe. Leave a tiny monotonic counter that guards correctness (revision or generation numbers) alone: deleting it can reissue an old value.
- **Bounded but long is still a leak at churn.** A 30 s retention per call becomes the steady state when calls come every second.
- **Test runs leak too.** A runner killed before teardown leaves children behind: tmux started with `env -i`, grandchildren that ignore SIGTERM. Sweep orphans (re-parented to PID 1) whose command line names a dead run's temp folder at the next setup.
- **Tooling false alarms.** `expect(a).not.toBe(b)` in Vitest deep-compares for its hint and can trip throwing getters on closed handles. Compare identity directly.

Thinking checks: separate retention (the heap after GC grows) from churn (allocation rate) and from caches that plateau. Prove a leak with a slope over cycles, not with one large snapshot. A leak fixed in a test harness is not a product fix: say which one it was.
