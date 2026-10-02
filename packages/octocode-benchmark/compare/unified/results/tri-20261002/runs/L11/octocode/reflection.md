**1. Helped:** The first `localSearch` was the fastest step. One regex over `runtime/task` for the function names (`store_output|take_output|try_read_output|complete|...`) located every relevant function across `core.rs`, `harness.rs`, `raw.rs`, `join.rs` and `state.rs` in one call. `localFetch` with explicit line ranges then gave the deciding bodies (`complete`, `can_read_output`, `take_output`, `poll_future`). The `localSearch` on `join.rs` for `try_read_output|coop` showed the `JoinHandle::poll` call site with context.

**2. Did not help:**
- One `localFetch` failed on a malformed range (`"404,440"`), and I had to redo it.
- Multi-range fetches elided middle sections ("lines 201-274 omitted"), which forced follow-up fetches.
- The `store_output` search returned line numbers only. I then needed a separate fetch for `poll_future`.

**3. Next time:** I would use correct `"start-end"` ranges from the start. I would also read `raw.rs:290-360` (the vtable dispatch), `state.rs` (`transition_to_complete`, the bit definitions) and the rules in `task/mod.rs`.

**4. Confidence:** High on the core flow: store in `Core.stage`, wake via the trailer waker, `take_output` on poll. I read each of those lines. Medium on two points. "Dispatches through the vtable" is inferred from `raw.rs` search hits that I never opened. The state-bit semantics rely on code comments rather than `state.rs`. I flagged the second in the answer but not the first.