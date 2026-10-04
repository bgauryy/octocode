There are four `begin_shutdown` functions. Three run on any given shutdown, and which three depends on the queue variant. I read the code only and did not run it or check the checkout's HEAD. The `git` and `rg` calls failed in this sandbox.

**Call order.** All paths are in `tokio/src/runtime/blocking/`.

1. `BlockingPool::shutdown` (`pool.rs:~316`) calls `inner.inner_impl.begin_shutdown(&inner.metrics)` at `pool.rs:319`. `Drop` also calls it, with `shutdown(None)` at `pool.rs:~345`.
2. The `InnerImpl::begin_shutdown` enum dispatcher (`pool.rs:576-581`) then picks one of two variants:
   - **`InnerImpl::Locked`** goes to `LockedInner::begin_shutdown` (`pool.rs:740`), which calls `ThreadManagementState::begin_shutdown` (`pool.rs:156`).
   - **`InnerImpl::Sharded`** goes to `ShardedInner::begin_shutdown` (`sharded.rs:336`), which calls `ThreadManagementState::begin_shutdown` (`pool.rs:156`).
3. `ThreadManagementState::begin_shutdown` is the innermost function. The `?` in the caller returns early if it returns `None`.

**What each one does beyond the call it makes**

- **`InnerImpl::begin_shutdown`** (`pool.rs:576`) only dispatches on the variant. The sharded arm also passes `metrics`.
- **`ThreadManagementState::begin_shutdown`** (`pool.rs:156-166`) is the shared core, and it makes no further `begin_shutdown` call. It returns `None` if `shutdown` is already set, which makes repeated shutdown a no-op. Otherwise it:
  - sets `shutdown = true`
  - sets `shutdown_tx = None`, which drops the sender
  - takes `last_exiting_thread` and `worker_threads` out of the state
  - returns them as `ShutdownHandles`
- **`LockedInner::begin_shutdown`** (`pool.rs:740-745`) takes `self.mutex.lock()` and calls the core. If the core returns `Some`, it also calls `self.condvar.notify_all()` to wake all waiting workers, then returns the handles.
- **`ShardedInner::begin_shutdown`** (`sharded.rs:336-352`) takes `self.coord.lock()` and calls the core. If the core returns `Some`, it also:
  - stores `is_shutdown = true` with `Release` ordering
  - calls `condvar.notify_all()`
  - reads `metrics.num_threads() == 0`
  - drops the lock
  - if there were no workers, calls `self.drain_and_seal(metrics, 0)`, so a racing spawner's push isn't stranded
  - returns the handles

**After the chain.** Back in `BlockingPool::shutdown`, `shutdown_rx.wait(timeout)` runs (`pool.rs:321`). If it returns true, the pool joins `last_exited_thread` and then every worker handle (`pool.rs:321-337`). Under loom the workers are sorted by ID first.