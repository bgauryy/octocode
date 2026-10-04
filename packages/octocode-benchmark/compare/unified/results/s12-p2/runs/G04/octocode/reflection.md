**Helped:** A single `ghGetHistoryItem` call on pallets/click#3866 (body, files, patches) answered almost everything. It returned the PR body, `CHANGES.md`, the upgrade-guide text and the `core.py` diff (`_check_name_is_usable`, `_check_name_is_normalized`, and their call sites). That covered both what is deprecated and which declarations warn. I made no other calls.

**Did not help:**
- The patch output was truncated (`isPartial`, `contentPagination`). The `tests/test_arguments.py` patch was cut off mid-file, and I never read `test_options.py` or `test_deprecations.py`.
- The `core.py` hunks had elided context (`...`), so I had no line numbers and could not cite `path:line`.
- I did not confirm behavior at `mergeCommitSha`. I also did not check whether later changes, such as the #3876 branch resync, altered anything.
- My claim that arguments silently lower-case their declaration comes from a test, not from the `Argument` code in `core.py`.

**Next time:** I would follow up with `ghGetHistoryItem` using `patches.mode: selected` on `src/click/core.py`, with `minify: none`, to get unelided hunks. Then I would use `ghGetFileContent` at the merge commit for line numbers and to check the `Argument` lower-casing.

**Confidence:** High on what is deprecated and which declarations warn, because the diff and docs agree. Medium on the finer details (the argument lower-casing, test coverage) and on the lack of line citations.