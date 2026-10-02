The output doesn't travel through a channel. It is written into a slot inside the task's own heap allocation. The `JoinHandle` then reads it from that slot, and the state bits and a stored waker coordinate the hand-off. All paths below are under `tokio/src/runtime/task/`.

**1. Storage slot.** The task's `Core` holds `stage: CoreStage<T>`, an `UnsafeCell<Stage<T>>` (`core.rs:138-139`, `core.rs:164`). `Stage` (`core.rs:225`) has `Running(future)`, `Finished(Result<T::Output, JoinError>)` and `Consumed` variants. This is the same allocation the `JoinHandle`'s raw pointer points at.

**2. Producer side (the runtime polling the task).**
- `Harness::poll` (`harness.rs:153`) calls `poll_future` (`harness.rs:523`).
- `poll_future` polls the future via `Core::poll` (`core.rs:367`). On `Ready`, `Core::poll` drops the future by setting the stage to `Consumed` (`core.rs:385-387`).
- `poll_future` wraps the result as `Ok(output)`, or as `Err(JoinError)` if the future panicked. It then calls `core.store_output(output)`, which does `set_stage(Stage::Finished(output))` (`core.rs:408-412`).
- `Harness::complete` (`harness.rs:331`) calls `state().transition_to_complete()`, which sets the COMPLETE bit.
  - If the `JoinHandle` is no longer interested (`!is_join_interested()`), it drops the output.
  - Otherwise, if `JOIN_WAKER` is set, it calls `trailer().wake_join()` to wake the awaiting task (`harness.rs:331-358`; `wake_join` is at `core.rs:581`). It then clears `JOIN_WAKER` with `unset_waker_after_complete()`.
- Cancellation and panics use the same path. `cancel_task` stores `Err(JoinError::cancelled)` or a panic error (`harness.rs:~500`).

**3. Consumer side (awaiting the `JoinHandle`).**
- `JoinHandle::poll` (`join.rs:327`) goes through the coop budget. It puts a `Poll::Pending` on the stack and calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:345-347`). This goes through the type-erased vtable (`raw.rs:360-367`) to `Harness::try_read_output` (`harness.rs:281`).
- `try_read_output` calls `can_read_output` (`harness.rs:422`). That function loads the state snapshot.
  - If the task is not complete, it stores the caller's waker in the trailer, which makes the producer's `wake_join` call reach the awaiting task. It does this through `set_join_waker` (`harness.rs:466`), after first swapping out any previously stored waker. If the task completed in the meantime, the state update fails and the function falls through to read.
  - If the task is complete, it returns `true`.
- When `can_read_output` returns `true`, `try_read_output` does `*dst = Poll::Ready(self.core().take_output())` (`harness.rs:281-285`). `take_output` (`core.rs:~423`) replaces the stage with `Consumed` and returns the `Finished(output)` value. It panics with "JoinHandle polled after completion" for any other stage.

**Synchronization.** The state bits `COMPLETE`, `JOIN_INTEREST` and `JOIN_WAKER` in `state.rs` decide who may touch the output and waker fields. The rules are documented in `task/mod.rs`, which `harness.rs` cites as "rule 2/3/4/5". I did not open `mod.rs` itself.

**Uncertainty.** I did not check the exact line numbers for the `Stage::Consumed` call inside `Core::poll`, for `cancel_task`, or for `take_output`, so those are approximate (`~`). The pinned commit SHA could not be confirmed with `git` because the sandbox blocked it, so I read the files from the provided checkout path.