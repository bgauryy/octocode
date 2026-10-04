**1. Helped**
- The first parallel pair was the most useful: `localSearch` for `arlastitems` found `arlastitemsCommand` at `src/t_array.c:1802`, and `ghGetHistoryItem` on issue #15874 gave the symptom, a reproduction and the fix PR number.
- `localFetch` on lines 1790-1900 gave the exact code with line numbers, so the citations come from bytes I read.
- `ghGetHistoryItem` on PR #15875 with `files` set to `src/t_array.c` returned a compact diff and showed all three changes.

**2. Did not help**
- `localSearch` was noisy: most hits were generated `commands.def` lines, and one value was truncated.
- The PR patch used `...` elisions, so I couldn't see full hunks.
- I never confirmed the checkout was really at 20bb2cfc54. I relied on the task statement, though the local lines matched the issue's quoted code.

**3. Next time**
- Skip the broad `localSearch` and go straight to `localFetch` with `matchString` for `arlastitemsCommand`.
- Read the PR's test files to confirm the new behavior, which I skipped.

**4. Confidence**
High on the location, cause and the step-cap and limit changes, since I saw both the code and the diff. Medium on the anchor change: it comes only from the diff, and the issue doesn't mention it.