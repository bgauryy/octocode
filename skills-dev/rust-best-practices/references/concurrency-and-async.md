# Concurrency, async and resource ownership

Load for threads, tasks, locks, channels, atomics, cancellation or shutdown. Tokio examples apply only when the project uses Tokio; inspect its locked version and enabled features.

## Choose the execution model

| Work | Starting point | Check |
|---|---|---|
| Simple synchronous operation | Keep it synchronous. | Async machinery has a caller or concurrency requirement. |
| Concurrent I/O | Existing async runtime and nonblocking APIs. | No blocking syscall or long CPU loop stalls a runtime worker. |
| Finite blocking operation inside async | Runtime's blocking bridge, such as `spawn_blocking`. | Admission limits outstanding work as well as running work; cancellation is cooperative. |
| Sustained CPU parallelism | Existing bounded CPU executor or scoped worker threads; Rayon is a candidate. | Workload justifies scheduling cost and avoids oversubscription. |
| Persistent blocking worker | Dedicated owned thread with stop signal and join. | Shutdown can stop the actual operation, not merely drop its handle. |

Calling an `async fn` creates a future; its body runs when polled. Await it, combine it with other futures or spawn it under an explicit owner. Concurrency does not itself imply parallel execution.

## Shared state

- Prefer workers owning their data and combining results when that fits. `Arc<T>` handles shared ownership, not synchronized mutation; `T` must satisfy the required `Send`/`Sync` bounds.
- Use a short lexical scope for a synchronous lock. Do not hold its guard across `.await`; it may make a future non-`Send` and contention can block the executor. An async mutex can span `.await`, but first consider moving I/O out of the critical section.
- Document lock order when taking multiple locks. Do not retain a concurrent-map guard while calling into code that may lock the same map or suspend.
- Treat mutex poisoning according to the protected invariant. Replacing the mutex implementation does not repair potentially inconsistent state.
- Use channels when ownership transfer fits the design. Derive capacity/admission from the resource owner; decide what happens when full, closed or cancelled. A bounded queue alone does not bound a task per waiting sender.
- Reach for atomics only with a stated synchronization invariant. Explain what is published and which operations synchronize. A different ordering cannot fix a logically incorrect algorithm; use targeted concurrency checking when warranted.

## Cancellation and completion are different

| Operation | Actual behavior | Owner action |
|---|---|---|
| Drop `JoinHandle` | Detaches its task; the result is lost. | Retain handles; observe task errors and join on shutdown. |
| `JoinHandle::abort()` | Requests cancellation of an async task; not proof it has finished. | Await completion when resource release matters. |
| Drop `JoinSet` | Aborts its tracked tasks; started blocking jobs are not abortable. | Use explicit cancel/drain/join; do not report cleanup from drop alone. |
| Abort a started `spawn_blocking` job | Does not stop it. | Pass a cooperative stop signal, make blocking operations interruptible where needed, and join. If hard termination is required, use an owned process boundary. |
| Timeout or drop a future | Stops polling that future; external effects may remain. | Define the operation's cancellation and retry semantics. |
| Drop a child-process handle | Can leave the process running. | Explicitly stop and reap; see [subprocesses](cli-tui-subprocess.md). |

- With `select!`, audit every losing branch. Restarting `read_exact` or `write_all` can lose partial progress; keep progress in owned state or use an API whose cancellation contract fits. Do not assume a timeout rolls back a write.
- Racing an owned `JoinHandle` can detach the losing task. Borrow a retained handle when appropriate, then explicitly cancel/join its owner.
- Shutdown must reject new work, signal existing work, drain or cancel according to policy, and wait for owned resources. If a policy deadline expires, report incomplete cleanup rather than claiming termination.
- `Drop` is synchronous and may not run on abort or process exit. Keep critical async shutdown explicit; avoid panicking during cleanup.
- A cancelled operation reports that it was cancelled wherever the cancel was first seen, between work items or inside one. Record the cancel at every check, not only in the outer loop: a result cut short without that reason reads as complete. A search that recorded it only between files returned partial results with no cancelled reason in 6–8 of 40 runs ([testing](testing-and-tooling.md#write-trustworthy-tests)).
- Take clocks and timers as values the owner passes in (a small trait or `Arc<dyn …>`; production passes the runtime's real time). Tests then advance a manual clock. A runtime's test-only paused time (tokio's `test-util`) either compiles into every dev binary or forks the dev and test graphs: 20 units here.

## Verify the lifetime

Cover success, error, cancellation before start and while running, queue saturation, receiver closure and shutdown with work outstanding. Use barriers, injected clocks and injected cancel points rather than sleeps or a runtime's test-only paused clock ([testing](testing-and-tooling.md#write-trustworthy-tests)). Check returned results and resource release; a timely response alone does not prove background work stopped.

Sources: [Tokio task cancellation](https://docs.rs/tokio/latest/tokio/task/#cancellation), [spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html), [select cancellation safety](https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety), [std atomics](https://doc.rust-lang.org/std/sync/atomic/).
