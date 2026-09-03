# Workflow: external research

Use when the corpus is a remote repository, PR, package, prior-art question, or upstream dependency.
Read `references/algorithm.md` first; read `references/octocode.md` only when transport or CLI syntax is unclear.

```text
npmSearch / ghSearch(operation:"repositories") for discovery
-> ghSearch(operation:"tree") for orientation
-> ghSearch(operation:"code") for anchors
-> ghGetFileContent(matchString or line range with minify:"none") for exact proof; symbols only for orientation
-> ghSearchHistory(operation:"pullRequests"|"issues"|"commits") for history candidates
-> ghGetHistoryItem(operation:"pullRequest"|"issue"|"commit"|"compare") for exact history detail
-> materialize when AST, LSP, negative proof, repeated reads, or local tests matter
```

External-proof rules:
- GitHub search zeros are provider evidence, not absence. Verify path/ref, try synonyms, inspect structure, then materialize before strong negative claims.
- Track `resolvedBranch`/ref and cite it. A fallback branch changes what was researched. <!-- style-lint: ignore-line passive-voice -->
- Packages: use npm/package metadata to find the source repository, but use exact code/docs/tests before recommending reuse.
- Materialize after the third read into one remote area, or earlier when structural search, LSP, many-file search, or exact absence matters.
- Execute every returned character or match continuation before claiming the remote file or match set is exhausted. <!-- style-lint: ignore-line passive-voice -->

Cross-pollinate with `references/workflow-local.md` when a local clue (dependency name, error string, config key) points outward, or an external fact (upstream fix, PR intent) needs local confirmation.

Next: when both directions are needed at once bridge through `references/workflow-combination.md`. For ranking or reuse decisions across multiple candidate repos load `references/github-landscape.md` instead of a single-repository pass here. For proof depth on any remote code claim load `references/code-research.md`. <!-- style-lint: ignore-line passive-voice -->
