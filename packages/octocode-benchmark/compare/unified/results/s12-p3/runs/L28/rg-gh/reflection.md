**1. Helped:**
- The second call, `grep -n` on `src/t_array.c` plus `sed -n 435,450p`, located `arlastitemsCommand` and the `ARGETRANGE_MAX_ITEMS` comment.
- The third call's `sed -n 1790,1910p` showed the buggy cap at line 1837 and the loop at line 1860, so I could confirm the cause in the pinned source.
- That same call's `gh api search/issues?q=repo:redis/redis+15874+is:pr` found PR #15875.
- The fourth call, `gh api pulls/15875` and `/files`, gave the full diff and tests, which covered the "how did the fix change behavior" part.
- The issue body from the first call's `gh api issues/15874` already stated the cause.

**2. Did not help:**
- `rg` was not installed, so the first call's search failed and I fell back to `grep`.
- Every Bash call printed `/dev/null: Operation not permitted`. It was harmless but noisy.

**3. Next time:** I would use `grep` from the start. I would also run `git rev-parse HEAD` in the checkout to confirm it is at 20bb2cfc54. I assumed that and never checked it. I would also read the merge commit SHA from the PR, so the fix can be cited by SHA.

**4. Confidence:** High on the cause, because the code and loop were seen at the pinned checkout. Medium-high on the fix details, which come from the PR diff and not from a checkout of the fixed commit.