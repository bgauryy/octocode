1. **Helped:** The `gh api -X GET search/issues` query (`repo:... is:pr unix socket scrape`) surfaced both PRs, #18091 and #19399, in one call. The second call fetched both PR bodies with `--jq`. It gave the "Fixes #12024" line, the release note, merge SHAs and the root-cause description. The same call also fetched the date of `ea954809ce`.

2. **Did not help:**
   - Every Bash call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless but noisy, and it made me wonder whether the output was complete.
   - I never opened issue #12024, so I can't confirm it was closed.
   - I never verified that the merge commits are ancestors of `ea954809ce`. I inferred that from dates only, and I never used the compare endpoint.
   - I didn't read any source at the pinned commit, so I have no `path:line` evidence.
   - The PR body printed a lot of template boilerplate.

3. **Next time:**
   - Add `gh api repos/prometheus/prometheus/compare/<sha>...ea954809ce` to confirm ancestry.
   - Fetch issue 12024 to confirm its state.
   - Grep the code with `gh api` contents or search for the per-socket client cache, to get `path:line` citations.
   - Truncate PR bodies with `--jq` and `head`.

4. **Confidence:** Medium-high. The PR numbers, SHAs and root cause come directly from PR text. The issue closure and presence at the pinned commit are inferred.