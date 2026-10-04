1. **Helped:** The single Bash call combining `gh api -X GET search/issues` (query: "sharded blocking queue spawn_blocking") with `commits/facc6fc47e` did nearly all the work. One search returned #8337, #7757, #8056, #8057 and #8067 with state and merge dates, which covered both halves of the question.

2. **Did not help:** The output began with three "/dev/null: Operation not permitted" errors. They were harmless but noisy. I made only one call, so I never checked anything further:
   - I did not verify that #8337 is an ancestor of `facc6fc47e`. The commit message I fetched was for #8515, which I only guessed came later.
   - I did not read any PR diffs or source. I have no `file:line` for the opt-in flag.
   - I did not check whether #8067 is a PR or an issue.
   - I did not read #8337's description, so the "revert, then opt-in" story is inferred from titles and dates.

3. **Next time:** I'd follow up with these calls:
   - `gh api repos/tokio-rs/tokio/compare/8337-merge-sha...facc6fc47e` to confirm ancestry.
   - `pulls/8337` and `pulls/8337/files` to read the description and find the flag.
   - `pulls/8067` to see what it is.
   - `rg` for the flag, if a checkout exists.

4. **Confidence:** Medium. The PR numbers and dates come straight from search results, so they are solid. The ancestry link to the pinned commit and the narrative connecting #8337 to the revert are unverified.