# Concurrency

Load when choosing a concurrency model or primitive, reviewing shared-state code, reasoning about atomics and memory ordering, or diagnosing a race, deadlock or stall.

## Choose quickly

| Problem | Start with | Why |
|---|---|---|
| Many tasks wait on APIs, sockets or databases | Async I/O, virtual threads or lightweight tasks | More concurrency without an OS thread per blocked request |
| CPU-heavy work in parallel | Bounded worker pool or work stealing | Uses the cores while preventing oversubscription |
| Several threads update shared state | Mutex, or atomics for simple independent state | A mutex protects multi-field invariants; atomics suit single-word transitions |
| Very frequent reads, rare writes | Immutable snapshots, RCU or a reader-writer lock | Less reader contention; old versions need managed lifetimes |
| High-throughput task communication | Bounded producer-consumer queue | Buffering with explicit backpressure |
| Lock-free structure needs safe deletion | Hazard pointers or epoch-based reclamation | Stops threads from touching reclaimed memory |
| Periodic unexplained latency spikes | Profile GC, page faults, locks and scheduling | Find the actual pause source before choosing a mitigation |

## Primitives and patterns

| Concept | Purpose | Important detail |
|---|---|---|
| Thread pool | Reuse worker threads | Avoids creation overhead; size it for CPU-bound or I/O-bound work separately |
| Work stealing | Dynamic load balancing | Idle workers steal queued tasks from busy ones |
| Fork-join | Parallel recursive computation | Split, compute, merge |
| Producer–consumer | Decouple processing stages | Bounded queues give backpressure |
| Reader–writer lock | Concurrent readers, exclusive writer | Writer or reader starvation depends on the fairness policy |
| Mutex | Exclusive access | Blocks competing threads |
| Spinlock | Busy-wait synchronization | Only for very short waits on dedicated cores |
| Semaphore | Limit concurrent access | Counting permits |
| Condition variable | Wait for a shared-state change | Wait in a loop that rechecks the predicate: wakeups can be spurious |
| Barrier | Synchronize execution phases | All participants reach the checkpoint; reusable per phase |
| Latch | One-shot synchronization | Wait for an event or a count of completions |
| Futures / promises | Represent pending results | Propagate results and failures |
| Actor model | Isolate mutable state | Communicate through messages; one mailbox per actor |
| CSP / channels | Coordinate concurrent tasks | Communicate instead of sharing memory |
| Structured concurrency | Manage task lifetimes | Child tasks finish, fail or cancel within their parent's scope |
| RCU (read-copy-update) | Read-heavy concurrent access | Readers see stable versions without locks; reclamation waits for a grace period |
| Lock striping | Reduce lock contention | Partition state across several locks |
| Lock-free algorithms | Non-blocking progress | Some thread always makes progress; built on atomic operations |
| Wait-free algorithms | Bounded completion steps | Every thread finishes in a bounded number of its own steps |

## Synchronization and atomic operations

| Technique | Application | Common issue |
|---|---|---|
| Atomic load / store | Shared scalar state | Wrong memory ordering |
| CAS (compare-and-swap) | Conditional state transition | ABA problem; spurious failure of weak CAS needs a loop |
| Fetch-and-add | Atomic counters | Contention on one cache line; shard hot counters |
| Memory fence | Enforce ordering constraints | Architecture-dependent semantics |
| Acquire / release | Publish and consume state | A missing happens-before edge |
| Sequential consistency | One global order of atomic operations | Can cost extra fences |
| Double-checked locking | Lazy initialization | Needs correct publication (Java `volatile`, C++ atomics); prefer a once primitive |
| Optimistic locking | Update only if the version is unchanged | Retry storms under contention |
| Pessimistic locking | Prevent competing updates | Blocking and deadlocks |
| Fine-grained locking | Smaller critical sections | Complexity and lock-ordering rules |
| Hazard pointers | Protect objects referenced by lock-free readers | Per-access bookkeeping |
| Epoch-based reclamation | Reclaim retired memory safely | One stalled thread delays all reclamation |

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

