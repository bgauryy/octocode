# Octocode Architect

Analyze software architecture with exact code evidence, then make or verify the smallest safe change. Agent rules live in `SKILL.md`.

## Install

```bash
npx -y octocode skill install octocode-architect
```

## Maintainer verification

Run the checks named at the end of `SKILL.md`.

## Sources

Local sources behind these rules (repository-relative). Defect-prevalence citations live in `references/agent-defect-evidence.md`.

| Source | Used for |
|---|---|
| `packages/octocode-native/crates/runtime/src/tools/ast_graph/` | `astTopology` analyses, edge kinds, coverage and pagination signals, dead-code candidates, persisted-graph issue detectors |
| `packages/octocode-native/crates/cli/src/cli/graph.rs` | `octocode graph ingest` / `query` operations |
| `packages/octocode-native/crates/engine/src/graph/algorithms.rs` | SCC condensation, layers, transitive edges, dominators, and path primitives |
| `packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md` | AST, graph, and LSP capability boundaries |
| `skills/octocode-research/` | schema-first evidence collection and semantic proof lanes |
| `skills/octocode-skills/` | progressive disclosure, trigger tuning, cleanup, and review gates |
| Repository `AGENTS.md` and package architecture guides | local ownership, dependency direction, and verification contracts |

General architecture and algorithm practices stay hypotheses until the checked-out system supplies intent, mechanism, and impact.
