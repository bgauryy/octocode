# Issue Catalog

Load after a graph signal appears. Why: translate topology into falsifiable code hypotheses without labeling every unusual shape a bug.

| Signal | Predict, do not assert | Strong alternate to kill | Required upgrade |
|---|---|---|---|
| SCC/cycle | initialization-order risk, boundary coupling, hard-to-isolate change | type-only import or intentional cohesive module | exact every cycle edge; AST import kind; runtime/test impact |
| Wide dependents | change hotspot or unstable shared contract | small stable value/type with broad legitimate reuse | LSP references by changed export; consumer categories; regression checks |
| Wide dependencies | orchestration module, mixed responsibility, fragile boundary | legitimate composition root/barrel | AST shapes/responsibilities; LSP callees; exact imports |
| Cross-layer path | layering leak or policy bypass | approved adapter/port direction | exact each path edge; identify importing symbol; stated layer rule |
| Immediate dominator | mandatory change chokepoint or fragile shared boundary | incomplete roots/resolution or dynamic bypass | exact alternate paths; LSP consumers; configuration search |
| Transitive condensation edge | redundant dependency or bypassed layer | distinct binding/side-effect contract on the direct edge | exact imports and side effects; semantic references |
| Topological layer | migration/build ordering opportunity | SCC condensation hides meaningful intra-component order | inspect SCC membership and runtime edges |
| Unreachable file/cluster | stale feature, alternate entrypoint, or generated/plugin path | dynamic/framework/config registration | explicit roots; broad text/config search; LSP; build/tests |
| Unreferenced export | dead public surface | re-export, reflection, CLI/config string, external consumer | follow graph `next.verifyReferences`; AST/export chain; package contract |

## Ranking

Rank by `evidence strength × consequence × actionability`, never topology size alone.

- Confirmed: exact edges plus semantic/runtime evidence show an observable contract or maintenance impact.
- Likely: exact edges and two proof lanes support the mechanism; one bounded gap remains.
- Candidate: graph signal only, partial graph, or unresolved alternate.
- Dismissed: stronger AST/LSP/config/test evidence explains the signal; state the killed hypothesis.

“High fan-in/out” has no universal threshold. Compare within the same package and role, record the observed count/depth, and avoid percentile claims unless computed. Next, load `references/proof-ladder.md` to test every surviving prediction.
