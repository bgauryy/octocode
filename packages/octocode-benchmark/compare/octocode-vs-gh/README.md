# Octocode MCP vs `gh` CLI — GitHub Research v2

This is the plain-`gh` permutation of the paired GitHub Research v2 benchmark.
It is an ablation of baseline output shaping, not a separate question suite.
Questions and oracle live ONLY in the canonical bank — this folder holds no
copies; it defines how to run the comparison.

## Identity

- `suiteVersion`: `2`
- `questionBankId`: `github-research-v2`
- Questions: canonical source —
  [`../../questions/github/research-v2/questions.md`](../../questions/github/research-v2/questions.md)
- Oracle (judge-only): canonical source —
  [`../../questions/github/research-v2/ground-truth.json`](../../questions/github/research-v2/ground-truth.json)

## Arms

- **Control C:** no research tools.
- **Baseline A — `gh`:** only matching `gh` CLI GitHub repository, code,
  content/tree, pull-request, issue, and commit operations. Output may be shaped
  only with `gh`'s own output options.
- **Treatment B — Octocode MCP remote GitHub only:** `ghSearchCode`,
  `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`,
  `ghSearchPullRequests`, `ghSearchIssues`, and `ghSearchCommits`.

Treatment B may not use the Octocode CLI, local tools, clone, AST, LSP, npm,
minification/symbol modes, or a cache/batching advantage. No raw GitHub API,
browser, package registry, or other research source is available to either
solver arm.

## Execution contract and question map

Owned by the canonical bank — follow
[`../../questions/github/research-v2/README.md`](../../questions/github/research-v2/README.md)
(frozen execution contract + Q1–Q14 category map), identical for both
paired permutations. Do not copy it here.

## References

- [Canonical bank](../../questions/github/research-v2/README.md)
- [Shared method](../../README.md)
- [Run instructions](../../INSTRUCTIONS.md)
- [Judging](../../JUDGING.md)
- [Scoring](../../SCORING.md)
- [Report template](../../REPORT_TEMPLATE.md)
- [v2 results ledger](../../results/octocode-vs-gh.md)
