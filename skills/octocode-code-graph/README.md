# Octocode code graph

Turns dependency-graph signals into evidence-backed findings for architecture risk, change impact, cycles, layers, reachability, and dead-code review.

## What it does

- Chooses a bounded `localAnalyzeGraph` operation and turns its output into falsifiable hypotheses.
- Proves file-topology signals with exact imports, AST shape, LSP identity, and runnable checks.
- Separates confirmed, likely, candidate, dismissed, and incomplete results.
- Keeps entrypoint assumptions, test policy, scan limits, truncation, and unavailable capabilities visible.

## What it does not do

It does not treat every cycle or high-degree file as a defect, infer safe deletion from reachability, or replace `octocode-research` for general investigation. Use it when the graph is the primary reasoning surface.

## Workflow

```text
FRAME -> SCHEMA -> GRAPH -> HYPOTHESIZE -> PROVE -> RANK -> VERIFY
```

The skill uses Octocode MCP tools when available, this monorepo's built CLI during development, and `npx octocode` elsewhere. It reads the live schema before raw calls.

## Evidence rules

- `cycleEdges` is a directed witness; SCC member order is not a cycle path.
- `runtimeCycleEdges` removes type-only edges. Imported bindings and initialization order still need proof.
- `transitiveCandidates` can reveal duplicate wiring, but side effects, re-exports, and symbol use can justify the edge.
- `path` minimizes unweighted import edges; it does not model performance or business cost.
- `deadCode` generates candidates; entrypoints, tests, framework loading, exports, LSP references, and a runnable check decide the verdict.

Results include confidence, evidence, impact, the smallest safe action, and verification. They also name incomplete scans or semantic gaps.

## Installation

```bash
npx octocode skill install octocode-code-graph
```

## Maintainer verification

```bash
node skills/octocode-code-graph/scripts/eval-code-graph.mjs --self-test
node skills/octocode-code-graph/scripts/eval-code-graph.mjs --json
node skills/octocode-skills/scripts/skill-review.mjs skills/octocode-code-graph --json
```

The evaluator covers six failure-derived controls from `evals/cases.json`; KPI and guardrails live in `evals/kpi-contract.json`. Before publishing, also smoke-test a fresh repository question through graph output, exact edge reads, AST/LSP proof, and a zero-exit check.
