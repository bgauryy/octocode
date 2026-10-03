1. **Helped:** The first Bash call, `gh pr view 8156 --json title,state,body,files,...` piped together with `gh pr diff`, gave me the PR description, the file list and most of the diff in one step. The PR body already named the two open Miri issues, #5054 and #5047. The second call filtered the rest of the diff with `sed` and `grep -nE '^(diff|\+.*(ignore|miri|Miri))'`. That surfaced every new Miri ignore with its stated reason.

2. **Did not help:**
   - The first call's `head -400` cut the diff off, so I needed the second call to see the remainder.
   - The grep only showed `+` lines. It missed context, such as which test sits under each ignore in `tcp_stream.rs`, and I didn't check the multi-line `ignore` attributes.
   - I never opened the files at the merge commit, so I had no `path:line` citations. The answer has PR-diff evidence only.
   - I didn't check whether Miri issues #5047 and #5054 are still open, or look for follow-up PRs.

3. **Next time:** I'd fetch the whole diff once and save it to a file, then grep that file. I'd also run `gh api` on the two Miri issues and search for follow-up PRs, then use `gh api` or `git show` to get line numbers.

4. **Confidence:** Medium-high on what the PR changes. Medium on the completeness of the "still ignored" list, since it comes from a filtered grep.