1. **Helped:** The first call, `rg -n -i "lifo" worker.rs`, found nearly every relevant site at once (the constant, the slot field, the poll loop, `schedule_local`). The second call, a batched `sed -n` over those ranges plus an `rg` for `disable_lifo_slot` in `builder.rs`, gave me the code and comments. The comments state the intent (locality, ping-pong starvation), so I needed no history lookups.

2. **Did not help:**
   - Both calls printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless noise, but it could hide real errors.
   - I never ran `git rev-parse HEAD` in the checkout, so I did not confirm it was at facc6fc.
   - I batched several `sed` ranges without `-n` line numbers (`rg -n` was only used in the first call). Some of my citations are therefore inferred from range starts, not read directly: `worker.rs:1359-1373`, `~790`, and the exact start of the `schedule_task` body.
   - I said the option is multi-thread-only without checking `builder.rs`.
   - I did not read `Handle::schedule_task`'s callers or the waker path.

3. **Next time:** Run `git rev-parse HEAD` first. Use `rg -n` or `cat -n` for every range I cite. Read `builder.rs:1410-1430` and the callers of `schedule_task` before claiming anything about them.

4. **Confidence:** Medium-high on the mechanism and the main limits (the 3-poll cap, coop budget, yield, same-core requirement), because I read that code directly. Medium on the individual line numbers and the "multi-thread-only" claim.