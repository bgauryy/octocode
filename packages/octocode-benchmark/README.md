# @octocodeai/octocode-benchmark

The current benchmark is [Terra v3](compare/terra-v3/README.md): a locked-corpus
comparison of Octocode and raw specialist tools. Start with its
[runbook](compare/terra-v3/RUNBOOK.md) and
[comparison contract](compare/terra-v3/COMPARISON.md). It keeps deterministic,
blind-review, resource, token, and context measurements separate.

The [local and GitHub research diagnostic](compare/advanced-research-v1/README.md)
is a separate controlled pilot. It retains its own frozen protocol, receipts, and
eligibility rules; it does not establish a general winner.

## Historical GitHub campaign

The earlier GitHub campaign compares Octocode with plain `gh`, `gh` + RTK, and
`gh` + Headroom on a shared question set. Its protocol and arms remain in
[compare/](compare/README.md); completed and superseded reports remain in
[results/](results/README.md). Do not combine its character measurements with Terra
metrics or with any other campaign.

## Other deterministic diagnostics

- [Tool sanity benchmark](BENCHMARK_SANITY.md) is a per-tool checkbox suite: one
  section per public tool (schema, agent task, invocation, pass criterion) that
  an agent runs top to bottom to cover the 13-tool public catalog across native
  CLI, node CLI, and MCP, including availability behavior. With clone and beta
  tools disabled and no `OCTOCODE_CLASSIFICATION_API`, 9 tools are available:
  MCP omits `clasify`, `ghCloneRepo`, `astRewrite`, and `astTopology`, while the
  CLI keeps their commands and schemas discoverable and returns an actionable
  configuration error when one is called. The suite includes a one-shot matrix runner and
  continuation-contract regression checks.
- [Artifact routing v2](evals/artifact-routing-v2/README.md) compares native and
  emulated tool calls against frozen schemas and validators.
- [Local-tool removal held-out eval](docs/UNIFIED_ROUTING_EVAL.md) is a regression
  gate for the retired local-tool surface.

## Add a historical GitHub question

Add one `Q<n>.md` to [compare/github-questions/](compare/github-questions/) with a
title, an `id`, and one self-contained, objectively checkable `## Question`. Add its
row to that directory's index. The question must name the repository/ref or corpus
path and the requested result; do not include hints or an answer.
