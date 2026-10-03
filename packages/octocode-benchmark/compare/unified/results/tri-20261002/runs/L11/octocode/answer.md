The output doesn't travel through a channel. The task writes it into its own heap cell, and the `JoinHandle` reads it from there. A waker stored in the task and a set of atomic state bits coordinate the two sides. All paths below are under `tokio/src/runtime/task/`.

**1. The task stores the output in its own cell**
- `poll_inner` calls `poll_future(self.core(), cx)` (`harness.rs:210`).
- `poll_future` polls the future inside `catch_unwind` (`harness.rs:525-540`).
- A `Ready(output)` becomes `Ok(output)`. A panic becomes `Err(JoinError)` via `panic_to_error` (`harness.rs:543-547`).
- It then calls `core.store_output(output)` (`harness.rs:551`). That call runs `set_stage(Stage::Finished(output))`, which overwrites the `stage` field in `Core` (`core.rs:408-413`, `core.rs:432-435`).
- Cancellation takes a different path to the same place. `cancel_task` stores `Err(...)` the same way (`harness.rs:508`).

**2. The task completes and wakes the joiner**
- `poll_inner` returns `PollFuture::Complete` (`harness.rs:212-214`), and `poll` then calls `complete()` (`harness.rs:169-170`).
- `complete()` calls `state().transition_to_complete()`, which returns a snapshot (`harness.rs:334`).
- If the snapshot says the `JoinHandle` is no longer interested, the runtime drops the output itself (`harness.rs:339-344`).
- Otherwise, if the join-waker bit is set, it calls `trailer().wake_join()` (`harness.rs:345-349`). `wake_join` calls `wake_by_ref` on the waker stored in the `Trailer` (`core.rs:581-586`).
- It then clears the `JOIN_WAKER` bit with `unset_waker_after_complete`. If the handle was dropped in the meantime, it drops the waker too (`harness.rs:355-363`).

**3. The `JoinHandle` polls and reads the output**
- `JoinHandle::poll` first takes a coop budget slot, `coop::poll_proceed` (`join.rs:332`).
- It then calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:346`), which dispatches through the vtable. It calls `coop.made_progress()` only if the result is ready (`join.rs:349-350`).
- `Harness::try_read_output` calls `can_read_output(header, trailer, waker)` (`harness.rs:281-284`).
  - If the task isn't complete, `can_read_output` stores a clone of the caller's waker in the trailer and sets the `JOIN_WAKER` state bit. If a waker is already stored, it skips this when `will_wake` is true. Otherwise it unsets the old one and sets the new one (`harness.rs:428-455`, `set_join_waker` at `harness.rs:466-492`). The poll returns `Pending`.
  - If the state update fails because the task completed in the meantime, it falls through and reads (`harness.rs:458-463`).
  - If the task is complete, the harness does `*dst = Poll::Ready(self.core().take_output())` (`harness.rs:283`).
- `take_output` swaps the `stage` field with `Stage::Consumed`. If it held `Stage::Finished(output)`, it returns that. Any other stage panics with "JoinHandle polled after completion" (`core.rs:420-430`).

**Synchronization:** the `stage` and `waker` fields have no locks. The `COMPLETE`, `JOIN_INTEREST` and `JOIN_WAKER` bits in `State` (`state.rs`) decide who may touch which field. The "rules" are documented in `task/mod.rs`, which I referenced via code comments but did not read.

If the `JoinHandle` is dropped first, `drop_join_handle_slow` makes the handle responsible for dropping the output and the waker (`harness.rs:287-322`). The output may not be `Send`, so it must not be left to be dropped on an arbitrary thread.