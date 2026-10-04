1. **Helped:** The first call, `gh api -X GET search/issues -f q='repo:prometheus/prometheus unix socket scrape ... is:pr'`, surfaced both PRs (#18091 and #19399) right away. The second call fetched both PR bodies with `--jq`. The #19399 body explained the mix-up directly, and the #18091 body gave the issue reference. The same call also returned the commit date for `ea954809ce`.

2. **Did not help:**
   - Every Bash call printed `/bin/bash: /dev/null: Operation not permitted`. It was noise, not a failure.
   - I never confirmed that `ea954809ce` contains the two merge commits. I only compared dates, which is weaker than an ancestry check.
   - I never opened issue #12024 to confirm it was closed by #18091, and I read no source, so there are no `path:line` citations.
   - The #18091 body says "Fixes Fixes #12024", which looks like a typo and which I did not cross-check.
   - The search was keyword-based, so it could have missed other later fixes.

3. **Next time:**
   - Run `gh api repos/prometheus/prometheus/compare/<merge_sha>...ea954809ce` to check ancestry.
   - Fetch `issues/12024` to confirm it closed and which PR closed it.
   - Check the timeline or other PRs that touch the unix socket code.

4. **Confidence:** Medium-high. The PR numbers, the issue number and the mix-up description come straight from the PR bodies. The ancestry check and the "closed by" link are the unverified parts, and I stated that in my answer.