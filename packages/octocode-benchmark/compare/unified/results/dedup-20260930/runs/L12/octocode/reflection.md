**1. Helped:**
- The first `localSearch` for "lifo" (case-insensitive, content view) put me on `worker.rs` and gave line anchors for the cap constant and the loop.
- The next three `localFetch` calls ran in parallel. They were a line range 700-810 for the run loop, a `matchString` on `fn schedule_local` with 55 context lines for the push side, and lines 262-272 for the constant. Those reads answered nearly the whole question.

**2. Did not help:**
- `localSearch` returned only page 1 of 5, so I never saw the other hits, such as `worker.rs:481` and the tests.
- `localFetch` returns text without line numbers. I derived line numbers by counting from the window start. I also said the `schedule_task` line numbers (~1350-1358) were approximate, but I never read lines 1330-1384 with real anchors.
- I did not check whether other workers can steal from the slot. I did not check the docs or history for why `MAX_LIFO_POLLS_PER_TICK` is 3.

**3. Next time:**
- Use `matchString` with a small context, or `lspSearch` documentSymbols, to get exact line numbers.
- Search for `lifo_slot` and `steal` to settle the stealing question.
- Read the `disable_lifo_slot` builder docs.

**4. Confidence:** high on the mechanism and the limits, because I read that source directly. Medium on the exact line numbers for `schedule_local` and the early `schedule_task` lines. Those are my estimates, and I said so in the answer.