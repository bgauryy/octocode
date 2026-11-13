---
name: octocode-graph
description: "Use when querying code dependencies, callers, paths, cycles, hubs, impact, drift or Git co-change with graphBuild and graphQuery. Use octocode-graph-view for interactive architecture maps."
---

# Octocode Graph

The graph is a syntax snapshot of one scope: files, symbols and packages joined by `imports`, `calls`, `uses`, `inherits`, `passes` and `contains`. Rows identify where to inspect; execution or tests establish what runs or breaks.

Call the tools over MCP (`graphBuild`, `graphQuery`) or the CLI: `OCTO='node packages/octocode/out/octocode.js'` from the Octocode checkout, then `$OCTO graphBuild '{"queries":[{"path":"src"}]}'` and `$OCTO schema graphQuery` for the exact fields.

Use the **same `path`** for build and queries. After edits, check `stale` and run its `reingest` call when not fresh. Follow `responsePagination.next` before `next.*`, copying calls unchanged. For an empty or surprising answer, query at `medium` and inspect `unresolved` before claiming absence. [Build and read](references/build-and-read.md) covers long builds, confidence, weights and gaps.

## Question → operation

| Question | Operation |
|---|---|
| Size, coverage, unresolved reasons | `stats` (`view:"details"` for the breakdown) |
| Find a node ID | `find` with `target` words, then `node` |
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

## Prove, then answer

Before citing callers, absence or dead code, run `lspSearch` references at the row's `path`, `symbolName` and `line`, then read the source. Check consumers the graph cannot model: dynamic dispatch, registration and config. For `cochange` with `linked:false`, inspect shared config, schemas or generated code. `sites`, PageRank and co-change measure different things; none is runtime frequency or probability.

Return the answer with scope `path`, snapshot `id`, freshness, cited rows (`path:line`, operation, confidence), source/LSP checks and gaps. Label unchecked rows as leads and incomplete counts as floors. Include unresolved reasons and unbuilt scope. Save requested results under `<workspace>/.octocode/octocode-graph/` (scratch in `tmp/`); a user-named path wins. Use `octocode-engineer` for design decisions about a cycle, hub or boundary.
