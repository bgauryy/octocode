Four functions are named `begin_shutdown`, but only three run in any one shutdown. `BlockingPool::shutdown` (`pool.rs:~315`, also called from `Drop`) calls the `InnerImpl` dispatcher, and that calls either the Locked or the Sharded variant. Whichever variant runs then calls `ThreadManagementState::begin_shutdown`. I couldn't run `git` in this sandbox, so I didn't confirm the checkout is at `facc6fc47e`.

**Order of calls**
1. `InnerImpl::begin_shutdown(&self, metrics)` at `pool.rs:576`. `shutdown` calls it at `pool.rs:319`. If it returns `None`, `shutdown` returns early, because shutdown has already begun.
2. One variant runs, depending on which pool implementation is in use:
   - Locked: `begin_shutdown` at `pool.rs:740`.
   - Sharded: `begin_shutdown` at `sharded.rs:336`.
3. `ThreadManagementState::begin_shutdown(&mut self)` at `pool.rs:156`. Both variants call it while holding their lock, and it runs first inside them.

**What each does**
- **`InnerImpl::begin_shutdown`** (`pool.rs:576-581`) only dispatches. It sends `Locked(l)` to `l.begin_shutdown()` and `Sharded(s)` to `s.begin_shutdown(metrics)`. It adds nothing else.
- **`ThreadManagementState::begin_shutdown`** (`pool.rs:156-166`) is the shared core.
  - It returns `None` if `shutdown` is already true.
  - Otherwise it sets `shutdown = true` and sets `shutdown_tx = None`.
  - It takes `last_exiting_thread` and `worker_threads` out of the state and returns them as `ShutdownHandles`.
  - It doesn't wake any workers. Its doc comment says the caller must do that.
- **Locked variant** (`pool.rs:740-745`):
  - It locks `self.mutex` and calls the core function, returning early on `None`.
  - It then calls `self.condvar.notify_all()` to wake all waiting workers, and returns the handles.
- **Sharded variant** (`sharded.rs:336-355`):
  - It locks `self.coord` and calls the core function, returning early on `None`.
  - It then stores `true` into the `is_shutdown` atomic with `Release` ordering and calls `condvar.notify_all()`.
  - It checks whether `metrics.num_threads() == 0` and drops the lock. If there are no workers, it calls `self.drain_and_seal(metrics, 0)`. Normally each exiting worker seals the shards. With no workers left, nobody would, so a racing spawner's push could be stranded.

**After the call chain**, `shutdown` waits on `shutdown_rx` with the timeout. If the wait succeeds, it joins `last_exited_thread` and then each worker handle (`pool.rs:~323-341`). Under loom the workers are sorted by ID first, for deterministic ordering.

**Uncertainty:** the line numbers for `shutdown` are approximate, since I read them from a range printout. I didn't check how the pool picks Locked or Sharded.