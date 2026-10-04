The output doesn't go through a channel. The task stores it in its own heap allocation, and the `JoinHandle` reads it from there. A join waker in the task's trailer, plus atomic state bits, coordinates the two sides. All paths below are under `tokio/src/runtime/task/`, and I read them in the local checkout. I did not run `git` to confirm it is at facc6fc47e, because `git` was blocked in this sandbox.

1. **The task stores its output in its `Core`.**
   - `poll_future` (`harness.rs:523`) polls the future inside `catch_unwind`.
   - A `Ready(output)` becomes `Ok(output)`. A panic becomes `Err(panic_to_error(...))`.
   - It then calls `core.store_output(output)` (`harness.rs:550-551`).
   - `store_output` (`core.rs:408-413`) writes `Stage::Finished(output)` into the task's `stage` cell via `set_stage`.
   - `Core::poll` (`core.rs:367-403`) first drops the finished future, setting `Stage::Consumed`. The `Finished` value is written afterward.

2. **The task is marked complete and the awaiter is woken.**
   - `Harness::complete` (`harness.rs:331`) calls `state().transition_to_complete()`, which sets the COMPLETE bit.
   - If the `JoinHandle` has lost interest (`!is_join_interested()`), the output is dropped instead (`harness.rs:339-343`).
   - Otherwise, if the join waker is set (`is_join_waker_set()`), it calls `trailer().wake_join()` (`harness.rs:349`).
   - `wake_join` (`core.rs:581-586`) calls `wake_by_ref` on the waker stored in `Trailer.waker`.
   - It then calls `unset_waker_after_complete()` to clear the JOIN_WAKER bit.

3. **The `JoinHandle` reads the output.**
   - `JoinHandle::poll` (`join.rs:327`) first checks the coop budget. It then calls `self.raw.try_read_output(&mut ret, cx.waker())` through the vtable. The result is written into a `Poll` slot on the stack, passed as an erased pointer (`join.rs:335-346`).
   - `Harness::try_read_output` (`harness.rs:281-285`) calls `can_read_output(...)`. If that returns true, it sets `*dst = Poll::Ready(self.core().take_output())`.
   - `take_output` (`core.rs:420-430`) swaps the stage with `Stage::Consumed` and returns the `Finished(output)` value. It panics with "JoinHandle polled after completion" if the stage was anything else.

4. **If the task isn't done yet, the waker is parked.**
   - `can_read_output` (`harness.rs:422-463`) loads the state.
   - If the task is not complete, it stores the caller's waker in the trailer through `set_join_waker` (`harness.rs:466`). If a waker is already stored, it skips the write when `will_wake` is true and otherwise swaps it.
   - If the task completes during that race, the state update fails with a snapshot showing `is_complete()`, and the function returns true so the output is read right away.

5. **Safety rests on the state bits.** The COMPLETE, JOIN_INTEREST and JOIN_WAKER bits in `state.rs` decide who may touch the `stage` and `waker` fields. The rules are documented in `task/mod.rs`, which I cited from the code comments and did not open.

I did not trace `drop_join_handle_slow` (`harness.rs:289`) past its first lines. It handles a `JoinHandle` dropped before completion, where the handle is responsible for dropping the output.