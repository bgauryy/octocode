**1. Helped:** I made one `ghGetHistoryItem` call on PR 5881 with `body`, `changedFiles` and `patches: {mode: "all"}`. It returned the PR description, the full patches for `pool-base.js`, `pool.js`, `round-robin-pool.js` and the new test file, all in one response. The author's inline comments in the patch explained the cause. That was enough to answer the question, and I made no other calls.

**2. Did not help:** Nothing failed and nothing was repeated. The patches were minified with `...` elisions, so I couldn't cite line numbers, and I said so. I did not check the claim about `kOnDrain` in the source. The PR body was thin: its Bug Fixes section said "N/A" and it linked no issue.

**3. Next time:** I would also call `ghGetFileContent` on `lib/dispatcher/pool-base.js` at the merge commit. That would confirm the `kOnDrain` behavior and give real `path:line` citations. I would also run `ghSearchHistory` for a linked issue to see the original report.

**4. Confidence:** Medium-high. The PR patch and the author's comments state the mechanism directly. The residual uncertainty is the missing line citations and the unread `kOnDrain` code.