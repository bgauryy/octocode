# From a repository question to evidence

Use the native operations below with `graphQuery` on one pinned snapshot. For a saved export, pass the quoted query to `octocode-graph-query <out> "<query>"` or enter it in the viewer. Replace the paths and symbols with IDs from your scope. [Native operations](operations.md) · [Saved queries](query.md)

| Question | Flow | Return |
|---|---|---|
| Who calls this declaration? | Use native `callers` with a known symbol ID or `find` to locate it. Use `witnesses` for call sites and `lspSearch` when dispatch is ambiguous. | Caller, callee, source location, resolution evidence and any open sites. |
| What should we retest? | Native `impact` on changed files → follow the returned test and entry-point continuations. In an export, compare `impact <file>` with `impact <file> --tests` and read the test edges. | Candidate tests and why each was reached. Record actual coverage or a test run separately. |
| How do two files connect? | In the export run `path <from> <to> --hops` for fewest edges and `path <from> <to>` for strongest structural route. Keep identical filters. Inspect import lines and call sites on each hop. The viewer offers **Compare fewest hops** under the weighted answer. | Direction, objective, hop count and witness for each edge. A reverse-only answer has `found:false`. |
| Which boundary deserves investigation? | Start with `top betweenness` for bridges on hop-shortest routes or `top pagerank` for incoming structural influence. Follow with `community <file>`, then inspect `deps <file>` and `dependents <file>` across the proposed boundary. | A named crossing with a source line, plus the effect of including weak edges when relevant. |
| What should move together? | Start with `cycle <file> --imports`. Inspect members and the example ring. Use `order <file> --imports` for prerequisite-first groups and inspect the source evidence crossing group boundaries. Check actual build manifests before scheduling a migration. | Mutually dependent group, outbound prerequisites, and the direction of the proposed move. |
| Did two modules evolve together? | Native `cochange` on a file → inspect the history window and commit evidence → compare with direct imports. For structural differences run `drift` against a retained baseline. | Commit coupling and static dependencies as separate findings. |
| Is a structural hotspot slow, untested, or on a mandatory execution path? | Use graph witnesses to locate the relevant source. Use a [matching artifact](providers.md) and query `trace test:<id> COVERS` or `trace workload:<name> OBSERVED EXECUTES_FILE`. Mandatory execution claims require compiler control-flow evidence. | Observed units, workload and revision, or a named missing measurement. |

For an unresolved member call, `member-unknown-receiver-in-repo` means a matching name exists in the repository. Use `astSearch` to locate the call, then ask `lspSearch` for the definition at the receiver's method name and line (and occurrence for repeated names). A `locationsWithheld` answer can identify an external definition while keeping its source outside the read scope. Retain the reason alongside the open call site.

## Choose a traversal

For several changed files, `impact a.rs b.rs depth 2` seeds one reverse breadth-first search. Distance is the minimum hop count from any seed. Run each seed separately with `--stdin` when the question is which changes reach a particular consumer.

For a route between two known files, use BFS (`path a.rs b.rs --hops`) for the fewest boundaries, or Dijkstra (`path a.rs b.rs`) to minimize the sum of reciprocal edge strengths. A strongly coupled route can be longer. Runtime latency needs timing evidence. [Shortest-path algorithms](https://networkx.org/documentation/stable/reference/algorithms/shortest_paths.html)

For migration order, start with `cycle a.rs --imports` and `order a.rs --imports`. Condensation contracts each strongly connected component into a group and produces an acyclic dependency graph. Inspect the returned crossing witnesses before deciding which group to move. [SCC condensation](https://networkx.org/documentation/stable/reference/algorithms/generated/networkx.algorithms.components.condensation.html)

## Compare revisions

Repeat the query with the same scope, direction and edge filters on each snapshot. For changed call bindings, compare the witness locations. For a change in ranking, inspect added or removed neighbors. Measure timings separately when the question is about performance.

A cycle query reports the weight of its selected component. Its saved cycle ID appears only for an exact membership match. Older exports retain their original precomputed rankings and test classifications. Rebuild them to apply updated classification rules.
