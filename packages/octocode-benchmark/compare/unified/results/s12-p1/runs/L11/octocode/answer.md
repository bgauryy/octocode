The output is passed through the task's own heap cell. The task writes it into a `stage` field, and the `JoinHandle` reads it from there when polled. A stored waker and atomic state bits coordinate the two sides. All paths below are under `tokio/src/runtime/task/`.

**1. The task stores its output.**
- `poll_future` (`harness.rs:~525-559`) polls the future inside `catch_unwind`.
- A `Ready(output)` becomes `Ok(output)`. A panic becomes `Err(JoinError)` via `panic_to_error` (`harness.rs:543-547`).
- It then calls `core.store_output(output)` (`harness.rs:551`).
- `store_output` (`core.rs:408-413`) does `set_stage(Stage::Finished(output))`. So the output lives in the task's `Core` stage field, not in a channel.
- Cancellation takes the same route: `cancel_task` stores `Err(...)` (`harness.rs:508`).

**2. The task completes and wakes the joiner.**
- `poll_inner` returns `PollFuture::Complete` once the future is ready (`harness.rs:212-214`). `Harness::complete` (`harness.rs:331`) then runs.
- `complete` calls `state().transition_to_complete()` (`harness.rs:334`).
- If the `JoinHandle` is no longer interested, it drops the output (`harness.rs:339-344`).
- Otherwise, if the join waker is set (`is_join_waker_set()`), it calls `trailer().wake_join()` (`harness.rs:345-349`). That calls `waker.wake_by_ref()` on the stored waker (`core.rs:581-586`).
- It then calls `unset_waker_after_complete()`. If the handle has since been dropped, it drops the waker itself (`harness.rs:355-363`).

**3. The `JoinHandle` polls and takes the output.**
- `JoinHandle::poll` builds a `Poll::Pending` slot and calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:~346`). This goes through the vtable in `raw.rs:290` and `raw.rs:360` to `Harness::try_read_output`.
- `try_read_output` (`harness.rs:281-285`) does the following:
  - It calls `can_read_output(header, trailer, waker)`.
  - If that returns true, it writes `Poll::Ready(self.core().take_output())` into the slot.
  - `take_output` (`core.rs:420-425`) does `mem::replace(stage, Stage::Consumed)` and unwraps the `Finished` value.

**4. Waker registration (the race handling).** `can_read_output` (`harness.rs:422-464`) works as follows:
- If the task isn't complete, it stores the caller's waker in the trailer through `set_join_waker` (`harness.rs:466-492`). That sets the waker field, then calls `state.set_join_waker()` to set the `JOIN_WAKER` bit. It returns false, so the poll is `Pending`.
- If a waker is already stored and `will_wake` matches, it returns false without touching it (`harness.rs:439-441`). Otherwise it swaps the waker with `unset_waker()` followed by `set_join_waker` (`harness.rs:445-448`).
- If setting the waker fails because the task completed in the meantime, the snapshot shows `COMPLETE`. It then returns true and the output is read immediately (`harness.rs:456-463`).

The `JOIN_INTEREST`, `JOIN_WAKER` and `COMPLETE` bits in the task state decide who may touch the waker and stage fields. The rules are in the comments in `task/mod.rs`, which I did not read.

**Not verified.** I did not open `raw.rs`, `state.rs` or `mod.rs` in full. I also skipped the part of `join.rs` after the `ret.is_ready()` check, around line 349, which handles coop budget handling.