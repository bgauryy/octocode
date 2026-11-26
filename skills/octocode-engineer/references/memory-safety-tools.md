Load when reviewing a memory-safety problem (use-after-free, overflow, data race) or picking a sanitizer or profiler.

# Memory: safety problems and tools

## Memory safety problems

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

## Debugging and profiling tools

| Tool | Primary purpose |
|---|---|
| AddressSanitizer (ASan) | Memory corruption, use-after-free, overflows |
| LeakSanitizer (LSan) | Leaks at exit (integrated in ASan and on by default where supported: Linux, macOS, Android, Fuchsia, NetBSD) |
| UndefinedBehaviorSanitizer (UBSan) | Signed overflow, invalid shifts, misaligned access and other undefined behaviour |
| ThreadSanitizer (TSan) | Data races |
| MemorySanitizer (MSan) | Uninitialized reads (Clang; Linux, FreeBSD, NetBSD) |
| Valgrind / Memcheck | Leaks and invalid accesses without recompiling (Linux, FreeBSD, Solaris; macOS only on Intel, up to 13) |
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
