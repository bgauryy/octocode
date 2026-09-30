# octocode: reflection after 2 questions

# REFLECT.md

## What helped

- **G03:** A single `ghGetHistoryItem` call on PR 5881 with `body`, `changedFiles` and `patches: {mode: "all"}` returned the PR description and full patches for `pool-base.js`, `pool.js`, `round-robin-pool.js` and the new test file. The author's inline comments in the patch explained the cause. That one call was enough to answer.
- **L14:** `localSearch` for `function debounce`, limited to `lodash.js`, gave the exact line (10403). A `localFetch` with `startLine`/`endLine` 10403–10527 then returned the whole function in one read. Two successful calls were enough.

## What did not help

- **G03:**
  - The patches were minified with `...` elisions, so no line numbers could be cited, and the answer said so.
  - The `kOnDrain` claim was not checked against the source.
  - The PR body was thin: its Bug Fixes section said "N/A" and it linked no issue.
- **L14:**
  - The first `localFetch` failed validation because `goal` and `reasoning` were missing. That cost one call.
  - `localFetch` returned content without line numbers. Inner functions such as `shouldInvoke` and `debounced` could only be given as approximate offsets within the range. Only the `debounced` declaration (10499) was known, from the search hit.
  - The checkout was never verified to be at the pinned commit 2b5e6f7. The prompt was trusted.

## Patterns

- Both answers were reached with very few calls (one in G03, two successful in L14).
- Both left line-level citations weaker than the prompt requires. G03 got them from minified patches and L14 from unnumbered fetch output.
- Both skipped a cheap verification step: the `kOnDrain` source in G03 and the pinned commit in L14.

## Suggested changes

1. **Line-anchor step in the instructions.** After reading content, run a `localSearch` or `matchString` with `contextLines` for each function or symbol cited, so every claim has an exact `path:line`. This would have fixed the citation gaps in both G03 and L14.
2. **Numbered output from `localFetch`.** Return line numbers with fetched content, or make that an option. This removes the need for extra searches just to get anchors.
3. **Follow-up read for PR-based answers.** When a PR patch is used, also fetch the touched file at the merge commit (`ghGetFileContent`) to confirm behavior and get real line numbers. Optionally run `ghSearchHistory` for a linked issue.
4. **Required fields up front.** State in the instructions that `goal` and `reasoning` are required on every query, or make the tool's schema error say so more clearly. This would have avoided L14's failed call.
5. **Pinned-commit check.** Give a cheap way to confirm a local checkout's commit (for example reading `.git/HEAD`), or state explicitly that the prompt's pin can be trusted.
6. **Patch elision.** Make the `...` elisions in `ghGetHistoryItem` patches less lossy, or flag them clearly, so line-number limits are visible up front.

---

# Per-question reflections

## G03

**1. Helped:** I made one `ghGetHistoryItem` call on PR 5881 with `body`, `changedFiles` and `patches: {mode: "all"}`. It returned the PR description, the full patches for `pool-base.js`, `pool.js`, `round-robin-pool.js` and the new test file, all in one response. The author's inline comments in the patch explained the cause. That was enough to answer the question, and I made no other calls.

**2. Did not help:** Nothing failed and nothing was repeated. The patches were minified with `...` elisions, so I couldn't cite line numbers, and I said so. I did not check the claim about `kOnDrain` in the source. The PR body was thin: its Bug Fixes section said "N/A" and it linked no issue.

**3. Next time:** I would also call `ghGetFileContent` on `lib/dispatcher/pool-base.js` at the merge commit. That would confirm the `kOnDrain` behavior and give real `path:line` citations. I would also run `ghSearchHistory` for a linked issue to see the original report.

**4. Confidence:** Medium-high. The PR patch and the author's comments state the mechanism directly. The residual uncertainty is the missing line citations and the unread `kOnDrain` code.

## L14

1. **Helped:** The `localSearch` for `function debounce`, limited to `lodash.js`, gave the exact line (10403). The following `localFetch` with `startLine`/`endLine` 10403–10527 returned the whole function in one read. Two successful calls were enough.

2. **Did not help:** My first `localFetch` failed validation because I left out `goal` and `reasoning`. That wasted one call. I also didn't run `git rev-parse` or read `.git/HEAD`, so I never checked that the checkout is at the pinned commit 2b5e6f7. I relied on the prompt saying so.

   In my answer I said inner-function line numbers were "approximate offsets" and gave only the range. `localFetch` returned content without line numbers, so I couldn't cite specific lines like `shouldInvoke` or `debounced`. I did know the `debounced` declaration was at 10499, from the search hit.

3. **Next time:** I'd include `goal` and `reasoning` in every query from the start. I'd also run one `localSearch` for `function shouldInvoke`, `function trailingEdge` and similar names to get exact line anchors for each cited piece. I'd use `matchString` with `contextLines` if I wanted numbered output. I'd try to check the commit too.

4. **Confidence:** High on the behavior. It comes straight from the source I read, and the logic is short and unambiguous. Medium on the precise per-function line citations, since I gave only the range. I did not verify the pinned commit.
