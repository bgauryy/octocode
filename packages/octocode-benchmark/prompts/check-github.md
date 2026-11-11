## Check brief: Octocode GitHub and package tools vs gh lean

You check Octocode's GitHub, package and security tools against the best `gh` an expert writes:
`--json` with named fields, `--jq`, raw file media, narrow trees. The lean `gh` style is in
`prompts/gh-lean.md`; the method and per-tool checklist are in
`skills-dev/octocode-dev/references/tool-checks.md`. Read both first.

**Rules.** No scripts: make each call by hand, read the answer, judge it. A one-line `| wc -c` or `time`
is fine; keep nothing. Live targets: `vitejs/vite` and at least one other public repository; never
`facebook/react` (renamed, and some repositories are refused by the API). Use the pinned binary you were
given and name its hash. Nothing secret-shaped in any query.

**For each task, run both sides and record one row:**

| Task | Octocode call | Octocode chars, first / repeat ms | gh lean command | gh chars, ms | Right on both? | Better and why |

**Tasks:**
1. Repository metadata; repositories by topic, sorted by stars and by forks.
2. Code search for a symbol; then read its body (two calls each side).
3. A folder listing; the three largest files under a folder, all depths.
4. A small file; lines A–B of a large file; one function of a large file; its outline.
5. A word in a minified file (e.g. `jquery/jquery` `dist/jquery.min.js` at a tag).
6. Commits of one path, two pages; the newest commit's patch for that path.
7. Merged PRs about a term; one PR's files and reviews; PRs sorted by comments.
8. An issue thread.
9. Tags only, newest ten; resolve a tag to its commit.
10. What changed in one file between two releases.
11. A package version → its repository, directory and commit → a file at that commit (npm, PyPI, crates).
12. Is a package version affected by known vulnerabilities, and what to upgrade to; one advisory in detail.
13. Many searches in one repository (clone, then local search ×3).

gh lean equivalents to use: `gh repo view --json`, `gh search repos|code|prs|issues … --json … --jq …`,
`gh api 'repos/O/R/contents/PATH?ref=X' -H 'Accept: application/vnd.github.raw' | sed -n`,
`gh api repos/O/R/git/trees/REF?recursive=1 --jq …`, `gh api repos/O/R/commits?path=…&per_page=3&page=N --jq …`,
`gh pr view N --json files,reviews --jq …`, `gh api repos/O/R/compare/A...B --jq '.files[] | select(.filename=="P") | .patch'`,
`gh api repos/O/R/tags --jq`, `npm view pkg@v repository gitHead --json`, `curl … | jq`,
`curl -d '{…}' https://api.osv.dev/v1/query | jq`, `gh api advisories/GHSA-… --jq …`, `gh repo clone -- --depth 1` + `rg`.

**Judge each row on:** right answer; deciding row on page 1; next call possible from the output (commit SHA,
number, path); paging (Octocode cursor vs rebuilding `page=N`); characters (≈ 4 per token); first and repeat
time; what the gh side had to know (REST path, field names, jq). Octocode loses a row when gh lean is right and
smaller with no guessing; say so plainly.

**Report:** the table, a one-line verdict per task (Octocode / gh lean / even), every Octocode defect with the
exact call that shows it, and what you did not check. Delete anything you wrote before you report.
