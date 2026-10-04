**Answer:** There are five functions named `begin_shutdown`, but a single shutdown runs only four of them. Which four depends on which pool implementation is active. All paths are under `tokio/src/runtime/blocking/`.

**Order of calls**
1. **`BlockingPool::shutdown`** (`pool.rs:314`) is the entry point, and it is not itself a `begin_shutdown`. It is called explicitly or from `Drop` (`pool.rs:343-345`). It calls `inner.inner_impl.begin_shutdown(&inner.metrics)` at `pool.rs:319`. If that returns `None`, shutdown has already begun and it returns early.
2. **`InnerImpl::begin_shutdown`** (`pool.rs:576-581`) is a pure dispatcher. It matches on the variant and calls `l.begin_shutdown()` for `Locked`, or `s.begin_shutdown(metrics)` for `Sharded`. It does nothing else.
3. **Implementation-specific `begin_shutdown`**: only one of these runs, depending on the variant.
   - **Locked** (`pool.rs:740-745`):
     - It takes `self.mutex.lock()`.
     - It calls `thread_mgmt_state.begin_shutdown()?`.
     - It then calls `self.condvar.notify_all()` to wake all waiting workers.
     - It returns the handles.
   - **Sharded** (`sharded.rs:336-352`):
     - It takes `self.coord.lock()`.
     - It calls `thread_mgmt_state.begin_shutdown()?`.
     - It stores `true` in `is_shutdown` with `Release` ordering.
     - It calls `condvar.notify_all()`.
     - It reads `metrics.num_threads() == 0` and drops the lock.
     - If there were no workers, it calls `self.drain_and_seal(metrics, 0)`. This seals the shards so a racing spawner's push isn't stranded.
     - It returns the handles.
4. **`ThreadManagementState::begin_shutdown`** (`pool.rs:156-166`) is the innermost one, called by both variants. It sets `self.shutdown = true`, sets `self.shutdown_tx = None`, and takes `last_exiting_thread` and `worker_threads` with `mem::take`. It returns them as `Some((last_exited_thread, workers))`, or `None` if `shutdown` was already set (`pool.rs:157-159`).

**After the calls return:** `BlockingPool::shutdown` waits on `shutdown_rx.wait(timeout)` (`pool.rs:324`). If the wait succeeds, it joins the last exited thread and then every worker (`pool.rs:325-338`). On loom builds it sorts the workers by id first.

**Uncertainty:** I read only the code at these line ranges. I did not read the omitted regions of `pool.rs`, such as the `Spawner`/`Inner` definitions, and I did not open `drain_and_seal`.