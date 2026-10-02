# References

Audit trail for the local sources behind these rules. Paths are repository-relative.

| Source | Used for |
|---|---|
| `packages/octocode-native/crates/runtime/src/tools/ast_graph/` | `astTopology` analyses, edge kinds, coverage and pagination signals, dead-code candidates, persisted-graph issue detectors |
| `packages/octocode-native/crates/cli/src/cli/graph.rs` | `octocode graph ingest` / `query` operations |
| `packages/octocode-native/crates/engine/src/graph/algorithms.rs` | SCC condensation, layers, transitive edges, dominators, and path primitives |
| `packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md` | AST, graph, and LSP capability boundaries |
| `skills/octocode-research/` | schema-first evidence collection and semantic proof lanes |
| `skills/octocode-skills/` | progressive disclosure, trigger tuning, cleanup, and review gates |
| Repository `AGENTS.md` and package architecture guides | authoritative local ownership, dependency direction, and verification contracts |

General architecture and algorithm practices remain hypotheses until the checked-out system supplies intent, mechanism, and impact. Defect-prevalence citations live in `references/agent-defect-evidence.md`.
