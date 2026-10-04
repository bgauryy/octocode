**Short answer:** the event is the `tracing::trace!` inside `Waiter::assign_permits`, at `tokio/src/sync/batch_semaphore.rs:555-566`. It fires after the waiter's permit count has already been set to 0, and before the waiter is popped from the queue. I read the code but did not reproduce the panic or read the issue text.

**Where the event is**
- The event is `tracing::trace!(target: "runtime::resource::async_op::state_update", permits_obtained = assign, permits.op = "add")`. It runs inside `self.ctx.async_op_span.in_scope(...)`.
- It is compiled only with `cfg(all(tokio_unstable, feature = "tracing"))`.
- It sits in the `Ok(_)` arm of the `state.compare_exchange`, right after `*n -= assign;`. The function returns `next == 0` immediately afterwards.

**Why a panic there leaves a completed waiter linked**
1. `add_permits_locked` (`batch_semaphore.rs:306-333`) peeks at the tail of the queue with `waiters.queue.last()`. It does not pop yet. It calls `waiter.assign_permits(&mut rem)`, and only if that returns `true` does it call `waiters.queue.pop_back()` and take the waker.
2. Inside `assign_permits`, the CAS has already stored `next` in `waiter.state`. If this is the final assignment, `state` is now 0, so the waiter looks complete. The `trace!` runs after that, while the node is still linked in `waiters.queue`.
3. If the subscriber panics in that `trace!`, `assign_permits` unwinds before it returns `true`. `pop_back` never runs, so the node stays in the queue, and `state == 0` and `*n` are already updated.
4. The `MutexGuard` is dropped during unwinding, so the queue lock is released with the completed node still in the list.
5. The `Acquire` future can then be dropped, or it can see `state == 0` and complete. `Drop for Acquire` (`batch_semaphore.rs:687-708`) only unlinks the node if `self.queued` is true. If the future treats itself as finished, or unlinks on a path that doesn't match the list state, the `Waiter` memory is freed or moved while the queue still holds a pointer to it. The next `queue.last()`, `pop_back` or `remove` then dereferences freed memory. That is the use-after-free.

**Uncertainty:** step 5 is my inference from the `queued` flag and the drop logic. I did not trace the exact drop and poll sequence the issue describes. The other `trace!` calls in this file (for example the one in `poll_acquire` near line 479) run under the lock but do not sit between "state modified" and "node popped", so I don't think they are the cause.