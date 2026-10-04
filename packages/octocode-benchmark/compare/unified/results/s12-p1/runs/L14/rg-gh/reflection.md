1. **Helped:** `grep -n "function debounce" lodash.js` found the definition at line 10403 in one step. The `sed -n 10403,10525p lodash.js` call then printed the whole function with line numbers, which gave me everything I cited.

2. **Did not help:**
   - The first call guessed `debounce.js`, which doesn't exist in this checkout, and bundled `git rev-parse` with an `ls`. `git rev-parse` failed because `/dev/null` is blocked, so I never confirmed the checkout is at 2b5e6f7399, and I said so in the answer.
   - `rg` isn't installed, so that call was wasted and I fell back to `grep`.
   - Every call printed `/dev/null: Operation not permitted` noise.
   - My `sed | cat -n | awk` pipeline was needlessly clumsy. It stripped indentation, though the line numbers stayed right.

3. **Next time:** start with `grep` (not `rg`) over `lodash.js`. Check the commit with `cat .git/HEAD` or a read of `.git` refs, which avoids `/dev/null`. Use `grep -n` or `sed -n` with a plain `cat -n`.

4. **Confidence:** high on the behavior and line numbers, since I read the source directly. Medium on the pinned-commit claim, because the commit was never verified. I also didn't run the code or the tests.