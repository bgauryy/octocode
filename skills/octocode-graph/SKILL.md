---
name: octocode-graph
description: "Load this skill first when asked who imports or calls what, dependency paths, cycles, change impact or dead-code candidates: graphBuild and graphQuery answer from a code graph, not grep. Interactive map: octocode-graph-view."
---

# Octocode Graph

The graph is a syntax snapshot of one scope: files, symbols and packages joined by `imports`, `calls`, `uses`, `inherits`, `passes` and `contains`. Rows identify where to inspect; execution or tests establish what runs or breaks.

Call `graphBuild` and `graphQuery` over MCP when listed (`OCTOCODE_BETA=true`). The CLI always offers them. From the Octocode checkout, run `node packages/octocode/out/octocode.js schema graphQuery`. Elsewhere use `octocode`, `octocode-cli` in Octocode Code agent shells, or `npx -y octocode`. To graph a `ghCloneRepo` checkout, set `OCTOCODE_GRAPH_HOME` in the home environment first: without it graphBuild builds only scopes inside the workspace.

Build over a `path` that contains the target's possible consumers (often the repository root), and use the same `path` for every query. After edits, check `stale` and run its `reingest` call when not fresh. A reply with a top-level `next` is unfinished: run it unchanged until none is left, then the row's `next.*` branches. For an empty or surprising answer, query at `medium` and inspect `unresolved` before claiming absence. [Build and read](references/build-and-read.md) covers long builds, confidence, weights and gaps.

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

Lead with the answer and its cited rows (`path:line`, operation, confidence) and the source/LSP checks run. Add scope `path`, snapshot `id`, freshness, unresolved reasons and unbuilt scope where they limit a claim. Label unchecked rows as leads and incomplete counts as floors. Dead-code rows are leads: deletion needs a removal proof (`octocode-clean-agentic-code`). Save requested results under `<workspace>/.octocode/octocode-graph/` (scratch in `tmp/`); a user-named path wins. Use `octocode-engineer` for design decisions about a cycle, hub or boundary.
