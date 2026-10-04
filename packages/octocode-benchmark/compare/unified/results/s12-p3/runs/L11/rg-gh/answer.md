**Short answer:** the output never goes through a channel. It is written into a `stage` field inside the task's heap allocation, and the `JoinHandle` later moves it out of that same field. Atomic state bits and a stored `Waker` coordinate the hand-off.

**1. The task writes the output.**
- `poll_future` (`tokio/src/runtime/task/harness.rs:523`) polls the future through `core.poll`.
- On `Ready(output)` it wraps the value as `Ok(output)`. A panic becomes `Err(JoinError)` instead.
- It then calls `core.store_output(output)` (`harness.rs`, end of `poll_future`).
- `store_output` (`core.rs:408-413`) calls `set_stage(Stage::Finished(output))`. `set_stage` (`core.rs:432`) overwrites the `stage` cell.
- Cancellation takes the same path. `cancel_task` stores `Err(JoinError::cancelled)` via `store_output`.

**2. The task marks completion and wakes the joiner.**
- `Harness::complete` (`harness.rs:331`) calls `state().transition_to_complete()` (`state.rs:184`). That is an atomic `fetch_xor` that flips the RUNNING and COMPLETE bits, with `AcqRel` ordering.
- If the snapshot says the `JoinHandle` is no longer interested, the task drops the output itself via `drop_future_or_output` (`harness.rs:331` onward).
- Otherwise, if the JOIN_WAKER bit is set, it calls `trailer().wake_join()` (`core.rs:581`). That calls `wake_by_ref` on the waker the `JoinHandle` stored in the `Trailer`.
- It then calls `unset_waker_after_complete`. If the handle was dropped in the meantime, it drops the waker itself.

**3. The `JoinHandle` reads the output.**
- `JoinHandle::poll` (`join.rs:327`) first checks the coop budget. It then puts a `Poll::Pending` on the stack and calls `self.raw.try_read_output(&mut ret, cx.waker())`.
- That call goes through the type-erased vtable (`raw.rs:290` and `raw.rs:360`) to `Harness::try_read_output` (`harness.rs:281`).
- `Harness::try_read_output` calls `can_read_output` (`harness.rs:422`).
  - If the COMPLETE bit is not set, it stores or swaps in the caller's waker. It does this with `set_join_waker` (`harness.rs:466`) and the state bit set by `state.rs:427`. It returns `false`, so the poll stays `Pending`.
  - If the task completed in the meantime, the state update fails and the function returns `true`.
- When `can_read_output` returns `true`, `*dst = Poll::Ready(self.core().take_output())` runs (`harness.rs:283`).
- `take_output` (`core.rs:420-430`) does `mem::replace(stage, Stage::Consumed)`. It returns the `Finished(output)` value, or panics with "JoinHandle polled after completion" if the stage is anything else.

**Why this is safe without a lock.** The state bits (RUNNING, COMPLETE, JOIN_INTEREST, JOIN_WAKER) decide who may touch the `stage` and waker cells. The comments cite "rules" in `task/mod.rs`. I did not open that file, so I haven't verified the rules themselves.

**Caveats.**
- I did not read the `Stage` enum definition directly. Its variants `Running`, `Finished` and `Consumed` appear in the code above.
- `Core::poll` (`core.rs:367`) calls `drop_future_or_output` when the future is ready. That sets the stage to `Consumed` before `store_output` overwrites it with `Finished`.
- I could not run `git` against the checkout, so I did not confirm that it is at the pinned commit `facc6fc`.