# Workflow: External Research

Use when the corpus is a remote repo, PR, package, prior-art question, or upstream dependency.
Read `references/algorithm.md` first; read `references/octocode.md` only when transport or CLI syntax is unclear.

```text
npmSearch / ghSearchRepos for discovery
-> ghViewRepoStructure for orientation
-> ghSearchCode for anchors
-> ghGetFileContent(matchString or symbols) for exact proof
-> ghSearchPullRequests / ghSearchCommits for PR/commit intent, ghSearchIssues (issueNumber) for issue context
-> ghListReleases for releases — opt-in, needs ENABLE_RELEASES=1 (see references/octocode.md); otherwise use tags via ghSearchCommits and mark releases skipped
-> materialize when AST, LSP, negative proof, repeated reads, or local tests matter
```

External-proof rules:
- GitHub search zeros are provider evidence, not absence. Verify path/ref, try synonyms, inspect structure, then materialize before strong negative claims.
- Track `resolvedBranch`/ref and cite it. A fallback branch changes what was actually researched.
- Packages: use npm/package metadata to find the source repo, but use exact code/docs/tests before recommending reuse.
- Materialize after the third read into one remote area, or earlier when structural search, LSP, many-file search, or exact absence matters.

Cross-pollinate with `references/workflow-local.md` when a local clue (dependency name, error string, config key) points outward, or an external fact (upstream fix, PR intent) needs local confirmation.

Next: when both directions are needed at once bridge through `references/workflow-combination.md`; for ranking or reuse decisions across multiple candidate repos load `references/github-landscape.md` instead of a single-repo pass here; for proof depth on any remote code claim load `references/code-research.md`.

