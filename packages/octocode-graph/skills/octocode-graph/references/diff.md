Load when asked what a change, branch or refactor did to the structure, or to review a diff by its effect on dependencies.

# Diff snapshots

| Question | Tool | Covers |
|---|---|---|
| What did my last edits change structurally? | `octocode-graph refresh <dir>` (rebuilds only the chunks holding a change), then `octocode-graph diff <dir>` (`previous.json` → `data.json`) | The stitched graph: every package, metrics, LSP-confirmed calls. `refresh` keeps the replaced graph as `previous.json` |
| How do two builds compare (main against a branch, before and after a refactor)? | `octocode-graph diff <before-dir> <after-dir> [--json]` | The same, for any two saved runs of one project |
| What drifted in one scope since a retained snapshot? | native `graphQuery` `drift` with `baseline` ([native commands](operations.md#native-drift-and-impact)) | One snapshot scope; rows per file and relation, cycles added or resolved, static and dynamic transitions |
| What may a working-tree change affect? | native `graphQuery` `impact` with `since: "main"` ([native commands](operations.md#native-drift-and-impact)) | Changed nodes from Git, their dependents, the tests to run and the affected entry points |

## Saved-graph diff

`<dir>` is a run under `.octocode/octocode-graph/`. The text output leads with the review findings, high first:

| Finding | Severity | Meaning |
|---|---|---|
| New dependency cycle, or a cycle that grew or reshaped | high | Files that must now change together; the added members are listed |
| New package dependency against stability | high | A more stable package (lower instability) now depends on a less stable one |
| New package dependency | medium | Chunk A now imports or calls chunk B |
| More dependents | medium | A touched file now has more transitive dependents: its blast radius grew |
| Entered a ranking's top 20 | low | A file became a new PageRank or betweenness hotspot |
| Cycle shrank or resolved | low | Coupling went down |

Then come the dependencies added and removed (weight before → after, and whether they cross chunks), renames and files. A file renamed with the same path and a new extension, or moved as the only file with its name, keeps its edges, so a migration reads as renames instead of hundreds of removed and added edges. Call changes are counted. `--json` carries every list in full, and also the reweighted dependencies, coupling per package, every reach change and every ranking move.

## Review a change by its structure

1. Build or refresh, then run `diff` and read the high findings first.
2. For each new cycle, `cycle <file>` and `order <file> --imports`: read the crossing edge's line and decide whether to cut it.
3. For each new package dependency, `path <from-file> <to-file>` shows the route; check it against the intended layering.
4. For each file with more dependents, `impact <file> depth 2` lists what to retest; `critical path <file> --up` shows the longest chain above it.
5. Cite the lines behind each finding (`files <path>:<line>`, then read them).
