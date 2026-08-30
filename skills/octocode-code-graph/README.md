# Octocode code graph

Turns dependency-graph signals into evidence-backed code findings. It supports architecture-risk triage and change-impact mapping. Use it to analyze cycles, layers, and reachability. It also verifies dead-code candidates carefully.

## What it does

- Chooses the cheapest bounded `localAnalyzeGraph` operation for the question.
- Converts cycles, paths, dependents, dependencies, reachability, and dead-code output into falsifiable hypotheses.
- Interprets directed cycle witnesses, type-only versus runtime cycles, SCC condensation layers, immediate dominators, and transitive-edge candidates.
- Upgrades syntactic file topology with exact imports, AST structural search, LSP symbol identity, and runnable verification.
- Ranks confirmed findings separately from likely, candidate, dismissed, and incomplete results.
- Preserves entrypoint assumptions, test policy, scan limits, truncation, unsupported resolution, and unavailable LSP capabilities.
- Rejects common false positives from type-only cycle edges, narrow scan roots, alternate package entrypoints, barrel contracts, and misleading partial-test summaries.

## What it does not do

It does not treat every cycle or high-degree file as a defect, infer safe deletion from graph reachability, or replace `octocode-research` for general code investigation. Use it when the graph itself is the primary reasoning surface.

## Workflow

```text
FRAME -> SCHEMA -> GRAPH -> HYPOTHESIZE -> PROVE -> RANK -> VERIFY
```

Use Octocode MCP tools when they are available. In this monorepo the skill uses the built CLI; installed copies fall back to `npx octocode`. The workflow remains schema-first so it does not depend on memorized fields.

## How it reads graph findings

- `cycleEdges` is the deterministic directed witness to inspect. The order of files in an SCC is not a cycle path.
- `runtimeCycleEdges` removes type-only edges and isolates cycles that can affect module loading. The skill still checks imported bindings and initialization order before confirming a defect.
- A type-only SCC is coupling evidence, not a runtime loading cycle.
- `transitiveCandidates` identifies condensation-graph edges with an alternate directed path. The skill treats these as possible duplicate wiring, then checks side effects, re-export contracts, and symbol use before recommending removal.
- `path` finds the fewest import edges in an unweighted graph. It does not pretend that import edges have performance or business-cost weights.
- `deadCode` is a candidate generator. Entrypoints, tests, framework loading, package exports, LSP references, and runnable verification remain part of the proof.

The result is a ranked finding with an explicit confidence level, evidence chain, impact, smallest safe action, and verification command. Incomplete scans and unavailable semantic checks stay visible instead of being silently promoted to conclusions.

## Installation

```bash
npx octocode skill --name octocode-code-graph
```

## Maintainer verification

```bash
node skills/octocode-code-graph/scripts/eval-code-graph.mjs --self-test
node skills/octocode-code-graph/scripts/eval-code-graph.mjs --json
node skills/octocode-skills/scripts/skill-review.mjs skills/octocode-code-graph --json
```

The evaluator freezes six failure-derived controls in `evals/cases.json`; its KPI and guardrails live in `evals/kpi-contract.json`. Then run a fresh repository question through graph output, exact edge reads, AST/LSP proof, and a zero-exit deterministic check before publishing.
