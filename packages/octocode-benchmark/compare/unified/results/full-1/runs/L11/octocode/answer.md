The output does not travel through a channel. It is written into the task's own heap allocation, and the `JoinHandle` reads it from there. A join waker plus atomic state bits handle the synchronization. All paths below are under `tokio/src/runtime/task/`.

**Write side (the task finishes)**
- `harness.rs:551` calls `core.store_output(output)` after the future returns. `harness.rs:508` does the same with `Err(panic_result_to_join_error(...))` when the future panicked.
- `core.rs:408-413` implements `store_output`. It calls `set_stage(Stage::Finished(output))`, which overwrites the task's `stage` cell (`core.rs:432`). That cell held the future and now holds the output.
- `Harness::complete` (`harness.rs:331`) then calls `state().transition_to_complete()`. It branches on the returned snapshot:
  - If `JOIN_INTEREST` is not set, the `JoinHandle` was dropped, so the runtime drops the output itself with `drop_future_or_output()`.
  - If `JOIN_WAKER` is set, it calls `trailer().wake_join()`. That is `core.rs:581`, which does `waker.wake_by_ref()` on the waker stored in the trailer. It then calls `unset_waker_after_complete()`, and drops the waker itself if the handle is gone.

**Read side (the awaiting side)**
- `JoinHandle::poll` in `join.rs` starts with `let mut ret = Poll::Pending`. It calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:346`). This goes through the task vtable (`raw.rs:290`, `raw.rs:360`) with the result slot passed as a `*mut` because the generic types are erased.
- `Harness::try_read_output` (`harness.rs:281-285`) calls `can_read_output(header, trailer, waker)` (`harness.rs:422`). If it returns true, the harness sets `*dst = Poll::Ready(self.core().take_output())`.
- `can_read_output` checks the state snapshot. If `is_complete()` is false (`harness.rs:428`), it stores the caller's waker in the trailer through `set_join_waker` (`harness.rs:466`) and returns false, so `poll` returns `Pending`.
  - If a waker is already stored, it is replaced only when it doesn't `will_wake` the new one.
  - The join waker is only stored while the task is incomplete. If the state update fails because the task completed in the meantime, the snapshot is asserted `is_complete()` (`harness.rs:459`) and the output is read.
- `take_output` (`core.rs:420-430`) does `mem::replace(stage, Stage::Consumed)`. It returns the `Finished(output)` value, or panics with "JoinHandle polled after completion" if the stage was anything else.

**Synchronization**
`state.rs` holds the atomic `JOIN_INTEREST`, `JOIN_WAKER` and `COMPLETE` bits. They decide who may touch the output cell and the waker slot. `set_join_waker` is at `state.rs:427`. The "rules" that these accesses follow are documented in `task/mod.rs`, which I did not open.

**Uncertainty**
I read the harness, core and join code paths above but not `state.rs` in detail. I did not open `raw.rs` beyond confirming the vtable entries.