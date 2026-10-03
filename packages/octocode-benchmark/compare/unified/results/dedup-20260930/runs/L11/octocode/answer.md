The output goes through the task's own heap allocation. The task stores it in a `stage` field, and the `JoinHandle` takes it out of that field after the task has signalled completion. There is no channel. The handoff is coordinated by atomic state bits and a stored join waker. I read the code below but did not trace every path, such as `can_read_output` (lines 422–466) and the `poll_future` body.

All paths are under `rust/tokio/src/runtime/task/`.

1. **Producer stores the output.**
   - `poll_future` (`harness.rs:523`) polls the future. When it finishes, it calls `core.store_output(output)` (`harness.rs:551`).
   - A panic is stored the same way as an `Err` join error (`harness.rs:508`).
   - `store_output` (`core.rs:408`) writes `Stage::Finished(output)` into the `Core`'s `stage` cell via `set_stage` (`core.rs:432`).

2. **Completion and wakeup.**
   - `Harness::complete` (`harness.rs:331`) first calls `state().transition_to_complete()`, which sets the COMPLETE bit.
   - If the `JoinHandle` has been dropped (`!is_join_interested()`), the task drops the output itself with `drop_future_or_output()`.
   - Otherwise, if a join waker is registered (`is_join_waker_set()`), it calls `trailer().wake_join()` (`harness.rs:~349`, defined at `core.rs:581`).
   - It then calls `unset_waker_after_complete()`. If the handle was dropped in the meantime, it clears the waker itself.

3. **Consumer polls.**
   - `JoinHandle::poll` (`join.rs`, around line 346) creates a `Poll::Pending` slot and calls `self.raw.try_read_output(&mut ret, cx.waker())`. It goes through the type-erased `RawTask` (`raw.rs:290`, vtable function at `raw.rs:360`).
   - That reaches `Harness::try_read_output` (`harness.rs:281`).
   - If `can_read_output(header, trailer, waker)` returns true, it sets `*dst = Poll::Ready(self.core().take_output())`.
   - `can_read_output` (`harness.rs:422`) checks the state bits. If the task isn't complete, it registers or updates the join waker (`set_join_waker`, `harness.rs:466`), so the task can wake the handle later.
   - `take_output` (`core.rs:420`) swaps the stage for `Stage::Consumed` and returns the value from `Stage::Finished(output)`. Any other stage panics with "JoinHandle polled after completion".

4. **Why it is safe.** Only one side touches `stage` at a time. The COMPLETE, JOIN_INTEREST and JOIN_WAKER bits in `state.rs` decide who has access. The output stays in the task until the handle reads it. `harness.rs:~290` explains that the handle or scheduler must drop the output, because it may not be `Send`.

**Uncertainty:** the `harness.rs:~349` line for the `wake_join` call is approximate, from the excerpt I read.