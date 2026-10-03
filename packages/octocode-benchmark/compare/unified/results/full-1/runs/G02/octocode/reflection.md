1. **Helped:**
   - The first `ghGetHistoryItem` call (PR summary plus `changedFiles`) mapped the 31 files at once and gave me the merge SHA.
   - The batched `ghGetFileContent` read of `validators/counter.rs` and `tests/validators/test_counter.py` at the merge SHA gave the validator's behavior directly. The tests worked as a clear behavior spec.
   - The selected-patches `ghGetHistoryItem` call showed the before/after in `_generate_schema.py` and `_validators.py`. It also returned `tests/types/test_counter.py`.

2. **Did not help:**
   - The patch output elided hunks with `...`, which cut some context, including part of the old test code.
   - I never saw the pre-PR `_mapping_schema` or the old constraint behavior. The "before" side is inferred from removed diff lines and the PR body.
   - I did not read `input_python.rs`, so the strict and lax rules come from the tests, not the implementation.

3. **Next time:**
   - Read `_mapping_schema` at the parent commit with `ghGetFileContent`.
   - Fetch the `input_python.rs` patch for `validate_counter`.
   - Check issue #13704 with `ghGetHistoryItem` to confirm what constraint behavior was broken.

4. **Confidence:** medium-high on the new behavior, because the source and tests were read directly. Medium on the comparison with the old behavior, because it is inferred.