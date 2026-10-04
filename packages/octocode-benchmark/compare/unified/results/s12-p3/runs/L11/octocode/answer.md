The output goes through the task's own heap cell. The task writes it into a `stage` field, and the `JoinHandle` reads it from there after the task wakes it. No channel is involved. An atomic state word and a stored join waker coordinate the two sides.

**1. Storing the output (task side)**
- The future's output lives in the task's `Core` as `Stage<T>`, which has `Running(future)`, `Finished(output)` and `Consumed` variants. The variants are visible in `core.rs:371-374`, `core.rs:411` and `core.rs:425-427`.
- The poll helper in `harness.rs` polls the future. It maps `Ready(output)` to `Ok(output)` and a panic to `Err(JoinError::panic)` (`harness.rs:543-547`). It then calls `core.store_output(output)` (`harness.rs:551`).
- `store_output` calls `set_stage(Stage::Finished(output))` (`core.rs:408-412`), which overwrites the `Running` future with the output (`core.rs:432-435`).
- Cancellation takes the same route. `cancel_task` stores `Err(JoinError::cancelled)` or a panic error (`harness.rs:502-508`).

**2. Waking the waiter (task side)**
- `Harness::complete()` first calls `state().transition_to_complete()` (`harness.rs:334`).
- If the `JoinHandle` is no longer interested, the task drops the output itself (`harness.rs:339-344`).
- Otherwise, if the `JOIN_WAKER` bit is set, it calls `trailer().wake_join()` (`harness.rs:345-349`). That calls `waker.wake_by_ref()` on the waker stored in the `Trailer` (`core.rs:581-586`).
- It then clears the `JOIN_WAKER` bit with `unset_waker_after_complete()`. If the handle has been dropped by then, it drops the stored waker (`harness.rs:355-363`).

**3. Reading the output (`JoinHandle` side)**
- `JoinHandle::poll` first checks the coop budget. It puts `ret = Poll::Pending` on the stack and calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:327-347`). That call goes through the task vtable, so the generic types are erased and `ret` is passed as `*mut ()` (`join.rs:337-340`; the `raw.rs:290/360` wrappers are the vtable entry points, which I didn't read).
- `Harness::try_read_output` calls `can_read_output(header, trailer, waker)`. If that returns true, it sets `*dst = Poll::Ready(self.core().take_output())` (`harness.rs:281-284`).
- `take_output` does `mem::replace(stage, Consumed)` and returns the `Finished(output)` value. It panics with "JoinHandle polled after completion" if the stage isn't `Finished` (`core.rs:420-429`).
- `can_read_output` is where the handoff is decided (`harness.rs:425-463`):
  - If the task isn't complete yet, it stores a clone of the caller's waker in the `Trailer`. It skips the write if `will_wake` says the stored waker already matches (`harness.rs:439`). Otherwise it swaps the waker with `unset_waker` followed by `set_join_waker`. `set_join_waker` writes the waker and then sets the `JOIN_WAKER` bit via `header.state.set_join_waker()` (`harness.rs:466-492`).
  - If that state update fails because the task completed in the meantime, it falls through and returns true, so the output is read straight away.
  - If the task is already complete, it returns true.

**Uncertainty:** I read the code around the harness's `poll_inner` only partially (`harness.rs:201-224` and `275-539` were not shown in full). I did not read the `state.rs` bit transitions or the `raw.rs` vtable wrappers in detail. The ordering guarantees between `COMPLETE`, `JOIN_WAKER` and `JOIN_INTEREST` are described in comments in `task/mod.rs`, which I didn't open.