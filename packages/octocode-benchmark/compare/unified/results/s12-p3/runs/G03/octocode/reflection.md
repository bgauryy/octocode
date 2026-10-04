1. **Helped:** One call, `ghGetHistoryItem` (operation `pullRequest`, number 5881, include `body` and `patches`), returned the PR description, all four file patches and the merged state. That was enough to answer, so I made no other calls. The PR body gave the symptoms and the code comments gave the mechanism.

2. **Did not help:**
   - I never read the merged files. `ghGetFileContent` at the merge commit would have given real `path:line` citations, and I didn't make that call.
   - The patch hunks were trimmed with `...`, so function names were missing. I wrote "`kDestroy`" for the hunk that destroys retiring clients, which is an inference from context, not something I saw. I also tied the `clientTtl` eviction path to `Pool` partly from the test names.
   - The tool output was fine. The gaps were mine.

3. **Next time:** After the PR read, I'd run one `ghGetFileContent` on `lib/dispatcher/pool-base.js` at the merge commit with `matchString` on `kRetireClient`. That would confirm the function names and give line numbers. The tool's `readAtMerge` hint pointed at exactly this call.

4. **Confidence:** Medium-high on the bug and the fix design, because the PR body, comments and tests agree. Medium on the exact symbol names and on the `clientTtl` detail, which I inferred. I gave no line numbers, and said so in my answer.