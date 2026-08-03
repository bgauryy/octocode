# Octocode MCP vs (`gh` CLI + `rtk`) — GitHub Research v2

This is the `rtk` output-shaping permutation of the paired GitHub Research v2
benchmark. It is an ablation of baseline output shaping, not a separate question
suite. The canonical bank must not drift between this suite, the plain-`gh`
permutation, and the bank source.

## Identity

- `suiteVersion`: `2`
- `questionBankId`: `github-research-v2`
- Questions: canonical source —
  [`../../questions/github/research-v2/questions.md`](../../questions/github/research-v2/questions.md)
- Oracle (judge-only): canonical source —
  [`../../questions/github/research-v2/ground-truth.json`](../../questions/github/research-v2/ground-truth.json)

## Arms

- **Control C:** no research tools.
- **Baseline A — `gh` + `rtk`:** the same `gh` CLI GitHub repository, code,
  content/tree, pull-request, issue, and commit operations as the plain-`gh`
  permutation. `rtk` may only filter or shape `gh` output; it provides no extra
  research source.
- **Treatment B — Octocode MCP remote GitHub only:** `ghSearchCode`,
  `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`,
  `ghSearchPullRequests`, `ghSearchIssues`, and `ghSearchCommits`.

### What `rtk` is

`rtk` is a third-party CLI that filters/reshapes `gh` stdout to cut tokens; it
adds **no new research source**. It is not part of this repo. Pin the exact
binary + version in `manifest.md` (`baselines.rtk`) and confirm it before the
run — last observed `rtk 0.41.0` alongside `gh 2.76.2`.

### Fairness rules (HARD — this ablation measures the tool, not solver discipline)

The whole point of this permutation is to compare Octocode against a
*token-optimized* baseline. If the baseline solver forgets to shape a large
payload, its byte cost balloons and Octocode's efficiency edge is inflated by
solver behavior rather than tool design (the `2026-08-03-cross-repo-draft`
run saw exactly this on Q1/Q3, where raw 395KB/641KB `gh` payloads reached the
solver unfiltered). To remove that confound:

1. **Mandatory shaping over 50 KB.** Any Arm A tool call whose `rawBytes` > 50 KB
   MUST be piped through `rtk` before the payload enters the solver context. A
   trial that reads a raw `gh` payload > 50 KB unfiltered is an **invalid trial**
   (`taskStatus: invalid`), excluded from aggregates, and re-run.
2. **Hard call cap.** Each question's `task.budget.maxToolCalls` is a hard cap,
   not advisory: any trial (A **or** B) that exceeds it is `taskStatus: invalid`
   and re-run. Retries, failures, empty calls, and pagination all count.
3. **Symmetry.** Both arms are held to the same hard cap and the same fresh,
   uncached context. Record every invalidated + re-run trial in the report's
   guardrails section.

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
- [v2 results ledger](../../results/octocode-vs-gh-rtk.md)
