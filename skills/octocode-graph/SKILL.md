---
name: octocode-graph
description: "Load this skill first when asked for callers, imports, dependency paths, cycles, change impact or dead-code candidates. Query a code graph and verify the relevant edges in source."
---

# Octocode Graph

Build a snapshot over a `path` that contains the target and its possible consumers. Query that same path. Files, symbols and packages are connected by `imports`, `calls`, `uses`, `inherits`, `passes` and `contains`. These are syntax relationships. Verify execution claims with tests or a trace.

Use `graphBuild` and `graphQuery` over MCP when listed (`OCTOCODE_BETA=true`), or use the CLI. For invocation, cloned checkouts and long builds, follow [create a snapshot](references/build-and-read.md#create).

After edits, check `stale` and run the supplied `reingest` call when needed. Finish each top-level `next` before following row evidence. For an empty or surprising answer, check `medium` confidence and `unresolved`. Use [row interpretation](references/build-and-read.md#read-the-rows) for weights, coverage and depth.

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
