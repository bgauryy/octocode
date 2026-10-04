There are four `begin_shutdown` functions in the blocking pool. On shutdown, three run in a chain, and which one starts the chain depends on the pool's `InnerImpl` variant (`Locked` or `Sharded`). The first is `BlockingPool::shutdown`, which is not named `begin_shutdown` but is what starts the chain.

**Order of calls**
1. `BlockingPool::shutdown(timeout)` (`pool.rs:314`) calls `inner.inner_impl.begin_shutdown(&inner.metrics)` at `pool.rs:319`. `Drop` also calls it, at `pool.rs:345`. If the result is `None`, it returns, so a second call does nothing. Otherwise it waits on `shutdown_rx` and joins the threads (`pool.rs:324-338`).
2. `InnerImpl::begin_shutdown` (`pool.rs:576`) is a dispatcher. It matches on the variant and calls one of two functions:
   - `Locked(l)` calls `l.begin_shutdown()` (`pool.rs:578`).
   - `Sharded(s)` calls `s.begin_shutdown(metrics)` (`pool.rs:579`).
3. The `Locked` variant's `begin_shutdown` is at `pool.rs:740`. It locks the mutex and calls `thread_mgmt_state.begin_shutdown()?` (`pool.rs:742`).
4. The `Sharded` variant's `begin_shutdown` is at `sharded.rs:336`. It locks `coord` and calls `thread_mgmt_state.begin_shutdown()?` (`sharded.rs:338`).
5. `ThreadManagementState::begin_shutdown` (`pool.rs:156`) is the innermost function and does the real state change.

**What each one does beyond the function it calls**
- **`InnerImpl::begin_shutdown`** (`pool.rs:576-581`) only dispatches by variant. It adds nothing else.
- **`ThreadManagementState::begin_shutdown`** (`pool.rs:156-166`) calls no other `begin_shutdown`, so everything it does is its own work:
  - It returns `None` if `shutdown` is already set.
  - It sets `shutdown = true` and drops the shutdown sender with `shutdown_tx = None`.
  - It takes `last_exiting_thread` and `worker_threads` and returns them as the handles to join.
- **Locked variant** (`pool.rs:740-745`): after the call, it runs `self.condvar.notify_all()` to wake waiting workers, and returns the handles.
- **Sharded variant** (`sharded.rs:336-352`) does more than the Locked one:
  - After the call, it sets `is_shutdown` to `true` with `Ordering::Release`.
  - It runs `condvar.notify_all()`.
  - It checks whether `metrics.num_threads() == 0`, drops the lock, and if there are no workers calls `drain_and_seal(metrics, 0)`. This seals the shards so a spawner racing with shutdown can't leave a task stranded (`sharded.rs:342-349`).
  - It returns the handles.

The `?` after the innermost call means that on a repeat shutdown, none of the extra steps in the Locked and Sharded variants run.

I read all of this in the source at the pinned commit and found no uncertainty.