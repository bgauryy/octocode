Load before reading a metric or ranking as a fact: how edges are weighted, and what each algorithm measures and does not.

# Graph algorithms and code-quality checks

## The weighted graph

A saved graph has file nodes (plus workspace package nodes) and a function call graph. File edges carry one weight:

**w(u→v) = 1 if u imports v + Σ call sites from u into v × evidence**, where evidence is 1 for a call the language server confirmed or the graph bound with high confidence, 0.5 for medium and 0.25 for low.

- **Production graph:** test-only edges are excluded from every metric. Add `--tests` to a query to include them. Most isolated files are tests.
- **Strong and weak edges:** walks follow strong edges while metrics count every production edge, so the two can disagree about one file: [saved queries](query.md) owns the rule and `--weak`.

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
