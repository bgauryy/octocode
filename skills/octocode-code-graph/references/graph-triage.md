# Graph Triage

Load when selecting a graph operation and defining what its result can predict. Why: a narrow question avoids expensive whole-repository scans and smell inflation.

| Question | `localAnalyzeGraph` operation | Required inputs | Candidate signal |
|---|---|---|---|
| What can this file affect? | `dependents` | `path`, `file`, bounded `depth` | change affected scope |
| What does this file rely on? | `dependencies` | `path`, `file`, bounded `depth` | coupling or boundary crossings |
| Can layer A reach layer B? | `path` | `path`, `file`, `target` | concrete dependency chain |
| Which files mutually depend? | `cycles` | `path` | SCC requiring runtime-edge review |
| What is outside known roots? | `reachability` | `path`, explicit `entrypoints` when known, `includeTests` | orphaned or alternate-entry code |
| What might be removable? | `deadCode` | same root policy as reachability | unreachable export/cluster candidate |

## Efficient sequence

1. Read the live schema; do not copy fields from this table blindly.
2. Start at the narrowest known file/path. Use repository-wide `cycles`, `reachability`, or `deadCode` only when that is the question.
3. Fix `maxFiles`, `limit`, `pageSize`, exclusions, tests policy, and entrypoints so repeats are comparable. `limit` bounds the total candidate set; `pageSize` bounds one returned page.
4. Read summary, warnings, confidence, edge kinds, and `next.*` before interpreting results. For cycles, start from the directed `cycleEdges`; use `runtimeCycleEdges` when the claim is specifically about module loading.
5. For traversal, use `immediateDominator` as a mandatory-path candidate, `topologicalLayer` as ordering evidence, and `transitiveEdge:true` as a redundant-edge candidate; exact-read the involved edges before an architectural verdict.
6. Form at most three hypotheses: `signal → possible mechanism → observable impact → proof needed`.
7. Drop hypotheses whose impact cannot change a decision.

Graph absence is scoped absence only. Truncation, unresolved aliases, dynamic imports, framework discovery, generated code, and unsupported languages can hide edges. When a hypothesis survives, continue to `references/proof-ladder.md` to upgrade it beyond topology. <!-- style-lint: ignore-line passive-voice -->
