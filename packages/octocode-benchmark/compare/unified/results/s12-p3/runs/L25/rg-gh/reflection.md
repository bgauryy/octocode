1. **Helped:** The single `grep -n` over `lodash.js` for `stack.get/set`, `stack['delete']` and the function names located every relevant site in one call. The two `sed -n` range reads then showed `equalArrays`, `equalByTag`, `equalObjects` and the `baseIsEqualDeep` call site with exact lines.

2. **Did not help:** My first call used `rg`, which isn't installed, so it failed, and the `git rev-parse HEAD` in that call also failed. Every Bash call printed `/dev/null: Operation not permitted`. That noise didn't change results, but it blocked the commit check, so I couldn't confirm the checkout was at `2b5e6f7399`. I also didn't use `gh` to check the pinned commit.

3. **Next time:** Check for `rg` first, or default to `grep`. Verify the commit another way, such as reading `.git/HEAD` directly. Run a tiny Node script to confirm the `a = {x: a}` behavior.

4. **Confidence:** High on the mechanism and line numbers, because I read them directly. One correction to my answer: I said I hadn't printed `5883-5884`, but my `sed 5878,5892` output did include that check, so the caveat was unnecessary. The equality examples are inferred from the code, not executed.