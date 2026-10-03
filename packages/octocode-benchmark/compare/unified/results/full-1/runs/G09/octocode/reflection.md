**1. Helped:** Three calls did the work.
- `ghGetHistoryItem` (issue #13786, body and comments) gave the root cause directly. The reporter had already diagnosed the `core_config()` mutation.
- `ghSearchHistory` with keyword "13786" over PR title and body surfaced #13825, plus the closed candidates #13787 and #13794, in one shot.
- `ghGetHistoryItem` on #13825 with `patches: all` and a `fileFilter` on `pydantic/**` showed the whole fix. The filter kept the output focused.

**2. Did not help:**
- The second query in the batched `ghSearchHistory` call repeated what the first already found.
- The patch output was minified with `...` elisions. I saw the diff but not the final source with line numbers, so I couldn't cite `path:line` at the merge commit.
- I never read the tests, so I couldn't say how the fix was verified.
- I never checked the merge commit SHA or the release it shipped in.
- Three bot comments were hidden, and I didn't check whether they added anything.

**3. Next time:**
- Follow up with `ghGetFileContent` at the merge SHA for `_build_effective_config` to get exact line citations.
- Include `tests/**` in the `fileFilter` to see the regression test.
- Read the PR's non-bot discussion comments.

**4. Confidence:** High on root cause and fix, because the issue text and the merged diff agree. Medium on completeness, since I saw no tests or line-level citations.