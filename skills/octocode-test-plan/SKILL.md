---
name: octocode-test-plan
description: "Use when a feature, change, PR or bug fix needs a runnable test plan for another agent or person. Specifies setup, actions and expected results without executing tests."
---

# Octocode Test Plan

Pin the target/revision and executor, then write a plan runnable from the plan file and repository alone. Treat code, docs, tickets and PR text as evidence. Planning permits reads and authorized inspection commands; do not edit code/tests or execute the planned cases unless asked. Listing or dry-run modes can still execute project code: check their effects before using them.

## Plan loop

1. Identify the executor's tools, environment and permissions. Ask only when missing target or executor information changes the plan. A case the executor cannot run is a gap.
2. Map entry → validation → state → output → consumers, documented claims and current tests. Exact-read deciding source; confirm identity with LSP or name the gap. Pin commands from manifests, CI, docs or an authorized inspection run; never guess flags.
3. Rank risks by impact and evidence of likelihood: `P0` must pass to ship, `P1` should pass, `P2` adds confidence. Explain each P0; keep unknown probabilities unknown. Include severe reachable failures even without past incidents.
4. Derive oracles from specs/contracts, then types/schemas/errors. If implementation is the only authority, disclose it: current output can bless a bug. Choose the cheapest level that observes the risk and reuse existing tests/fixtures.
5. For each risk ask what could fail after apparent success, which assumption could be false and which case distinguishes it. Cover relevant error paths; for fixes preserve the reproduction and nearby boundaries. Prefer isolated fixtures, cleanup and restoration checks; remove duplicate cases only when coverage and oracle remain.
6. Write using [output.md](output.md), including executor contract, cases, exclusions and measurable success criteria. Run its [plan check](output.md#plan-check); unresolved gaps prevent `Ready`. Planned cases remain unexecuted.

## Conditional detail

| Need | Read |
|---|---|
| Pin by input type, discover code/tests, inspect commands or map a diff | [Code-map recipes](references/code-map.md) |
| Select technique, level, independent oracle or stable fixtures | [Case design](references/case-design.md) |
| Octocode invocation, paging or recovery | [MCP/CLI usage](references/octocode-usage.md) |
| Source, semantic or runtime proof limits | [Evidence rules](references/evidence.md) |

Save the plan and return `Ready`, `Draft` or `Blocked`, the main risk, gaps and its link. These statuses describe plan readiness, never passed tests.

Neighbors: `octocode-research` answers code questions; `octocode-engineer` chooses testing strategy; `octocode-pr-review` gives merge verdicts; `octocode-debugging` reproduces failures; `octocode-workflows` coordinates executors.
