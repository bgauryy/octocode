**1. Helped:**
- `ghGetHistoryItem` on PR #8546 with body, files and patches returned the whole diff in one call. That gave me every hunk and the PR's own explanation.
- One `localSearch` regex alternation, covering both the old (unfixed) and new (fixed) forms of each line, showed the state of every affected spot in a single pass.

**2. Did not help:**
- The PR patch was trimmed with `...` in `list.rs`, so I couldn't see every hunk in full.
- I only fetched `has_value` with `localFetch`. The `Block::grow` line (`block.rs:361`) and `reclaim_blocks` (`list.rs:405`) rest on search snippets, not full function reads.
- I stated that `channel_from_index` and the `wraparound` tests are absent from the checkout. My search patterns did not include those names, so that claim is inferred, not verified. I should have run a search for them.
- I never confirmed how the pinned commit relates to the `tokio-1.51.x` target branch.

**3. Next time:** I'd add `channel_from_index|wraparound` to the same `localSearch`. I'd batch `localFetch` reads of all three affected functions.

**4. Confidence:** High that the bug is present, because the unfixed lines are visible in the checkout. Medium on the claim that the test helpers are absent.