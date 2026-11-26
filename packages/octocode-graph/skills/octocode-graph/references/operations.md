Load when a question needs a fresh scope or a native detector: picks the `graphQuery` operation and compares retained snapshots (`drift`, `impact`).

# Native graph queries

| Question | Operation |
|---|---|
| Size, coverage, unresolved reasons | `stats` (`view:"details"` for the breakdown) |
| Search for a file or symbol by name | `find` with `target` words returns ranked IDs |
| Inspect a known ID | Pass the ID directly to `node` for details, or to `callers`, `callees` or `dependents` for the relationship |
| What a file imports / who imports it | `deps` / `dependents` (`depth`, `view:"files"\|"modules"`) |
| Who calls / what it calls | `callers` / `callees` on a symbol ID |
| How A reaches B | `path` with `target` and `to` |
| Most depended on | `hubs` (`rankBy:"degree"` default, `rankBy:"pageRank"` for importance weighted by sites) |
| Import loops | `cycles` (`runtime` identifies a cycle closed by eager module-loading imports; it does not prove execution) |
| Dead code, choke points, layering | `issues` (`detectors`, `minTier`, `baseline`). Detectors: `cycle` and `dir-cycle` (with suggested cuts), `unreachable-file`, `test-only`, `unused-export`, `export-only-local`, `undeclared-dependency`, `boundary-violation`, `unresolved-import`, `critical-file`, `single-point-of-failure` |
| What depends on a change | `impact` with `target`, `changed` or `since` |
| Structural change since a snapshot | `drift` with `baseline` |
| Whether the graph still matches the files | `stale`: `fresh:false` comes with a `reingest` call to run |
| Files that change together in Git | `cochange` with a file `target` |
| Every occurrence behind an edge, or why a call stayed unlinked | `witnesses` / `unresolved` |

For invocation, pagination, coverage and edge confidence, see [snapshots and rows](build-and-read.md).

## Native drift and impact

Keep a baseline. A build keeps `keep` snapshots per scope (default 3), so raise `keep` before a long series. Then compare:

```sh
octocode-graph graphBuild '{"queries":[{"path":"packages/api","keep":5}]}'        # note data.graph
octocode-graph graphQuery '{"queries":[{"operation":"drift","path":"packages/api","baseline":"<older id>"}]}'
octocode-graph graphQuery '{"queries":[{"operation":"impact","path":"packages/api","since":"main"}]}'
```

`drift` counts `filesAdded`, `filesRemoved`, `filesChanged`, `cyclesAdded`, `cyclesResolved`, `relationsAdded` and `relationsRemoved`, and pages its rows: add `--all-pages` to read them all in one reply. It does not detect renames, so a renamed file appears removed and added.
