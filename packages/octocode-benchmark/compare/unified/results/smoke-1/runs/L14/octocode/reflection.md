1. **Helped:** The `localSearch` for `function debounce`, limited to `lodash.js`, gave the exact line (10403). The following `localFetch` with `startLine`/`endLine` 10403–10527 returned the whole function in one read. Two successful calls were enough.

2. **Did not help:** My first `localFetch` failed validation because I left out `goal` and `reasoning`. That wasted one call. I also didn't run `git rev-parse` or read `.git/HEAD`, so I never checked that the checkout is at the pinned commit 2b5e6f7. I relied on the prompt saying so.

   In my answer I said inner-function line numbers were "approximate offsets" and gave only the range. `localFetch` returned content without line numbers, so I couldn't cite specific lines like `shouldInvoke` or `debounced`. I did know the `debounced` declaration was at 10499, from the search hit.

3. **Next time:** I'd include `goal` and `reasoning` in every query from the start. I'd also run one `localSearch` for `function shouldInvoke`, `function trailingEdge` and similar names to get exact line anchors for each cited piece. I'd use `matchString` with `contextLines` if I wanted numbered output. I'd try to check the commit too.

4. **Confidence:** High on the behavior. It comes straight from the source I read, and the logic is short and unambiguous. Medium on the precise per-function line citations, since I gave only the range. I did not verify the pinned commit.