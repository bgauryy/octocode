**1. Helped:**
- The first `localSearch` for "lifo" in `multi_thread/` returned every relevant anchor at once: the constant at `:269`, the slot take at `:727` and the disable at `:764`.
- The batched `localFetch` of `:695-810` returned the whole run loop, including the poll cap, the budget check and the core-stolen break.
- The `localSearch` with a regex alternation on `worker.rs` pointed me to `schedule_local` at `:1392`. The `localFetch` of `:1340-1440` then gave me the push logic and the notify rule.

**2. Did not help:**
- My first `localSearch` failed validation because I omitted `goal` and `reasoning`. That cost one call.
- I put a pointless `startLine: 1` fetch in the batched `localFetch`. It returned one line.
- `localFetch` returned line ranges but didn't prefix each line with its number. I had to count offsets to get citations like `:757-762` and `:1398-1408`, which is error-prone.

**3. Next time:**
- Include `goal` and `reasoning` on every query from the start.
- Skip filler queries.
- Read `next_task` and the stealing code, which I skipped.

**4. Confidence:** medium-high.
- The mechanism and the limits come from code I read directly.
- The exact line citations are hand-counted and may be off by a line or two.
- "The slot isn't stealable" rests only on the comment at `:478-480`, not on the steal code.