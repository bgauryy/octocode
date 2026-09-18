# Terra v3 comparison contract

Terra v3 compares research outcomes, not command popularity. Every arm receives the same
case, locked repository bytes, cache cohort, budget, and fresh `gpt-5.6-terra` runner
context. Correctness is the primary metric; speed, memory, tokens, context, and call
efficiency are guardrails and tie-breakers.

This page defines what the comparison means. See [RUNBOOK.md](RUNBOOK.md) for the operating
procedure and [contracts/](contracts/) for the executable arm and role contracts.

## Current TL;DR

Octocode has the strongest integrated agent-research workflow in this comparison: one typed
surface connects lexical search, exact reads, structural matching, repository topology, and
LSP evidence. That architectural breadth does not prove it is faster or uses fewer
tokens.

- Raw ripgrep remains a direct, mature choice for pure lexical searches.
- ast-grep remains the more mature dedicated structural rewrite surface.
- Direct LSP remains the semantic authority; Octocode's advantage is agent-facing discovery,
  normalization, pagination, and diagnostics around it.
- Sourcegraph remains stronger for already-indexed, very large multi-repository estates and
  managed cross-repository changes.
- This architecture review rates Octocode strongest when an agent must cross several evidence modes in one
  investigation without dumping large files or losing continuation state.

Assessment rating: architecture **8/10**, implementation readiness **5/10**, relative
performance **unscored**. Only a validator-passing Terra campaign can replace the unscored
performance verdict.

## Surfaces under comparison

| Surface | Native interface | Eligible work | Important limitation |
|---|---|---|---|
| Octocode | Workspace-built `octocode tools` CLI | Lexical, structural, semantic, indexed, codemod preview, end-to-end | The workspace receipt must bind the CLI, native addon, sources, dependency lock, catalog, and schemas. |
| ripgrep | Raw `rg --json` | Lexical and end-to-end evidence gathering | It does not provide structural identity, semantic resolution, or persistent indexing. |
| ast-grep | Raw `ast-grep`/`sg` JSON output | Structural search, codemod preview, end-to-end evidence gathering | Patterns must use a capability shared with the Octocode structural lane. |
| Native LSP | Direct stdio JSON-RPC through `terra_v3_lsp_client.py` | Python and TypeScript semantic cases | This measures the language server, not editor UI latency or editor-assisted human interaction. |
| Sourcegraph | Self-hosted `src search -json -stream` | Indexed and end-to-end cases | Both repositories must be indexed at the exact corpus commits before timing begins. |
| Native editor workflow | Editor plus its configured LSP and search UI | Human/operator workflow cases | Not yet an executable Terra v3 arm. Do not present direct-LSP results as editor-workflow results. |

The comparison must not substitute wrappers or emulations for a native surface. A missing
binary or unavailable Sourcegraph instance is an environmental blocker, not a loss for that
arm.

## Metric families

| Family | Measures | Interpretation |
|---|---|---|
| Outcome quality | Deterministic anchor pass rate; blind correctness 0–10; depth and workflow 1–5; confirmation-judge agreement | Primary decision signal. A wrong answer cannot win through lower cost. |
| Performance | Monotonic wall time; user/system CPU; peak process-tree RSS; isolated cgroup-v2 peak; complete process-tree I/O; page faults | Report cold, warm-process, and warm-index cohorts separately. Linux cgroup v2 is normative. |
| Token usage | Provider-reported input, cached-input, output, reasoning, and total tokens for every Terra role | Never infer tokens from characters. `total = input + output`; cached input and reasoning are reported subsets. |
| Context usage | Tool-output characters delivered to the runner plus runner command/final-output characters | Tokenizer-independent diagnostic, reported separately from tokens. |
| Tool-call efficiency | Total calls, schema-invalid attempts, repaired calls, expected empty calls, and unproductive empty calls | Repairs and empty searches remain visible in the trajectory. |
| Index lifecycle | Build time, refresh time, index bytes, peak memory, and query latency after exact commit convergence | Report separately from warm query latency. |

The aggregator in `compare/bin/terra_v3_report.py` keeps these families separate and emits
paired Octocode-versus-baseline ratios only for the same case, pass, and cache cohort.

## Fairness rules

- Freeze the public suite, private envelope manifest, arm contracts, role contract, tool
  catalog, relevant schemas, corpus lock, and workspace receipt before the first measured
  call.
- Give every surface its leanest legitimate native path. Do not force a full-file or
  full-tree read when the surface has a targeted operation.
- Enforce each case's eligible lane matrix. Do not run an ineligible arm and call the
  resulting failure comparative evidence.
- Use three independent passes. Every runner, judge, and confirmation judge uses a fresh
  context and exactly `gpt-5.6-terra` at the frozen reasoning effort.
- Blind the first judge with X/Y labels. Reveal the label mapping only to post-run
  validation and aggregation. Use reversed order for confirmation.
- Keep public results as orientation. Release acceptance requires a separately curated,
  sealed private suite that was not visible while implementing the candidate.
- Never edit cases, graders, contracts, or receipts during a campaign. Corrections create a
  new suite version and invalidate partial results from the earlier version.

## Valid claims

A campaign may claim a quality, performance, or token difference only when its campaign
validator passes and all required sensors are available. Report paired distributions and
confidence intervals, not only totals. If correctness differs materially, rank quality
first and describe efficiency results without declaring the cheaper incorrect arm the
winner.

Until those conditions hold, the only valid output is a harness/preflight assessment. The
checked-in Terra v3 assessment intentionally contains no Octocode-versus-baseline winner.

## External capability references

The benchmark uses native surfaces rather than marketing-level substitutes:

- [ripgrep](https://github.com/BurntSushi/ripgrep) is a line-oriented recursive regex
  search tool with ignore-aware filtering and JSON output.
- [ast-grep rewrite documentation](https://ast-grep.github.io/guide/rewrite-code) defines
  its native structural rewrite and preview/apply surface.
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) defines
  the JSON-RPC boundary between editors and language servers; the direct-LSP arm measures
  this boundary, not the editor interface around it.
- [Sourcegraph Code Navigation](https://sourcegraph.com/docs/code-navigation) documents
  search-based and precise cross-repository navigation, while
  [Batch Changes](https://sourcegraph.com/docs/batch-changes) is its separate large-scale
  change-management surface.
