# Workflow: External Research

Use when the corpus is a remote repo, PR, package, prior-art question, or upstream dependency.
Read `algorithm.md` first; read `octocode.md` only when transport or CLI syntax is unclear.

```text
npmSearch / ghSearchRepos for discovery
-> ghViewRepoStructure for orientation
-> ghSearchCode for anchors
-> ghGetFileContent(matchString or symbols) for exact proof
-> ghHistoryResearch for PR/commit intent, issue context (type:"issues" + issueNumber), or releases
-> materialize when AST, LSP, negative proof, repeated reads, or local tests matter
```

External-proof rules:
- GitHub search zeros are provider evidence, not absence. Verify path/ref, try synonyms, inspect structure, then materialize before strong negative claims.
- Track `resolvedBranch`/ref and cite it. A fallback branch changes what was actually researched.
- Packages: use npm/package metadata to find the source repo, but use exact code/docs/tests before recommending reuse.
- Materialize after the third read into one remote area, or earlier when structural search, LSP, many-file search, or exact absence matters.

Cross-pollinate with `workflow-local.md` when a local clue (dependency name, error string, config key) points outward, or an external fact (upstream fix, PR intent) needs local confirmation.
For repo-ecosystem ranking or reuse decisions across multiple candidate repos, use `github-landscape.md` instead of a single-repo pass here.

Validate: `node scripts/eval-research.mjs --case external-research`.
