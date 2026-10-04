1. **Helped:** The `grep -n -E '\bgo (func|[a-zA-Z_.]+\()' scrape/*.go | grep -v _test.go` call found all 7 `go` statements in one shot. The combined `sed -n` call then showed the surrounding code for each one.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so it failed.
   - The `git log -1` in that call also failed. Every Bash call printed `/dev/null: Operation not permitted`. I never confirmed the checkout was at `ea954809ce`, and my answer didn't say so. I only assumed the checkout was pinned, and the line numbers rest on that.
   - The second grep repeated the first.
   - I never read the full `reloader` loop body or the `ApplyConfig` goroutine body. I flagged this in the answer, but the "reloads the pool" claim is still inferred.
   - My opening sentence said "Five functions" and then listed six. I should have counted before writing.

3. **Next time:** Check that `rg` exists, use `grep` from the start, and read `git rev-parse HEAD` or `.git/HEAD` directly to confirm the commit. I'd read the complete goroutine bodies and write the answer once, with the count checked.

4. **Confidence:** Medium-high on the list of 7 goroutines and the line numbers. Medium on the descriptions of `reloader` and the `ApplyConfig` goroutine, since I read only part of each. The commit pin is unverified.