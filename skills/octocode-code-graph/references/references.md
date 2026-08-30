# References

Audit trail for the local evidence used to create this skill. Paths are repository-relative unless noted.

## Local sources

| Source | Used for |
|---|---|
| `packages/octocode-tools-core/src/tools/local_analyze_graph/analyzeGraph.ts` | operation behavior, pagination, warnings, syntactic confidence, and dead-code LSP continuation |
| `packages/octocode-tools-core/src/tools/local_analyze_graph/scheme.ts` | graph output and dead-code candidate contracts |
| `packages/octocode-tools-core/src/tools/local_analyze_graph/retention.ts` | re-export, lexical fallback, and liveness limitations |
| `packages/octocode-tools-core/src/graph/advancedOperations.ts` | SCC condensation, topological layers, transitive edges, dominators, and weighted-path primitive |
| `packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md` | AST, graph-fact, and LSP language/capability boundaries |
| `packages/octocode-engine/docs/NATIVE_GRAPH_DOMAIN_SCOPE.md` | graph discovery versus LSP deletion-proof boundary and import-resolution risks |
| `skills/octocode-research/references/algorithm.md` | schema-first routing and multi-lane proof model |
| `skills/octocode-research/references/workflow-local.md` | graph operation selection and semantic upgrades |
| `skills/octocode-research/references/code-research.md` | candidate-to-verdict proof ladder |
| `skills/octocode-skills/references/skill-anatomy.md` | lobby/reference structure and progressive disclosure |
| `skills/octocode-skills/references/skill-review-rules.md` | mechanical skill quality gates |
| Dogfood graph run over `octocode-tools-core/src` | type-only SCC inflation, root-scope dead-code false positives, alternate entrypoints, symbol-vs-file liveness, barrel contracts, and exit-status verification controls |

No external web, registry, or marketplace source was needed; the requested behavior is defined by the checked-out implementation and its bundled skills.
