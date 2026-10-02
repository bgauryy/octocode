**Short answer:** the output never moves through a channel. The task's heap cell holds a `stage` slot. The runtime writes the result into that slot when the future finishes. `JoinHandle::poll` reads it back out through the task vtable. An atomic state word and a stored join waker coordinate the two sides. All paths below are under `tokio/src/runtime/task/`.

**1. Writing the output (task side)**
- `Harness::poll_inner` calls `poll_future(self.core(), cx)` at `harness.rs:210`.
- `poll_future` (`harness.rs:523-559`) polls the future inside `catch_unwind`.
  - A ready value becomes `Ok(output)` (`harness.rs:545`).
  - A panic becomes `Err(JoinError::panic)` via `panic_to_error` (`harness.rs:546`, `562-568`).
  - It then calls `core.store_output(output)` (`harness.rs:551`) and returns `Poll::Ready(())`.
- `Core::store_output` (`core.rs:408-413`) calls `set_stage(Stage::Finished(output))`. That writes into `self.stage.stage` (`core.rs:432-435`).
- Cancellation uses the same slot. `cancel_task` stores `Err(JoinError::cancelled)` (`harness.rs:502-509`).

**2. Signalling completion (task side)**
- When `poll_inner` returns `PollFuture::Complete`, `poll` calls `Harness::complete` (`harness.rs:169-171`, `331-390`).
- `complete` first calls `state().transition_to_complete()` (`harness.rs:334`).
- If the `JoinHandle` is not interested, the runtime drops the output with `drop_future_or_output` (`harness.rs:339-344`).
- If a join waker is set, the runtime calls `trailer().wake_join()` (`harness.rs:345-349`). `wake_join` does `waker.wake_by_ref()` on the waker stored in the `Trailer` (`core.rs:~590`). The runtime then clears `JOIN_WAKER` with `unset_waker_after_complete` (`harness.rs:355-363`).

**3. Reading the output (JoinHandle side)**
- `impl Future for JoinHandle<T>::poll` (`join.rs:~324-352`) starts with `ret = Poll::Pending` on the stack. It then calls `self.raw.try_read_output(&mut ret, cx.waker())`.
- The call goes through the vtable (`raw.rs:290-293`, `try_read_output` at `raw.rs:~360`). The vtable erases the generic types, so `ret` is passed as a `*mut ()`.
- `Harness::try_read_output` (`harness.rs:281-285`) calls `can_read_output(header, trailer, waker)`. If that returns true, it sets `*dst = Poll::Ready(self.core().take_output())`.
- `take_output` (`core.rs:420-430`) does `mem::replace(stage, Stage::Consumed)`. It returns the `Finished(output)` value, which is a `Result<T::Output, JoinError>`. Any other stage panics with "JoinHandle polled after completion".

**4. The not-yet-complete case**
- `can_read_output` (`harness.rs:422-464`) loads the state snapshot.
- If the task is not complete, it stores a clone of the caller's waker in the trailer (`harness.rs:432-461`). It does this with `set_join_waker` (`harness.rs:466-492`, `trailer.set_waker` plus `state.set_join_waker()`).
- It skips the write if the stored waker `will_wake` the new one (`harness.rs:439`).
- If the task completes during this step, the state update fails. The function then falls through and returns `true`, so the output is read immediately (`harness.rs:456-463`).
- Otherwise `JoinHandle::poll` returns `Pending`. Later, `complete` wakes the stored waker (step 2) and the handle is polled again.

**Uncertainty:** `core.rs:~590` for `wake_join` and `raw.rs:~360` for the vtable `try_read_output` are approximate. I took them from search output, and the line numbers in the search results were not consistent with my direct reads. I did not read `state.rs` beyond the search hits, so the exact bit transitions in `transition_to_complete` are unverified.