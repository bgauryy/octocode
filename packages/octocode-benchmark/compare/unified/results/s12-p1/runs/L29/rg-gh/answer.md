**Short answer:** The event is the `tracing::trace!` inside `Waiter::assign_permits` at `tokio/src/sync/batch_semaphore.rs:559-565`. It runs after the waiter's `state` has already been set to its final value, but before the waiter is unlinked from the queue. I read the code but not issue #8541, and I didn't read the part of `Acquire::poll` that clears `queued`. The last step below is therefore inferred.

**Where the event is**
- `assign_permits` (line 551) does a `compare_exchange` on `self.state`, reducing the waiter's outstanding need to `next` (lines 554-557).
- Right after that succeeds, and only with `tokio_unstable` and the `tracing` feature, it calls `self.ctx.async_op_span.in_scope(|| tracing::trace!(target: "runtime::resource::async_op::state_update", permits_obtained = assign, permits.op = "add"))` (lines 559-566).
- Only after that does it `return next == 0` (line 567).
- Any subscriber code that runs inside that `trace!` can panic.

**Why a completed waiter stays linked**
1. `add_permits_locked` (line 306) peeks at the tail with `waiters.queue.last()` and calls `waiter.assign_permits(&mut rem)` (lines 313-316). The waiter is still linked at this point.
2. The unlink happens later, at `waiters.queue.pop_back().unwrap()` (line 327). Its waker is taken just after that (lines 328-331).
3. If `assign_permits` panics inside the trace event, it unwinds before the `return next == 0` and before the `pop_back`. By then the CAS has already stored `state == 0`, so the waiter looks fully satisfied. The node is still in `waiters.queue`, and the `MutexGuard` is dropped during unwinding.
4. `poll_acquire` has the same pattern. It calls `node.assign_permits(&mut acquired)` (line 489) and, if that returns true, calls `add_permits_locked` and returns `Ready` (lines 489-491). A panic in that trace event leaves the same state.
5. `Acquire::drop` (line 687) only takes the lock and calls `queue.remove(node)` if `self.queued` is true (lines 689-702). If `queued` is false, it returns early on the assumption that "there is no node in the wait list".
6. **Inferred, not verified:** the use-after-free needs `queued` to be false, or the future to have been completed, while the node is still linked. Then the `Acquire` is dropped or freed with its node still in the list, and a later queue operation touches freed memory. I did not read where `Acquire::poll` sets `*queued` after `poll_acquire` returns (around line 600), so I haven't confirmed which path produces that state.