**Where the event is:** `Waiter::assign_permits` in `tokio/src/sync/batch_semaphore.rs:559-566`. It is a `tracing::trace!` with target `runtime::resource::async_op::state_update`, run inside `self.ctx.async_op_span.in_scope(...)`. It is compiled only with `tokio_unstable` and the `tracing` feature. It fires right after the successful `state.compare_exchange` (line 556) and the `*n -= assign` update (line 558), and before `return next == 0` (line 567).

**Why a panic there leaves a completed waiter linked:**
- `add_permits_locked` (`batch_semaphore.rs:306`) handles the waiter at the back of the queue like this:
  - It calls `waiters.queue.last()`, which only peeks, so the waiter is still linked.
  - It then calls `waiter.assign_permits(&mut rem)` (about line 316).
  - Only if that returns `true` does it run `waiters.queue.pop_back()` (about line 326) and take the waker.
- So the waiter's state is decremented to 0 (all its permits assigned) while it is still in the intrusive linked list. The unlink comes later, and only if `assign_permits` returns normally.
- If the subscriber panics inside the trace event, the unwind skips `return next == 0` and the `pop_back`. The waiter's state is already 0, so it looks complete, but it is still in `waiters.queue`. The panic is caught outside, per the issue's reproduction.
- Nothing on that path repairs the queue. I did not read the `Acquire` drop code to confirm exactly how it behaves.
- The issue says the pending `Acquire` can then see its state as complete and finish, and its allocation is freed without being unlinked. A later release then walks the queue and dereferences the freed waiter. That is the use-after-free, and the permits that were assigned are also leaked.

**Uncertainty:** I read the issue body only up to the start of its PoC, and I did not read `Acquire::drop` or the poll-completion path. The claim that the completed waiter is freed without being unlinked therefore rests on the issue's description, not on code I checked. The same `Poll::Ready` path in `poll_acquire` (lines 481-487) also emits a trace event. It runs after `assign_permits` returns and before the waiter is queued, so it isn't the case the issue describes.