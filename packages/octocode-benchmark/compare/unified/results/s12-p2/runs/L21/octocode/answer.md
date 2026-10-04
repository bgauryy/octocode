**Answer:** There are five `begin_shutdown` functions in `tokio/src/runtime/blocking/`. Three of them run on any given shutdown, and which three depends on which pool implementation is active. `BlockingPool::shutdown` (`pool.rs:314`) is the entry point. It isn't named `begin_shutdown`, but it starts the chain.

**Order of execution**
1. `InnerImpl::begin_shutdown(&self, metrics)` (`pool.rs:576-581`) runs first, called from `pool.rs:319`. It only dispatches, with no logic of its own. It calls `Locked(l).begin_shutdown()` or `Sharded(s).begin_shutdown(metrics)`.
2. The implementation-specific one runs next. Only one of these two runs, depending on the variant:
   - **Locked** (`pool.rs:740-745`):
     - It takes `self.mutex.lock()`.
     - It calls `thread_mgmt_state.begin_shutdown()?`, and returns `None` if that returns `None`.
     - It calls `self.condvar.notify_all()` to wake all waiting workers.
     - It returns the handles.
   - **Sharded** (`sharded.rs:336-352`):
     - It takes `self.coord.lock()`.
     - It calls `thread_mgmt_state.begin_shutdown()?`.
     - It stores `is_shutdown = true` with `Release` ordering.
     - It calls `condvar.notify_all()`.
     - It checks `metrics.num_threads() == 0` while still holding the lock, then drops the lock. If there were no workers, it calls `drain_and_seal(metrics, 0)`. This stops a racing spawner's push from being stranded, because no worker is left to seal the shards.
     - It returns the handles.
3. `ThreadManagementState::begin_shutdown` (`pool.rs:156-166`) runs last. Both variants call it while holding their lock.
   - If `self.shutdown` is already set, it returns `None`. The `?` in the caller then skips the notify and seal steps, and `shutdown` returns early at `pool.rs:321`.
   - Otherwise it sets `shutdown = true` and sets `shutdown_tx = None`. Dropping the sender is what lets `shutdown_rx.wait` complete.
   - It takes `last_exiting_thread` and `worker_threads` with `mem::take` and returns them as `Some((last_exited_thread, workers))`.

The calls nest, so the order of entry is 1, 2, 3. The state-level function (3) finishes first, then the impl-level one (2) does its notify and seal, and then the dispatcher (1) returns.

**After they return:** `shutdown` waits on `shutdown_rx.wait(timeout)` (`pool.rs:324`). If that succeeds, it joins `last_exited_thread` and then each worker handle (`pool.rs:325-338`). Under loom the workers are sorted by id first.

**Uncertainty:** I didn't read `drain_and_seal` itself, so I haven't checked exactly what it does. I also didn't check which variant is chosen at runtime.