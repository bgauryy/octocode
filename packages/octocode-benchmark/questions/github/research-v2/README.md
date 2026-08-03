# GitHub Research Question Bank v2

Canonical 14-question bank shared by `octocode-vs-gh` and
`octocode-vs-gh-rtk`. The bank measures remote GitHub research, not shell
filtering, local code intelligence, package-registry lookup, or output
minification.

## Version and tool equivalence

- Bank version: `github-research-v2`
- Canonical `questionBankId`: `github-research-v2`
- Suite version: `2`
- Questions: exactly `Q1`–`Q14`
- Treatment allowlist: `ghSearchCode`, `ghGetFileContent`,
  `ghViewRepoStructure`, `ghSearchRepos`, `ghSearchPullRequests`,
  `ghSearchIssues`, and `ghSearchCommits`
- Baseline equivalence: matching `gh` CLI repository, code, content/tree,
  pull-request, issue, and commit operations
- `rtk` role: filter or shape baseline output only; it adds no research source
- Forbidden differentiators: local clone/search, AST, LSP, npm lookup,
  symbols/minification, and Octocode-only cache or batching claims

Capability-equivalent baseline mapping:

- `ghSearchCode` → `gh search code`
- `ghGetFileContent` → `gh` repository-content/file operations
- `ghViewRepoStructure` → `gh` repository tree/content operations
- `ghSearchRepos` → `gh search repos` / repository metadata
- `ghSearchPullRequests` → `gh search prs`, `gh pr view`, and `gh pr diff`
- `ghSearchIssues` → `gh search issues` / issue view
- `ghSearchCommits` → `gh search commits` / commit view

Both comparison suites must provide the same solver-facing
[`questions.md`](questions.md) and judge against the same
[`ground-truth.json`](ground-truth.json). Suite overlays may set arm invocation
rules and measurement details, but may not change a question, oracle, rubric,
budget, or accepted variant.

## Admission rubric

Every admitted task has:

1. one named primary capability and a distinct `duplicateFamily`;
2. finite repositories and a fixed PR, issue, commit, or source ref where the
   existing verification supports one;
3. atomic required claims and an explicit partial-credit rubric;
4. an independently verified oracle with accepted variants;
5. a deterministic access budget expressed as maximum research calls and
   pagination depth;
6. contamination status plus the control cutoff used at run time;
7. difficulty and a stable `taskId`;
8. an equivalent workflow available to both remote-GitHub arms.

Reject a task if it depends on a null oracle, a live “latest” result, broad
essay judgment, famous-memory recall without a control, an unsupported
cross-repository premise, or an Octocode-only capability. Search snippets are
discovery evidence, not proof. Negative claims require bounded structure and
content checks, not one empty search.

## Stability and re-verification

Fixed PR and issue numbers preserve the historical object, but branch contents,
states, repository metadata, language byte totals, line numbers, and default
branch heads can drift. Before every scored run:

1. independently re-verify every entry marked `reverifyBeforeRun`;
2. record the resolved commit for each mutable branch;
3. freeze the questions, oracle, accepted variants, budgets, and control cutoff;
4. run the no-tools control first;
5. keep contaminated rows visible and exclude rows meeting the registered
   cutoff from the primary mean.

The source audit and per-task provenance are recorded in
[`TASK_ACCEPTANCE.md`](TASK_ACCEPTANCE.md). Shared comparison methodology remains
owned by [`README.md`](../../../README.md), with execution in
[`INSTRUCTIONS.md`](../../../INSTRUCTIONS.md).

## Strengthened-schema integration note

The bank uses the strengthened task field names (`category`, `budget` with
`maxToolCalls`/`maxPages`/`timeoutMs`, normalized `contamination`, and
`targetTools`). The ground-truth schema accepts structured claim-level oracles
and either an independently verified commit SHA or an honest ref that the
pre-run verifier must resolve and freeze. No immutable value is invented.

## Frozen v2 execution contract

Shared by BOTH paired permutations (`octocode-vs-gh`, `octocode-vs-gh-rtk`) —
owned here; suites reference it, never copy it.

- **Prompt:** the exact bank `questions.md`, including its evidence and
  `Unknown` instruction; no arm-specific hints, examples, or extra context.
- **Model:** select and record one exact model version and settings in the run
  manifest; use them unchanged for every C/A/B trial in both permutations.
- **Budget:** use each question's canonical `task.budget`
  (`maxToolCalls`, `maxPages`, and `timeoutMs`) unchanged for every solver arm.
  Bookkeeping calls do not research, but every research attempt counts.
- **Retries:** pre-register one identical retry policy for both permutations
  and all solver arms; retries, failures, empty calls, and pagination count
  toward the canonical budget. No arm-specific retry allowance is permitted.
- **Cache:** start every trial in a fresh isolated context with no primed or
  shared research-result cache. Do not score Octocode cache or batching as an
  advantage; record any unavoidable asymmetric cache state and invalidate the
  comparison.
- **Refs:** independently resolve every mutable ref marked for re-verification,
  record the resolved commit and UTC time, then freeze and reuse the same refs
  for C/A/B in both permutations.
- **Judge:** use a fresh blind judge and the three-stage protocol in
  [`../../JUDGING.md`](../../../JUDGING.md); verify decisive anchors on a surface
  outside both solver arms and score with [`../../SCORING.md`](../../../SCORING.md).
- **Control:** run C first. Apply each canonical contamination cutoff
  (`controlCorrectness >= 1.0`), keep contaminated rows visible, and exclude
  them from the primary mean.

## Q1–Q14 category map

| Q | Category | taskId |
|---|---|---|
| Q1 | Code search | `ghrv2-route-regex-builder` |
| Q2 | Repository discovery + negative evidence | `ghrv2-is-repo-absence` |
| Q3 | Commit history | `ghrv2-flask-route-history` |
| Q4 | Pull-request state metadata | `ghrv2-zustand-fix-pr-state` |
| Q5 | Pull-request code review | `ghrv2-vue-pr-diff-review` |
| Q6 | Cross-repository trace | `ghrv2-express-router-trace` |
| Q7 | Cross-repository comparison | `ghrv2-zustand-next-contract` |
| Q8 | Large-repository code search | `ghrv2-vscode-keybinding-dispatch` |
| Q9 | Documentation/source cross-check | `ghrv2-fastify-lifecycle` |
| Q10 | Repository discovery + entry trace | `ghrv2-axios-entry-chain` |
| Q11 | Repository discovery + runtime trace | `ghrv2-esbuild-process-boundary` |
| Q12 | Multi-file bounded fetch | `ghrv2-node-stream-event-wiring` |
| Q13 | Issue-to-PR diff trace | `ghrv2-redis-bitfield-security` |
| Q14 | Live pull-request review | `ghrv2-deepagents-oolong-pr-review` |
