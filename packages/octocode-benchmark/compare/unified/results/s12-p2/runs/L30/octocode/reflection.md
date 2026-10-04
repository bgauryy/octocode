**1. Helped:**
- `ghGetHistoryItem` (PR 8546, with files and patches) returned the full diff and description in one call. That gave me the three fix sites (`has_value`, `grow`, `reclaim_blocks`).
- A single batched `localSearch` for the pre-fix literals confirmed all three at exact lines in the checkout. The literals were `fn has_value`, `start_index + `, `Block::new(` and `required_index > self.index`. That was enough to show the bug is still present.

**2. Did not help:**
- The `list.rs` patch came back elided with "..." between hunks. The key lines were visible, but I could not see the full context. The hint suggested a re-read, which I skipped.
- The `localSearch` also matched a doc comment at `block.rs:27` and `list.rs:53`. These were minor noise.
- I had no tool to check whether facc6fc47e descends from the PR's merge commit, so ancestry stayed unverified. The PR targets `tokio-1.51.x`, which makes that gap more relevant.

**3. Next time:**
- I would re-read the `list.rs` patch with `mode: selected` to see the full `reclaim_blocks` hunk.
- I would also try `ghSearchHistory` or a commit lookup to place facc6fc47e relative to the merge.

**4. Confidence:** High that the bug is still present. I saw the old code at specific lines in the checkout, and the PR diff shows exactly what replaces it. Medium on the "test-only helpers absent" claim. I only observed `channel()` using `Block::new(0)`, and did not search for `channel_from_index` directly.