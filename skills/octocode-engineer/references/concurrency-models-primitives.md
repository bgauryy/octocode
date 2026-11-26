Load when picking an execution model (threads, event loop, actors, CSP) or a primitive or pattern (mutex, semaphore, channel, pool).

# Concurrency: models, primitives and patterns

## Concurrency models

| Model | Best for | Limitation |
|---|---|---|
| OS threads | CPU parallelism, blocking tasks | Per-thread memory and scheduling overhead |
| Async / await | High-concurrency I/O | CPU-heavy work blocks the event loop unless offloaded |
| Event loop | Network and UI workloads (Node.js, GUI frameworks) | Handlers must not block |
| Reactor / proactor | Event-driven I/O servers | Reactor dispatches on readiness, proactor on completion of an asynchronous operation |
| Actor model | Isolated stateful components that communicate through messages, one mailbox per actor | Message-passing overhead |
| Thread pool | Bounded worker execution; reuses threads for many short tasks, avoiding creation overhead | Size it separately for CPU-bound and I/O-bound work; bound the queue |
| Work-stealing pool | Irregular CPU-bound tasks; idle workers steal queued tasks from busy ones | Scheduling and locality overhead |
| Virtual threads | Many blocking I/O tasks | No faster for CPU-bound work |
| Multiprocessing, process pool | CPU-heavy parallelism with memory isolation | IPC and serialization costs |
| GPU parallelism | Massively parallel data processing | Transfer costs and branch divergence |

Java virtual threads exist "to provide scale (higher throughput), not speed (lower latency)" ([JEP 444](https://openjdk.org/jeps/444)). CPython threads run Python bytecode one at a time under the GIL unless you use the optional free-threaded build, so CPU-bound Python work uses processes or native extensions.

## Primitives and patterns

| Concept | Purpose | Important detail |
|---|---|---|
| Worker pool, work queue | Fixed workers drain a queue of buffered tasks | Bounded parallelism; bound the queue too |
| Fork-join | Parallel recursive computation | Split, compute, merge |
| MapReduce | Parallel transformation, then aggregation | Large-scale batch processing; the shuffle between phases dominates |
| Producer–consumer | Decouple processing stages | Bounded queues give backpressure |
| Reader–writer lock | Concurrent readers, exclusive writer | Writer or reader starvation depends on the fairness policy |
| Mutex | Exclusive access; the owner that locks releases | Blocks competing threads |
| Spinlock | Busy-wait synchronization | Only for very short waits on dedicated cores |
| Semaphore | Limit concurrent access | Counting permits |
| Condition variable | Wait for a shared-state change | Wait in a loop that rechecks the predicate: wakeups can be spurious |
| Barrier | Synchronize execution phases | All participants reach the checkpoint; reusable per phase |
| Latch | One-shot synchronization | Wait for an event or a count of completions |
| Futures / promises | Represent pending results | Propagate results and failures |
| Reactive streams | Asynchronous data streams | Backpressure is part of the protocol |
| Cancellation token | Cooperative cancellation of long-running work | Every blocking point must check it; release resources on cancel |
| Thread-local storage | Per-thread state, such as a request context | Leaks across tasks when pooled threads are reused; lost across async hops |
| CSP / channels | Coordinate concurrent tasks | Communicate instead of sharing memory |
| Structured concurrency | Manage task lifetimes | Child tasks finish, fail or cancel within their parent's scope |
| RCU (read-copy-update) | Read-heavy concurrent access | Readers see stable versions without locks; reclamation waits for a grace period |
| Lock striping | Reduce lock contention | Partition state across several locks |
| Lock-free algorithms | Non-blocking progress | Some thread always makes progress; built on atomic operations |
| Wait-free algorithms | Bounded completion steps | Every thread finishes in a bounded number of its own steps |
