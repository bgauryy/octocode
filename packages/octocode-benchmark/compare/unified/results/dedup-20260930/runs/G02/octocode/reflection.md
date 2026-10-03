**1. Helped:** The first `ghGetHistoryItem` call (PR overview with `changedFiles` and body) listed all 31 files in one go and gave the intent (constraints, issue #13704). The second call, with `patches.mode: selected` on `validators/counter.rs`, `_generate_schema.py`, `_validators.py`, the docs and the Python tests, gave nearly all the evidence. Those diffs showed the before/after schema wiring, the validator logic, the strict-mode tests and the error type. Two calls were enough.

**2. Did not help:**
- The test file patch was truncated (`hasMore`, `nextCharOffset` 5075). I didn't fetch the rest, so `test_counter_json_schema` and whatever follows it are unread.
- I didn't read the old `_mapping_schema` body or the diffs for `json_schema.py`, `_known_annotated_metadata.py`, and the serializer and input files. I described those parts from file names, the PR description and test assertions.
- PR diffs only show changed lines. They don't show the unchanged old code that explains the before-behavior.

**3. Next time:** I would read `_mapping_schema` at the parent commit with `ghGetFileContent` (`matchString`). I would also fetch the truncated patch continuation, and read the `json_schema.py` and `_known_annotated_metadata.py` diffs in a third call. Doing that would have grounded the before/after comparison.

**4. Confidence:** Medium-high on the after-behavior, because it is directly backed by the diff and the tests. Medium on the before-behavior, because I inferred it from the removed lines and did not read the old implementation.