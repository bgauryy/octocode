Load when choosing a concurrency model or primitive, reviewing shared-state code, reasoning about atomics and memory ordering, or diagnosing a race, deadlock or stall.

Read next: [models, primitives and patterns](concurrency-models-primitives.md) to pick an execution model or primitive; [atomics and memory ordering](concurrency-atomics.md) for lock-free code; [publishing data in C++](concurrency-publication-example.md) for a worked ordering example.

# Concurrency

## Choose quickly

| Problem | Start with | Why |
|---|---|---|
| Many tasks wait on APIs, sockets or databases | Async I/O, virtual threads or lightweight tasks | More concurrency without an OS thread per blocked request |
| CPU-heavy work in parallel | Bounded worker pool or work stealing | Uses the cores while preventing oversubscription |
| Several threads update shared state | Mutex, or atomics for simple independent state | A mutex protects multi-field invariants; atomics suit single-word transitions |
| Very frequent reads, rare writes | Immutable snapshots, RCU or a reader-writer lock | Less reader contention; old versions need managed lifetimes |
| High-throughput task communication | Bounded producer-consumer queue | Buffering with explicit backpressure |
| Lock-free structure needs safe deletion | Hazard pointers or epoch-based reclamation | Stops threads from touching reclaimed memory |
| Periodic unexplained latency spikes | [Profile first](memory-and-performance.md#choose-quickly): GC, page faults, locks and scheduling | Find the actual pause source before choosing a mitigation |

Blocking calls inside async code and unbounded fan-out are in [async vs sync hazards](io-models-async.md#async-vs-sync-hazards); rate and concurrency limits in [throttling](io-throttling.md#throttling).

## Concurrency failures

| Problem | Meaning | Common prevention |
|---|---|---|
| Data race | Conflicting accesses (at least one a write) without ordering synchronization | Locks, atomics, ownership |
| Race condition | Correctness depends on timing | Atomic state transitions |
| Deadlock | Threads wait on each other in a cycle | One global lock order; timeouts on lock acquisition |
| Livelock | Threads run without progress | Randomized backoff, coordination |
| Starvation | A task never receives the resource | Fair scheduling |
| Priority inversion | High-priority work waits on lower-priority work | Priority inheritance or priority ceiling |
| False sharing | Unrelated variables share a cache line | Padding and alignment to the line size |
| ABA | A value goes A → B → A and CAS misses the change | Version tags, safe reclamation |
| Lost wakeup | A notification arrives before the wait | Predicate-based condition waits |
| TOCTOU | State changes between check and use | Atomic operations, locking, open-then-check on file handles |

A deadlock needs all four Coffman conditions: mutual exclusion, hold and wait, no preemption and circular wait. Breaking any one, usually circular wait through lock ordering, prevents it.

## Rules

1. Share immutable data across threads; give mutable state one owner.
2. Use mutexes first; introduce lock-free code only when measured contention, cache misses or p99 latency justify the complexity.
3. Keep critical sections small without breaking invariants; never call unknown code while holding a lock.
4. Use bounded queues to control overload and memory growth.
5. For a lock-free structure, choose the reclamation scheme with the algorithm ([atomics](concurrency-atomics.md#synchronization-and-atomic-operations)).
6. Document thread safety, object lifetimes and ownership contracts in APIs.
7. Back a concurrency claim with an execution check: stress test, sanitizer or targeted fault injection ([claim limits](analysis-proof.md#claim-limits)).
