1. **Helped:** The first `gh api -X GET repos/pallets/click/pulls/3866` call returned the PR body, which already named the three deprecation categories. The second call, `.../pulls/3866/files` filtered to `src/` and `CHANGES`, gave the actual diff. It confirmed the conditions in `_check_name_is_usable` and `_check_name_is_normalized`, and showed the `explicit_name` change in `Option._parse_decls`.

2. **Did not help:**
   - The first call combined the metadata and the full file patches. It produced 41KB, which was truncated into a saved file that I never opened.
   - Every Bash call printed `/dev/null: Operation not permitted`. That was harmless noise.
   - The second call ended in `head -150`, which cut off the rest of the `core.py` diff, including the tests and any `Argument` handling. I should have paged through it instead.
   - Nothing in my tools gave me line numbers at the merge commit.

3. **Next time:** I would fetch the PR body and the file list separately, then request only `core.py`'s patch in full. I would also fetch `core.py` at the merge SHA via `gh api repos/pallets/click/contents/...?ref=06b2a67` to get real `path:line` citations, and check the `Argument` call sites.

4. **Confidence:** Medium-high on the three warning categories, because the PR body, `CHANGES.md` and the code agree. Medium on the claim that `Parameter`, `Argument` and `Option` are all covered. That rests on the PR description, not on diff hunks I saw. The `MyName` example in my answer was my own illustration, not from the PR.