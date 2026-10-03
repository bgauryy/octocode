The output is handed over through a slot in the task's heap cell, and a stored waker plus atomic state bits coordinate the two sides. All paths are in `tokio/src/runtime/task/` at facc6fc47e.

**1. The task stores its output in its own cell.**
- The task's `Core` holds `stage: CoreStage<T>`, which is an `UnsafeCell<Stage<T>>` (`core.rs:139`, `core.rs:164`).
- `Stage` is `Running(T)`, `Finished(Result<T::Output>)` or `Consumed` (`core.rs:225-229`).
- `poll_future` polls the future. On `Ready(output)` it calls `core.store_output(output)` (`harness.rs:537-551`).
- `store_output` calls `set_stage(Stage::Finished(output))` (`core.rs:408-413`).
- Panics and cancellation take the same route. `core.store_output(Err(panic_result_to_join_error(..)))` is at `harness.rs:508`.

**2. The task is marked complete and the joiner is woken.**
- `Harness::complete` (`harness.rs:331`) calls `state().transition_to_complete()`, which sets the COMPLETE bit.
- If `JOIN_INTEREST` is unset, the runtime drops the output itself (`drop_future_or_output`).
- Otherwise, if `JOIN_WAKER` is set, it calls `trailer().wake_join()` (`harness.rs:349`). That wakes the waker stored in `Trailer.waker: UnsafeCell<Option<Waker>>` (`core.rs:209`, `core.rs:581-586`).
- It then unsets the `JOIN_WAKER` bit, as the comment at `harness.rs:352` describes.

**3. The `JoinHandle` reads the output when polled.**
- `JoinHandle::poll` (`join.rs:327-346`) first checks the coop budget.
- It puts a `Poll::Pending` on the stack and calls `raw.try_read_output(&mut ret, cx.waker())`. This goes through the vtable (`raw.rs:35`, `raw.rs:290`, `raw.rs:360-368`), which erases the generic type, and the result is written through the `*mut ()` destination.
- `Harness::try_read_output` (`harness.rs:281-285`) calls `can_read_output(header, trailer, waker)`. If that returns true, it sets `*dst = Poll::Ready(core().take_output())`.
- `can_read_output` (`harness.rs:422`) loads a state snapshot:
  - If the task is not complete, it stores the caller's waker in `Trailer.waker` with `set_join_waker` (`harness.rs:466`). If a waker is already stored and `will_wake` says it matches, it returns without touching it (`harness.rs:433-441`). Otherwise it unsets `JOIN_WAKER`, swaps in the new waker and sets the bit again. It then returns false, so the poll stays `Pending`.
  - If the task is complete, it returns true (the lines after 460 were not read; this is inferred from the `Err(snapshot)` branch asserting `is_complete()` at `harness.rs:457-459`).
- `take_output` (`core.rs:420-430`) does `mem::replace(stage, Stage::Consumed)`. It returns the `Finished(output)` value, and panics with "JoinHandle polled after completion" for any other stage.

**Synchronization.** The bit layout is in `state.rs`: `JOIN_INTEREST` at `state.rs:31` and `JOIN_WAKER` at `state.rs:34`. The state bits decide who may touch the `stage` and `waker` cells, so no lock is needed. The rules are documented in `task/mod.rs`, which I cited from the comments but did not open.

**Dropping the handle.** `drop_join_handle_slow` (`harness.rs:~290-330`) unsets `JOIN_INTEREST`. If the task has already completed, the handle becomes responsible for dropping the output (`transition.drop_output`). This keeps a non-`Send` output from being dropped on an arbitrary thread.

**Uncertainty.** I did not read `task/mod.rs` or the tail of `can_read_output` (past `harness.rs:460`), so the ownership-rule details are taken from comments elsewhere.