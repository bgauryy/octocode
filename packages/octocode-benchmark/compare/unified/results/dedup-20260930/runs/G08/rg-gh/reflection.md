1. **Helped:** The second call, `gh issue view 18837 --json title,body,state,url` combined with `gh pr view 18838 --json body,files,mergeCommit`, gave the issue repro, the PR description, the changed files and the merge SHA in one step. The first call's `gh pr list --search "18837"` found PR 18838 right away. Fetching the `proxy.js` patch with `gh api repos/.../commits/<sha>` and a jq filter on `.filename` showed the actual code change.

2. **Did not help:**
   - The first `gh issue view --comments | head` printed nothing, so I had to re-run it with `--json`.
   - `gh pr diff` returned a 504.
   - My first jq filter used `.path`, which is null on the commits API, so it failed. The field is `.filename`.
   - The last call, a `gh api .../contents/...?ref=` with `rg`, failed because zsh tried to glob the `?`. I didn't retry it, so I never read the full file at that commit.

3. **Next time:** Quote URLs. Use `--json` from the start. Fetch the full `proxy.js` at the merge SHA so I can cite exact line numbers and see the old code. I'd also read the issue comments to check whether the maintainers confirmed the root cause.

4. **Confidence:** Medium-high. The fix mechanism comes straight from the diff and the PR text. The root-cause explanation is partly inferred, and the line number comes from the hunk header, not from the file.