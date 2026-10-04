**Helped:** The first `localSearch` was the fastest step. One regex over `runtime/task` for the function names (`try_read_output`, `store_output`, `complete`, `set_join_waker`, `wake_join`) gave me the whole call chain with line numbers. The batched `localFetch` calls with line ranges on `harness.rs`, `core.rs` and `join.rs` then read the deciding code directly. The `localSearch` with `contextLines` on `join.rs` found the `JoinHandle` entry point cheaply.

**Did not help:**
- `localFetch` with `matchString: "fn poll(self"` on `core.rs` returned `noMatches`. I guessed the signature wrong and never located `Core::poll`.
- Multi-range fetches silently omitted the middle ranges (216-274, 376-459, 465-524). I never read those lines, though I read around them.
- My ranges for `poll_future` were guesses, so I cited it as approximate (`~525-559`) and never saw its function signature line.

**Next time:** I would search for `fn poll_future` first to get exact lines. I would fetch `state.rs` `transition_to_complete` and the `mod.rs` rules directly, instead of leaving them as caveats.

**Confidence:** Medium-high on the mechanism, since every step was seen in fetched code. Lower on exact line numbers for `poll_future` and `join.rs:346`. My closing remark that the unread part of `join.rs` is "coop budget handling" was a guess, not something I saw, so please disregard it.