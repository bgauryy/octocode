**1. Helped:** The first Bash call, `gh pr view 8156 --json ...` piped into `gh pr diff`, gave me the PR body, the file list and the full diff in one round trip. The body already named the two open Miri issues (#5047 and #5054). The second call filtered the truncated remainder of the diff with `awk` and `grep` for miri-related added and removed lines. That produced the list of remaining ignores and their reasons.

**2. Did not help:**
- The first call cut the diff off at 20KB with `head -c`, so I needed the second call. I should have filtered for miri lines from the start.
- The grep output had no surrounding context. I couldn't tell which test each ignore belonged to, or how many tests were ignored.
- I never opened the linked Miri issues, so I don't know if they have been fixed.
- I never checked out the code, so nothing was verified against the actual file at the merge commit.

**3. Next time:** I'd run one `gh pr diff | grep -B3 -E 'miri'` to keep test names attached to each ignore. I would also run `gh issue view` on rust-lang/miri#5047 and `gh pr view` on #5054 to check their current status. To confirm the remaining gates I would read the files at the merge commit, for example with `gh api`.

**4. Confidence:** Medium-high. The changes and the stated reasons come straight from the diff and PR body. The per-test lists are not exhaustive, and the issue status is unchecked.