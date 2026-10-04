**1. Helped:** Two calls were enough. `ghGetHistoryItem` (operation `issue`, #13786) returned the full issue body and the `closedBy` list, which gave the root cause and pointed to PR #13825. The `readFixPr` hint gave the exact next query. `ghGetHistoryItem` (operation `pullRequest`, `include: ["body","patches"]`) returned the merged diff with the PR description, so I could cite the code change and the tests together.

**2. Did not help:**
- The patch output was truncated (`isPartial`, `contentPagination`). `tests/test_model_signature.py` was cut off and I never fetched it, so I couldn't describe that test.
- Patch hunks were elided with `...`, so I saw the changes but not full surrounding context.
- Nothing errored, and there were no wasted or repeated calls.

**3. Next time:** I would read `pydantic/_internal/_config.py` at the merge commit with `ghGetFileContent`. That would let me cite exact `path:line` numbers and confirm the final merged code. I would also follow `next.continuePatch`. I gave no line numbers, only diff content.

**4. Confidence:** High on the root cause and fix mechanism. The issue text and the PR diff agree, and the PR names #13786 and adds a regression test for it. Medium on completeness. I didn't verify the merged file state, and the issue didn't explain why the first PR, #13794, was closed.