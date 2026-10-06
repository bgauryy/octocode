# Architecture Analysis

Load when dependency topology, cycles, reachability, dead code, duplication, coupling, separation, interfaces, or efficiency can affect an architecture decision. Why: topology is useful triage, but it cannot prove symbol identity, runtime behavior, or impact.

## Choose the narrowest evidence lane

| Question | Start with | Candidate—not verdict |
|---|---|---|
| What can this file affect or rely on? | `astTopology` `dependents` / `dependencies`; `graph query impact <ref>` or `impact --since <rev>` for a diff | blast radius or coupling |
| Can one layer reach another? | `astTopology` `path` or `graph query path <from> <to>`, plus the declared rule | boundary crossing |
| Which files mutually depend? | `astTopology` `cycles` (`runtimeCycle` separates runtime from type-only closure) or `graph query cycles` | initialization or change coupling |
| What is outside known roots or removable? | `astTopology` `reachability` / `deadCode` with explicit `entrypoints` and `includeTests` policy | alternate entrypoint or stale code |
| Did a change add edges or cycles? | `astTopology` `drift` against a baseline root, or `graph query issues --baseline <snapshot>` | architectural drift |
| Where should review look first? | `graph query issues`: ranked hypotheses with `controls` and `verify` commands | hotspot, never a finding |
| Is policy duplicated or an interface leaky? | exact scenarios, consumers, AST/LSP, history | shared change pressure or contract leakage |
| Where is a runtime budget spent? | representative profile/trace plus end-to-end benchmark | measured hot path under the named workload |
| Is build or developer flow inefficient? | timed clean/warm run plus I/O/process/cache counts | measured build/developer path |

CLI-only beta `astTopology` answers one bounded question per query; `octocode graph ingest <path>` builds a persisted snapshot that `octocode graph query <op>` answers repeatedly — re-ingest when `graph query stale` lists changed files. Fix the absolute scan root, entrypoints, exclusions, test policy, and page size so reruns are comparable. Run `deadCode`'s `hints.verifyReferences` leads into `lspSearch` references. Use topology only for file relationships, never as a substitute for data/control flow or symbol lookup. Coverage signals: `astTopology` `completeness` and `coverage.imports.unresolvedInternal` (coverage diagnostics are counts until paged through `next.nextDiagnosticPage`), `pagination.outOfRange` on an empty page, `lspSearch` `coverage.exhaustive`, and `graph` `callInternalRecall` and finding `tier`. Language-server reference coverage can depend on server warmth; rerun or treat a partial result as a lower bound.

## Kill the strongest alternate

| Signal | Common false explanation | Required control |
|---|---|---|
| Cycle | the closing edge is type-only or the module is intentionally cohesive | exact-read every edge; inspect `edgeKinds` and runtime impact; implicit edges such as `java-same-package` have no import line, so prove the use with `lspSearch` references |
| Unreachable/dead | subpath export, CLI/plugin/framework registration, dynamic import, tests or consumers outside the root | inspect manifests/configs and broader consumers; verify symbols separately |
| Barrel-only export | external or deep-import public contract | inspect package surface and external consumers; re-export consumers surface as `dependents` rows with `reexportVia` and as `lspSearch` references grouped by file |
| Cross-folder edge | folder names imply layers but no rule exists | find an authoritative rule or label the boundary inferred |
| Similar code | distinct rules, trust boundaries, or volatility | compare semantics, edge cases, ownership, and change pressure |
| Wide node or long path | composition root, generated registry, stable shared value, or cold path | inspect responsibilities and measure the claimed cost |
| Small interface | unsafe call sequence or leaked lifecycle remains | inspect consumer workflows, invariants, errors, async/cancellation, and versioning |

Record `alternate → check → killed|unresolved`. An unresolved applicable alternate caps confidence at candidate.

## Proof and ranking

`graph candidate → exact decisive edges/code → AST shape → LSP definitions/references/callers → representative data/control/ownership trace → config/test/runtime/measurement → verdict`

- Affected scope: graph dependents/path plus semantic references for the changed symbol.
- Layer violation: exact path, importing symbol, authoritative rule, and quality impact.
- Dead code/delete: explicit roots, export/re-export chain, semantic references excluding declarations, broad registration/config search, then tests/build.
- Coupling or god module: graph breadth plus mixed responsibilities and real consumer/change impact; size alone is insufficient.
- Duplication: semantic equivalence, shared invariant, and shared change pressure; text similarity is insufficient.
- Separation/interface: inward framework/storage/view dependency, invariant owner, real consumer contract, and policy test seam.
- Hot path: define workload, traffic mix, input scale, environment, and latency/resource budget; measure end-to-end before attributing cost to a symbol.
- Hot-path attribution: correlate profiles and traces with request fan-out, I/O, serialization/copying, allocation/GC, cache cold/warm behavior, locks/queues, contention/backpressure, retries, and network/storage waits. Check inclusive and exclusive cost plus p50/tail behavior when available.
- Efficiency: keep only an improvement that also satisfies correctness, latency, throughput, memory, and resource guardrails. Centrality, call count, or one flame-graph sample alone is not proof.

Rank by `evidence strength × consequence × actionability`. Preserve partial scans, inferred roots, warnings, unsupported semantic tools, and unmeasured claims. Continue to `references/change-discipline.md` only for a proven or explicitly accepted design problem.
