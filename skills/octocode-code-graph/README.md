# Octocode code graph

Generate ranked hypotheses from repository dependency topology. Prove or dismiss each hypothesis with exact imports, AST structure, LSP identity, and runnable checks.

## Use when

- You need cycle, path, layering, reachability, or change-impact analysis.
- A dead-code candidate needs evidence beyond a search result.
- Dependency topology is the primary reasoning surface.

## Evidence rules

- A graph edge proves a syntactic file dependency, not symbol identity.
- Treat a cycle or high-degree node as a candidate until exact evidence confirms runtime edges and impact.
- Reachability alone never proves safe deletion.
- Truncation, entrypoint assumptions, test policy, and unavailable semantic tools remain visible.

## Workflow

```text
FRAME → TRIAGE → GRAPH → PROVE → RANK → REPORT
```

Use `octocode-research` when graph topology is only one part of a broader investigation.

## Install

```bash
npx octocode skill install octocode-code-graph --platform codex
```

## Maintainer verification

```bash
node scripts/eval-code-graph.mjs --self-test
node scripts/eval-code-graph.mjs --json
```

Then run the `octocode-skills` review against this folder.
