**1. Helped:** The first `grep -n "tracing::\|trace!\|event" batch_semaphore.rs` listed every trace site with line numbers. The `sed -n 540,580p` plus `grep -n "fn assign_permits"` gave verified lines 559-566. The `sed -n 262,345p` call showed that `add_permits_locked` peeks with `queue.last()` and only pops after `assign_permits` returns. `gh api` on issue #8541 confirmed the reported mechanism.

**2. Did not help:** Every Bash call printed `/dev/null: Operation not permitted`, which was noise but did not break anything. I cut the issue body with `head -60`, so I never saw the PoC or the maintainers' discussion. I never read `Acquire::drop`, and I never checked for a fix PR.

**3. Next time:** Print `add_permits_locked` with `sed -n` or `rg -n`, so I have exact line numbers. Read the whole issue and its comments. Read the drop path.

**4. Confidence:** Medium-high on the core answer: the event is at `batch_semaphore.rs:559-566`, and it fires before the pop. Corrections to my answer:
- I wrote "about line 316" and "about line 326". Those were estimates, not verified lines, which breaks the no-guessing rule.
- I said the trace at lines 481-487 runs after `assign_permits`. It runs before it, so that sentence is wrong.
- The use-after-free step rests on the issue text, not on code I read.