1. **Helped:** Saving the PR diff with `gh pr diff 13824 > /tmp/d.txt`, then running `grep -n '^diff'` to index it. I could then `sed` straight to the files that mattered: `validators/counter.rs`, `input_python.rs`, `_generate_schema.py`, `_known_annotated_metadata.py`, `_validators.py`, and `tests/types/test_counter.py`. The new tests were the most useful part, because they state the expected behavior directly.

2. **Did not help:**
   - The first `gh pr view --json ... | head -c` call dumped truncated file-list JSON and got persisted to a file, so I got little from it. I had to make a second call for the merge SHA.
   - I never looked at the pre-PR code. I did not check out or view `_mapping_schema` at the parent commit, so the "before" behavior is inferred from the removed lines, and I said so in the answer.
   - I did not run the tests or look for follow-up commits.

3. **Next time:** I would fetch the base-commit source with `gh api` or `git show`, or check whether a local pydantic checkout exists. That would let me confirm the old error type, strict-mode behavior and constraint handling. I would also search the diff for removed test expectations in `tests/types/test_counter.py`, which show the old behavior.

4. **Confidence:** High for the after-PR behavior, since I read the diff and the tests. Medium for the before/after comparison, because the "before" side is inferred. It is not verified.