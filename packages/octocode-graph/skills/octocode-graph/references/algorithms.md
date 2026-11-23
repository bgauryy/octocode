Load when judging code quality, risk or architecture from a saved graph, or before reading a metric as a fact.

# Graph algorithms and code-quality checks

## The weighted graph

A saved graph has file nodes (plus workspace package nodes) and a function call graph. File edges carry one weight:

**w(u→v) = 1 if u imports v + Σ call sites from u into v × evidence**, where evidence is 1 for a call the language server confirmed or the graph bound with high confidence, 0.5 for medium and 0.25 for low.

- **Production graph:** test-only edges are excluded from every metric. Add `--tests` to a query to include them. Most isolated files are tests.
- **Strong and weak edges:** walks (`impact`, `deps`, `path`, `cycle`, `critical path`) follow strong edges, meaning an import or a full-evidence call. Metrics count every production edge. `--weak` adds edges backed only by name-bound calls; each walk's summary says what `--weak` would add.

## What each algorithm measures

| Algorithm | Weighted | Query | High value means | Not |
|---|:-:|---|---|---|
| PageRank, damping 0.85, flowing to dependencies | yes | `top pagerank`, `about` | Foundational: important code rests on it | runtime frequency or complexity |
| Function PageRank over the call graph | yes | `critical functions` | A function much of the code ultimately calls into; rows add call sites, callers and transitive callers | dynamic dispatch the graph never bound |
| In-weight / out-weight | yes | `top inweight`, `top outweight` | Heavily used / uses a lot | |
| Betweenness (Brandes, hop counts) | no | `top betweenness` | Bridge on many shortest dependency routes: a choke point | weight-aware |
| Articulation points (Hopcroft–Tarjan) | no | `critical points` | The only link holding a part of the graph to the rest; `parts` gives the sizes it would split | |
| Strongly connected sets (Tarjan) | no | `cycle <file>`, `metrics.cycles` | Files that must change and test together | proof that code runs in a loop |
| Dependency depth over the SCC condensation | no | `top depth` | Stacked on many layers | |
| Longest chain | no (hops; ties take the heavier edge) | `critical path [<file>] [--up]` | The exact chain a file rests on, or the dependents a change to it can travel up through | |
| Condensation order | no | `order <file> [--imports]` | Prerequisite-first migration groups | build order |
| k-core (Batagelj–Zaversnik) | no | `top core` | Part of the densest tangled core | |
| Louvain modularity | yes | `community <file>`, wiki pages | The real module boundaries; `purity` is the share in one chunk | folder layout |
| Martin coupling: Ca, Ce, instability I = Ce/(Ca+Ce) | counts | `metrics.packages`, `diff` | I near 0 is stable (many depend on it), near 1 is volatile | quality by itself |
| Dijkstra on 1/w, BFS | yes / no | `path a b`, `path a b --hops` | The strongest coupling route, or the fewest boundaries | latency |
| Reverse BFS | no | `impact <file> [depth N]` | Everything a change may affect | proof of breakage |
| Forward ∩ backward reachability, dominators (Cooper–Harvey–Kennedy) | no | `region <entry> <outcome>` | The code that can take part in a flow, and the steps every modeled route must pass | routes through unresolved dispatch |
| Blocked-node shortest path | yes / no | `path <a> <b> --avoid <dir>` | A route around a required layer: an architecture-rule violation | |
| Regular-path automaton (Thompson) | per relation | `trace <seed> <pattern>` | Typed walks such as `DEFINES CALLS+` | |

Cycles, rings, fewest-hop paths, reverse reach and PageRank follow one spec shared with the engine's `graphQuery`. Both implementations pass the same fixture cases (`test-utils/graph-algorithms/`).

## Code-quality checks

Batch the first queries of a check in one load (`--stdin`), then read only the cited lines.

| Check | Ask | Signal | Next step |
|---|---|---|---|
| Critical functions to test first | `critical functions 30` | High PageRank or many transitive callers | `callers <fn> --tests` against `callers <fn>`: equal counts mean no test calls it directly |
| Choke points | `top betweenness 10`, `critical points` | One file carries many routes or holds a part together | `about <file>`; give it an owner and tests, or split the interface it exposes |
| God files | `top inweight 20`, `about <file>` | Heavy use plus many declarations whose callers sit in different communities | Split along the communities its callers come from |
| Cycles | `cycle <file>`, `order <file> --imports` | A set that must move together; ring rows carry each hop's weight | Cut the lightest crossing edge; confirm at its cited line |
| Stability violations | `metrics.packages`, `diff` (`againstStability`) | A stable package (low I) depends on a volatile one | Invert the dependency or move the shared code down |
| Change risk | `impact <file> depth 3`, `critical path <file> --up` | Many dependents or a long chain above | Run the tests on that chain first |
| Layering | `model.json` layers, then `octocode-graph render` | `upward_imports` in the brief | Read each upward import at its line |
| Dead code candidates | `about <file>` (0 dependents), native `issues` | No dependents and not an entry point | Confirm with `lspSearch` references; dynamic loading is invisible |
| Module boundaries | `community <file>` | Low purity: the community spans chunks | Compare with the packages a change touches |
| Architecture rule ("the CLI reaches storage only through the service layer") | `path <cli-file> <store-file> --avoid <service-dir>` | A route found: the rule is broken at each cited hop | Fix the crossing import, or record an allowed exception in `model.json` |
| A critical flow | `region <entry> <outcome>` (files) or `region <a#fn> <b#fn>` (functions) | `mustPass` steps: checkpoints a change cannot route around | Test the checkpoints; `path --avoid` one to prove an alternative exists |

## Flows that need more than one query

| Question | Flow |
|---|---|
| What should we retest? | `impact <file...> depth 2` lists dependents; `impact <file> --tests` adds the tests that reach it. Native `impact` with `changed` or `since: "main"` lists `testsToRun` and `affectedEntrypoints`. Record actual coverage or a test run separately. |
| Which seed reaches a consumer? | One `impact a b depth 2` gives each file its minimum distance from any seed; ask one query per seed over `--stdin` to attribute it. |
| Fewest boundaries or strongest coupling? | `path a b --hops` (BFS) against `path a b` (Dijkstra on 1/w): a strongly coupled route can be longer. Keep the same filters for both. |
| What should move together? | `cycle <file> --imports`, then `order <file> --imports`: condensation groups each strongly connected set; read the crossing witnesses before choosing the first group to move. |
| Did two modules evolve together? | Native `cochange` on a file gives commit coupling; compare it with direct imports, and with `diff` across builds for structure. |
| Is a hotspot slow, untested or on a mandatory path? | Attach a [provider](providers.md) (coverage, control flow, traces) and `trace test:<id> COVERS` or `trace workload:<name> OBSERVED EXECUTES_FILE`. Mandatory-path claims need control-flow evidence. |

Native detectors run on one snapshot scope with `graphQuery` `issues` (`detectors`, `minTier`): `cycle` and `dir-cycle` (with suggested cuts), `unreachable-file`, `test-only`, `unused-export`, `export-only-local`, `undeclared-dependency`, `boundary-violation`, `unresolved-import`, `critical-file` and `single-point-of-failure`. Findings are ranked hypotheses: read each one's `evidence` and confirm a dead-code finding with `lspSearch` references.

Rows are evidence candidates. Before a claim, read the cited `path:line`, and use `lspSearch` for an uncertain binding.
