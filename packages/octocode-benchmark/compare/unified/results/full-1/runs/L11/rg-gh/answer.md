The output is passed through the task's own heap cell. The task writes it into a slot in the cell, and the `JoinHandle` reads it out of that slot when it polls. The two sides coordinate through atomic state bits and a stored waker. There is no channel. All paths below are under `tokio/src/runtime/task/`.

**1. Producer side: the task finishes**
- `poll_future` polls the future through `core.poll(cx)`. On `Ready(output)` it wraps the value as `Ok(output)`. If the future panicked, it wraps a `JoinError` as `Err` instead (`harness.rs:523-551`).
- It then calls `core.store_output(output)` (`harness.rs:551`). That calls `set_stage(Stage::Finished(output))`, which overwrites the `stage` field of the task's `Core` (`core.rs:408-413`, `core.rs:432-435`).
- `Harness::complete` runs next. It calls `state().transition_to_complete()`, which sets the `COMPLETE` bit (`harness.rs:331-335`).
  - If `JOIN_INTEREST` is not set, nobody wants the output, so the task drops it with `drop_future_or_output()`.
  - If `JOIN_WAKER` is set, it calls `trailer().wake_join()`. That calls `wake_by_ref` on the waker the `JoinHandle` stored in the `Trailer` (`harness.rs:337-349`, `core.rs:581-586`).

**2. Consumer side: the `JoinHandle` is polled**
- `JoinHandle::poll` calls `self.raw.try_read_output(&mut ret, cx.waker())` through the type-erased vtable. The result slot `ret` starts as `Poll::Pending` and is passed in by pointer (`join.rs:327-346`). Polling is also subject to coop budgeting (`coop::poll_proceed`).
- `Harness::try_read_output` calls `can_read_output(header, trailer, waker)`. If that returns true, it sets `*dst = Poll::Ready(self.core().take_output())` (`harness.rs:281-285`).
- `take_output` swaps the stage with `Stage::Consumed` and returns the value from `Stage::Finished(output)`. Any other stage panics with "JoinHandle polled after completion" (`core.rs:420-430`).

**3. Synchronization between the two sides**
- `can_read_output` loads the state snapshot (`harness.rs:422-465`).
  - If `COMPLETE` is set, it returns true and the output is read.
  - Otherwise it stores the caller's waker in the trailer through `set_join_waker` (`harness.rs:466-490`).
  - If the stored waker already `will_wake` the new one, it skips the write. If a different waker is stored, it first unsets `JOIN_WAKER` and then swaps it.
  - It then sets the `JOIN_WAKER` bit in the state. If that update fails because the task completed in the meantime, it re-reads and returns the output.
- The state bits `COMPLETE`, `JOIN_INTEREST` and `JOIN_WAKER` are defined in `state.rs:22-40`.
  - `INITIAL_STATE` includes `JOIN_INTEREST` (`state.rs:61`).
  - The `Core::stage` slot and the `Trailer` waker are accessed without locks. Access is made safe by rules keyed on these bits, documented as "rules" in `task/mod.rs`. I only saw them referenced in comments and did not read the file itself.

**4. Dropping the `JoinHandle`**
- `drop_join_handle_slow` unsets `JOIN_INTEREST` (`harness.rs:284-`, `state.rs:381`).
  - If the task has already completed, the handle drops the stored output itself (`transition.drop_output`). This matters because the output may not be `Send` and must not be dropped on an arbitrary thread.
  - It also drops the stored join waker if it is responsible for it.

**Uncertainty:** I did not read `task/mod.rs`, so the safety-rule numbers come only from code comments. I did not trace the vtable wiring from `RawTask` to `Harness::try_read_output`. I only saw `self.raw.try_read_output` in `join.rs`.