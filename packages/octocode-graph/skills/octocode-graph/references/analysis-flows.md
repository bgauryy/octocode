# From a repository question to evidence

Use the native operations below with `graphQuery` on one pinned snapshot. For a saved export, pass the quoted query to `octocode-graph-query <out> "<query>"` or enter it in the viewer. Replace the paths and symbols with IDs from your scope. [Native operations](operations.md) · [Saved queries](query.md)

| Question | Flow | Return |
|---|---|---|
| Who calls this declaration? | Native `find` → `callers` on the symbol → `witnesses`. For overloaded or dynamically dispatched calls, inspect `unresolved`, then use `lspSearch` at the call site and read the deciding span. | Caller, callee, source location, resolution evidence and any open sites. |
| What should we retest? | Native `impact` on changed files → follow the returned test and entry-point continuations. In an export, compare `impact <file>` with `impact <file> --tests` and read the test edges. | Candidate tests and why each was reached. Record actual coverage or a test run separately. |
| How do two files connect? | In the export run `path <from> <to> --hops` for fewest edges and `path <from> <to>` for strongest structural route. Keep identical filters. Inspect import lines and call sites on each hop. The viewer offers **Compare fewest hops** under the weighted answer. | Direction, objective, hop count and witness for each edge. A reverse-only answer has `found:false`. |
| Which boundary deserves investigation? | Start with `top betweenness` for bridges on hop-shortest routes or `top pagerank` for incoming structural influence. Follow with `community <file>`, then inspect `deps <file>` and `dependents <file>` across the proposed boundary. | A named crossing with a source line, plus the effect of including weak edges when relevant. |
| What should move together? | Start with `cycle <file> --imports`. Inspect members and the example ring. Use `order <file> --imports` for prerequisite-first groups and inspect the source evidence crossing group boundaries. Check actual build manifests before scheduling a migration. | Mutually dependent group, outbound prerequisites, and the direction of the proposed move. |
| Did two modules evolve together? | Native `cochange` on a file → inspect the history window and commit evidence → compare with direct imports. For structural differences run `drift` against a retained baseline. | Commit coupling and static dependencies as separate findings. |
| Is a structural hotspot slow, untested, or on a mandatory execution path? | Use graph witnesses to locate the relevant source. Use a [matching artifact](providers.md) and query `trace test:<id> COVERS` or `trace workload:<name> OBSERVED EXECUTES_FILE`. Mandatory execution claims require compiler control-flow evidence. | Observed units, workload and revision, or a named missing measurement. |

**Worked route comparison.** On the saved `octocode-native` export, run both commands against the same result directory.

```sh
octocode-graph-query "$out" 'path crates/engine/src/lsp/client.rs crates/engine/src/lsp/spawn_limits.rs'
octocode-graph-query "$out" 'path crates/engine/src/lsp/client.rs crates/engine/src/lsp/spawn_limits.rs --hops'
```

The weighted route had two hops and reciprocal-strength cost 0.75. The direct route had one hop. Verify your own snapshot because edits can change either answer. For historical or negative claims, include snapshot scope, freshness, selected filters and unresolved coverage. [Evidence rules](evidence.md)

## Recheck a finding after editing code

1. Record the source scope, snapshot ID and exact query, including the edge and test filters. For a ranking, record the file ID and inspect its direct importers. Compare `dependents <file> --imports` with `dependents <file> --imports --tests`.
2. Read at least one witness on each edge of the proposed change. For a Rust module cycle, check whether the two directions are a module declaration, a re-export, an import or a call. Use LSP at the call site when a change depends on the exact target.
3. Write a regression for the observed behavior and run it before and after the edit. Build the changed scope again, then rerun the same query on the new snapshot. For a stitched ranking, create a fresh export and compare raw import and call endpoints, test flags and unanswered probes.
4. Report the changed behavior and its source evidence. An unchanged code graph can be the expected result of a performance fix. For a timing claim, measure cold creation, warm creation and a query against a pinned snapshot separately.

A cycle query reports the weight of its selected component. Its saved cycle ID appears only for an exact membership match. Older exports retain their original precomputed rankings and test classifications. Rebuild them to apply updated classification rules.