## Concurrency models

| Model | Best for | Limitation |
|---|---|---|
| OS threads | CPU parallelism, blocking tasks | Per-thread memory and scheduling overhead |
| Async / await | High-concurrency I/O | CPU-heavy work blocks the event loop unless offloaded |
| Event loop | Network and UI workloads | Handlers must not block |
| Actor model | Isolated stateful components | Message-passing overhead |
| Thread pool | Bounded worker execution | Pool sizing and queueing |
| Work-stealing pool | Irregular CPU-bound tasks | Scheduling and locality overhead |
| Virtual threads | Many blocking I/O tasks | No faster for CPU-bound work |
| Multiprocessing | Parallelism with memory isolation | IPC and serialization costs |
| GPU parallelism | Massively parallel data processing | Transfer costs and branch divergence |

Java virtual threads exist "to provide scale (higher throughput), not speed (lower latency)" ([JEP 444](https://openjdk.org/jeps/444)). CPython threads run Python bytecode one at a time under the GIL unless you use the optional free-threaded build, so CPU-bound Python work uses processes or native extensions.

## Memory ordering

| Ordering | Guarantee | Common use |
|---|---|---|
| `relaxed` | Atomicity only, no cross-thread ordering | Independent counters |
| `consume` | Dependency ordering | Deprecated in C++26; compilers treat it as `acquire` |
| `acquire` | Later reads and writes in this thread cannot move before the load | Consume published data |
| `release` | Earlier reads and writes in this thread cannot move after the store | Publish initialized data |
| `acq_rel` | Both, on a read-modify-write | Atomic read-modify-write |
| `seq_cst` | Plus one total order over all `seq_cst` operations | Simplest reasoning; the default |

Acquire and release synchronize only when the acquire load reads the value the release store wrote; choosing the flags does not by itself make arbitrary shared data safe ([cppreference: memory_order](https://en.cppreference.com/w/cpp/atomic/memory_order)).

### Example: publishing data in C++

```cpp
#include <atomic>
#include <thread>
#include <cassert>

int data = 0;
std::atomic<bool> ready{false};

void producer() {
    data = 42;
    ready.store(true, std::memory_order_release);
}

void consumer() {
    while (!ready.load(std::memory_order_acquire)) {
        std::this_thread::yield();
    }

    assert(data == 42);
}

int main() {
    std::thread a(producer);
    std::thread b(consumer);

    a.join();
    b.join();
}
```

The release store and the acquire load that reads `true` form a happens-before edge, so the consumer sees `data == 42`. The example assumes `data` is written only before publication. ThreadSanitizer (`clang++ -std=c++17 -fsanitize=thread`) reports no race for this program and reports a data race on `data` when both orderings are changed to `relaxed`.

## Rules

1. Prefer immutability when sharing data across threads.
2. Prefer clear ownership over widespread shared mutable state.
3. Use mutexes first; introduce lock-free algorithms when measurements justify the complexity.
4. Avoid allocating inside tight loops when reuse or batching fits.
5. Keep critical sections small without breaking invariants; never call unknown code while holding a lock.
6. Use bounded queues to control overload and memory growth.
7. Measure contention, allocation rates, cache misses and p99 latency.
8. Treat memory reclamation as part of the concurrent algorithm.
9. Document thread safety, object lifetimes and ownership contracts in APIs.
10. Validate concurrent code with stress tests, sanitizers and targeted fault injection.

## Learning order

1. Memory layout, stack and heap, pointers and references.
2. Ownership, lifetimes, RAII and garbage collection.
3. Threads, mutexes, semaphores and condition variables.
4. Atomics, memory ordering and happens-before.
5. Thread pools, async/await, actors and channels.
6. CPU cache, allocation, NUMA and performance.
7. Lock-free algorithms, RCU and memory reclamation.
