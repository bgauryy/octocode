**Short answer:** the output never moves through a channel. The task stores it in its own heap cell, in `Core.stage`, as `Stage::Finished(output)`. The `JoinHandle` then reads it out of that cell on `poll`. Atomic state bits and a stored `Waker` coordinate the hand-off. All paths below are under `tokio/src/runtime/task/`.

**1. The future finishes and the output is stored in the task cell.**
- `Stage<T>` has three variants: `Running(T)`, `Finished(Result<T::Output>)` and `Consumed` (`core.rs:225-229`).
- The `poll_future` helper in `harness.rs` polls the future through `core.poll(cx)` (`harness.rs:538`). `Core::poll` is at `core.rs:367`.
- A `Ready` result, or a caught panic converted to a `JoinError`, becomes `output` (`harness.rs:543-547`).
- That `output` is then written with `core.store_output(output)` (`harness.rs:551`).
- `store_output` calls `set_stage(Stage::Finished(output))` (`core.rs:408-413`).
- Cancellation and shutdown take the same path: `harness.rs:508` stores `Err(...)`.

**2. The task is marked complete and the waiter is woken** (`Harness::complete`, `harness.rs:331`).
- `state().transition_to_complete()` flips the `RUNNING` and `COMPLETE` bits with one atomic `fetch_xor` (`state.rs:184-192`).
- If the `JoinHandle` is no longer interested, the task drops the output itself with `drop_future_or_output()` (`harness.rs:339-345`).
- Otherwise, if `JOIN_WAKER` is set, it calls `trailer().wake_join()` (`harness.rs:346-352`). That wakes the waker the `JoinHandle` left in the `Trailer`.
- It then clears `JOIN_WAKER` through `unset_waker_after_complete()` (`harness.rs:354-366`).

**3. The `JoinHandle` polls and reads the output.**
- `JoinHandle::poll` (`join.rs:327`) first checks the coop budget. It then calls `self.raw.try_read_output(&mut ret, cx.waker())` (`join.rs:345-347`).
- The call goes through the type-erased vtable. The result slot is passed as a `*mut ()` pointing at a `Poll<Result<T>>` on the caller's stack (`join.rs:335-344`, `raw.rs:360-369`).
- `Harness::try_read_output` (`harness.rs:281-285`) calls `can_read_output(header, trailer, waker)`. If that returns true, it sets `*dst = Poll::Ready(self.core().take_output())`.
- `take_output` swaps `Stage::Consumed` into the cell and returns the `Finished(output)` value. Polling again after completion panics with "JoinHandle polled after completion" (`core.rs:420-429`).

**4. If the task isn't finished yet, the waker is registered** (`can_read_output`, `harness.rs:422-463`).
- It loads the state and checks `is_complete()`.
- If the task isn't complete, it stores the caller's waker in the trailer through `set_join_waker` (`harness.rs:466-492`). It then sets the `JOIN_WAKER` bit with `state.set_join_waker()` (`state.rs:427-441`). That CAS fails if the task completed in the meantime.
- If a waker is already stored and `will_wake` matches, it returns early without touching the field (`harness.rs:436-441`).
- Otherwise it clears the bit with `unset_waker`, then stores the new waker (`harness.rs:445-448`).
- A failed CAS means the task completed concurrently. The code asserts `is_complete()` and falls through to return `true`, and the output is read immediately (`harness.rs:455-460`).

The `JOIN_WAKER` and `COMPLETE` bits decide who may touch the waker field. The ownership rules are documented in `task/mod.rs` and referenced from the comments in `harness.rs`. I didn't open `mod.rs` myself.

**Uncertainty:** I ran the commands against the local checkout and did not confirm that `HEAD` is `facc6fc47e`. The `git log` check failed because of a sandbox error. The `poll_future` line numbers (about 525-557) come from a partial view of the file. I did not see the function's signature line, so I'm inferring its name from context.