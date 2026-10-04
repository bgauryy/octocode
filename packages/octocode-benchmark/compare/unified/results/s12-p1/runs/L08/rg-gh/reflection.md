1. **Helped:** After `rg` failed, the `grep -n -i "sampleLimit|errSampleLimit"` on `scrape/scrape.go` located every relevant site in one call. The next batched call (`sed` ranges plus a `grep -A12` on `scrape/target.go`) showed `appenderWithLimits`, `limitAppender`, `checkAddError` and the post-loop handling together. A last `sed` and `grep` confirmed `limitAppenderV2` and the config field.

2. **Did not help:**
   - `rg` isn't installed, so my first call was wasted.
   - Every command printed `/dev/null: Operation not permitted`, which is noisy but harmless.
   - `git rev-parse HEAD` failed for that same reason, so I never confirmed the checkout is at `ea954809ce`. My final answer cited lines as pinned-commit lines without saying this.
   - I did not trace which `Rollback()` call handles a sample-limit failure, and I did not read the V2 scrape-loop path.

3. **Next time:** Use `grep` from the start. Confirm the commit with `git -C <path> log -1` or by reading `.git/HEAD` directly. Read the rollback and commit logic around `scrape/scrape.go:2055`. State the unverified commit in the answer.

4. **Confidence:** Medium-high on the mechanism, because I read the code directly. Medium on the line numbers, because the commit is unverified.