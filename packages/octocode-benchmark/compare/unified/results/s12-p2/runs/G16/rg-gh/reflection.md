1. **Helped:** The first call, `gh api -X GET search/issues` with `q='repo:prometheus/prometheus unix socket scrape'`, found PRs #18091 and #19399 and issue #12024 in one step. The second call fetched both PR bodies from `pulls/$n`. That gave the "Fixes #12024" line, the merge dates and SHAs, and the mix-up explanation directly from the authors' text.

2. **Did not help:**
   - Every Bash call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless but noisy.
   - My second call also ran a stray `compare/ea954809ce...main` check that I didn't need.
   - I never read the source or the diffs. The mix-up explanation therefore comes from the PR #19399 description, not from the code at `ea954809ce`.
   - The PR #18091 body has a "Fixes Fixes #12024" typo. I did not check whether GitHub actually linked and closed the issue.

3. **Next time:**
   - I'd check issue #12024's state and timeline. I'd also fetch the file list or diff of #19399 (`pulls/19399/files`) to cite `path:line` for the per-socket client cache.
   - I'd combine the two compare checks into one call.

4. **Confidence:** Medium-high. The PR numbers, issue number and ancestry check (both merge commits are "ahead" of `ea954809ce` per the compare API) are solid. The mix-up description is only as good as the PR text, and I did not verify it in the code.