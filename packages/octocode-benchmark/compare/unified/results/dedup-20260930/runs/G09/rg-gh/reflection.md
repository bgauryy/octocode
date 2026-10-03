**1. Helped:**
- The second Bash call, `gh api repos/pydantic/pydantic/issues/13786`, returned the full issue body. It held the root cause, the code snippet and the repro, so it carried most of the answer.
- The search-issues call (`gh api "search/issues?q=repo:pydantic/pydantic+13786..."`) found the three linked PRs: #13787, #13794 and #13825.
- The per-PR `gh pr view --json` loop gave the merge state, merge commit SHA and changed files in one call.

**2. Did not help:**
- The first call, `gh issue view 13786 --comments | head -100`, printed nothing. I never found out why. It may have been a `gh` error or a pipe problem.
- I never read the #13825 diff or the issue comments. I also didn't check which PR the maintainers actually chose, beyond the merged state. So I can't say how #13825 stops the mutation. I said so in the answer.
- The `--jq` tsv output came out as long single lines, which made it noisy to read.

**3. Next time:**
- Run `gh pr diff 13825 -R pydantic/pydantic` on `_config.py` and `_model_construction.py`. That would confirm the actual mechanism.
- Fetch the issue comments through `gh api .../issues/13786/comments`.
- Get the diff line numbers at the merge commit.

**4. Confidence:** Medium-high. The root cause comes straight from the issue text and the merged PR's own description, and the merge is confirmed. The details of how the fix works are unverified.