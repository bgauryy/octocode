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
| Dead code, choke points, layering | `issues` (`detectors`, `minTier`, `baseline`) |
| What depends on a change | `impact` with `target`, `changed` or `since` |
| Structural change since a snapshot | `drift` with `baseline` |
| Files that change together in Git | `cochange` with a file `target` |
| Every occurrence behind an edge, or why a call stayed unlinked | `witnesses` / `unresolved` |

For invocation, pagination, coverage and edge confidence, see [snapshots and rows](build-and-read.md).
