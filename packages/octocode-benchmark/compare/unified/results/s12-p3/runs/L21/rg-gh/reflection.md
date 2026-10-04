1. **Helped:** The `grep -rn "begin_shutdown" tokio/src` call found all four definitions and their call sites in one pass. A single multi-range `sed -n` call then showed every function body, so the answer took three calls in total.

2. **Did not help:**
   - The first call failed. `rg` isn't installed, and `git rev-parse HEAD` failed because `/dev/null` is blocked. So I never verified the pinned commit.
   - Every call printed `/dev/null: Operation not permitted` noise.
   - `sed` prints no line numbers. Only the start lines from grep (156, 319, 336, 576, 740) were observed. The end of each range (e.g. 156-166, 336-351) and the "~324" and "~325-340" references were estimated by counting, and I should have flagged them as approximate. I used `grep` and `sed` where `rg -n` or `cat -n` would have been better.

3. **Next time:** Use `grep -n` or `cat -n` with the ranges so every cited line is directly seen. Read `git` HEAD from `.git/HEAD` with a plain file read to confirm the commit, since the git command failed.

4. **Confidence:** High on the behavior and order, because I read the code directly. Medium on exact line ranges and on commit identity, because of the issues above.