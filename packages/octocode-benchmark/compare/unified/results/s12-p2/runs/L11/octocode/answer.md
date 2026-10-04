The output never travels through a channel. It sits in the task's own heap cell (`Core.stage`), and the `JoinHandle` reads it from there once atomic state bits say the task is complete. A waker stored in the task's trailer tells the awaiting task when to look. All paths below are under `tokio/src/runtime/task/`.

**1. The task writes its output into its own cell**
- `poll_future` polls the future. If it returns `Ready(output)`, or panics, the result is wrapped as `Ok(output)` or `Err(JoinError)`. It is then stored with `core.store_output(output)` (`harness.rs:523-559`; the store is at `harness.rs:551`).
- `Core::store_output` calls `set_stage(Stage::Finished(output))`, which overwrites the `stage` field (`core.rs:408-413`, `core.rs:432-435`).
- Cancellation takes the same route: it stores `Err(JoinError::cancelled)` via `store_output` (`harness.rs:508`).

**2. The task is marked complete and the waiter is woken**
- `Harness::complete()` calls `state().transition_to_complete()` (`harness.rs:331-334`).
- If the `JoinHandle` has been dropped (`!is_join_interested()`), the task drops the output itself (`harness.rs:339-344`).
- Otherwise, if `is_join_waker_set()`, it calls `trailer().wake_join()`, which runs `waker.wake_by_ref()` on the waker stored in the `Trailer` (`harness.rs:345-349`, `core.rs:581-586`).
- It then calls `unset_waker_after_complete()`. If the handle was dropped meanwhile, it drops the waker itself (`harness.rs:355-363`).

**3. The `JoinHandle` polls and reads the output**
- `JoinHandle::poll` starts with `ret = Poll::Pending` and calls `self.raw.try_read_output(&mut ret, cx.waker())`. This goes through the task vtable, so the output type is erased and the result is returned through the `*mut` out-pointer (`join.rs:327-347`).
- That reaches `Harness::try_read_output`. If `can_read_output(...)` returns true, it sets `*dst = Poll::Ready(self.core().take_output())` (`harness.rs:281-285`).
- `take_output` swaps `Stage::Consumed` into the cell and returns the `Finished(output)` value. Any other stage panics with "JoinHandle polled after completion" (`core.rs:420-430`).

**4. The task isn't complete yet**
- `can_read_output` loads the state snapshot. If `!is_complete()`, it stores `cx.waker()` in the trailer and sets the `JOIN_WAKER` bit, then returns false, so the poll stays `Pending` (`harness.rs:422-463`).
- If a waker is already stored and `will_wake` says it matches, it returns early. Otherwise it unsets the old waker and stores the new one (`harness.rs:439-448`).
- `set_join_waker` writes the waker into the trailer, then calls `state.set_join_waker()`. If that fails because the task completed meanwhile, it clears the waker, and `can_read_output` returns true (`harness.rs:466-492`, `harness.rs:456-463`).

The `JOIN_WAKER` and `COMPLETE` bits in the state decide which side may touch the waker field at any moment. `harness.rs:312-320` and `harness.rs:345-348` refer to these as "rule 2" and "rule 4" in `task/mod.rs`. I did not read `mod.rs` itself or `state.rs`.