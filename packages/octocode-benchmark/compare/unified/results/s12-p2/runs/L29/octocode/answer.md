**Short answer:** The event is the `tracing::trace!` in `Waiter::assign_permits`, at `tokio/src/sync/batch_semaphore.rs:561-565`. It uses target `runtime::resource::async_op::state_update` and records `permits_obtained = assign`. It sits inside `self.ctx.async_op_span.in_scope(...)` (line 560). It is compiled only under `cfg(all(tokio_unstable, feature = "tracing"))` (line 559).

**Why a panic there leaves a completed waiter linked:**

1. `add_permits_locked` peeks at the tail of the wait queue with `waiters.queue.last()` (line 314). That call only borrows the waiter and does not unlink it. It then calls `waiter.assign_permits(&mut rem)` (line 316).
2. In `assign_permits`, the `compare_exchange` at line 556 succeeds first. This sets the waiter's `state` to `next` and subtracts `assign` from `*n` (line 558). The permits are now fully assigned, and `next == 0` for a waiter that is complete.
3. The tracing event at lines 561-565 runs after that state change. It runs before `return next == 0` (line 567).
4. If the subscriber's `event()` panics, the unwind skips `return next == 0`. It also skips the unlinking code in the caller: `waiters.queue.pop_back()` (line 327) and the waker take (lines 328-332).
5. The result is that the waiter's `state` is already 0, so it counts as completed. It is still linked in `waiters.queue`. Its waker was never taken, so it was never woken. `rem` was already decremented, so those permits are lost.
6. The issue says the pending `Acquire` can later be polled and see `state == 0`, so it completes. It then drops and frees its allocation. This is safe only if the node is no longer in the list. Here it is still in the list, so the list holds a dangling pointer.
7. The next `add_permits` calls `queue.last()` at line 314 and dereferences the freed `Waiter`. Miri reports this as a use-after-free at `linked_list.rs:256`.

**Evidence and caveats:**
- The issue body is from issue #8541. It was closed by PR #8542, which I didn't read, so I haven't checked how it was fixed.
- I read the issue body only up to about 12,000 of 14,366 characters.
- I did not read the `Acquire` drop and poll code (lines 376-534 and from 575 on). The claim in point 6 about completion and freeing rests on the issue's description, not on my own reading of that code.
- The issue also describes a related permit leak on the uncontended fast path. That path is the events at lines 452-460 and 481. It is separate from the use-after-free in the question.