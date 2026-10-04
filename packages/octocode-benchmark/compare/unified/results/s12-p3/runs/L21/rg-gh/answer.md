Three `begin_shutdown` functions run on every shutdown, always in the same order. A fourth never runs alongside the one before it, because it is the alternative to it. I couldn't confirm that the checkout is at facc6fc47e, because `git rev-parse` failed in this sandbox. All line numbers are from the files as checked out.

`BlockingPool::shutdown` (`tokio/src/runtime/blocking/pool.rs:319`) starts the chain. `BlockingPool::drop` also calls it, with `shutdown(None)`.

1. **`InnerImpl::begin_shutdown`** (`pool.rs:576-580`) is a dispatcher and does nothing else. It matches on the queue variant. `Locked` calls `LockedInner::begin_shutdown()`, and `Sharded` calls `Sharded::begin_shutdown(metrics)`. Only one of the two runs for a given pool.

2. **The variant-specific function** runs next. It takes the lock, calls step 3, and returns `None` early through `?` if shutdown had already begun.
   - **Locked** (`pool.rs:740-745`):
     - It takes `self.mutex`, calls `thread_mgmt_state.begin_shutdown()`, then does `condvar.notify_all()` to wake all waiting workers.
     - It returns the handles.
   - **Sharded** (`sharded.rs:336-351`):
     - It takes `self.coord`, calls `thread_mgmt_state.begin_shutdown()`, then does `is_shutdown.store(true, Release)` and `condvar.notify_all()`.
     - It reads `metrics.num_threads() == 0` and then drops the lock.
     - If there are no workers, it calls `drain_and_seal(metrics, 0)`. Live workers normally seal the shards as they exit, so this covers the case where none exist, and a racing spawner's push can't be stranded.
     - It returns the handles.

3. **`ThreadManagementState::begin_shutdown`** (`pool.rs:156-166`) is the shared core, and it touches no other state.
   - It returns `None` if `shutdown` is already true.
   - Otherwise it sets `shutdown = true` and sets `shutdown_tx = None`, which drops the shutdown sender.
   - It uses `mem::take` on `last_exiting_thread` and `worker_threads`.
   - It returns them as `ShutdownHandles`: an optional previously timed-out worker handle, plus the map of running workers' handles.

After the chain returns, `BlockingPool::shutdown` waits on `shutdown_rx.wait(timeout)` (`pool.rs:~324`). If the wait completes, it joins `last_exited_thread` and then every worker handle (`pool.rs:~325-340`).