# Code graph

`graphBuild` turns one directory into a syntax snapshot of file, symbol and package nodes joined by typed edges. `graphQuery` answers structural questions from it: importers, callers, paths, cycles, hubs, dead-code hypotheses, change impact, drift and Git co-change.

Answers are syntax candidates that show **where to inspect**. They do not prove what runs or breaks. Confirm a row before you cite it ([Prove](#prove)).

## Turn it on

The CLI always has both tools. In MCP they are beta: set `local.beta` to `true` (`OCTOCODE_BETA`, [settings](CONFIGURATION.md#all-settings-and-defaults)).

Snapshots go to `<workspace>/.octocode/graph`. `local.graphHome` (`OCTOCODE_GRAPH_HOME`) moves them. It is home-only, so a repository cannot set it; with it, a build can scan any allowed directory, such as a `ghCloneRepo` clone.

## Build

```bash
octocode graphBuild '{"queries":[{"path":"src"}]}'
```

Build the smallest directory that holds every file your question spans. The host's permission prompt approves each build.

| Option | Default | Effect |
|---|---|---|
| `path` | `.` | Scope to build. Use the same `path` for the queries. |
| `exclude` | none | Extra globs to skip, relative to `path` (up to 100). |
| `defaultExcludes` | `true` | `false` also walks build, dependency and cache directories. |
| `parserGlobs` | none | Parse other file names with a language (`typescript`, `python`, and so on). |
| `includeIgnored`, `includeHidden` | `false` | Include Git-ignored or hidden files. |
| `keep` | `3` | Snapshots kept per scope. Raise it to compare builds. |
| `refresh` | `false` | Rebuild an unchanged scope. Discards checkpoints of an interrupted build. |

| Situation | What happens |
|---|---|
| Scope unchanged | The existing snapshot returns at once. |
| Files edited since | `graphQuery` `stale` reports `fresh: false` and gives a `reingest` call. |
| Long build | It pauses with progress. Run `next.expandScan`, or repeat the call. |
| Scope too large | `scopeTooBroad` (`correctInput`) names narrower scopes to build. |

## Query

```bash
octocode graphQuery '{"queries":[{"operation":"hubs","path":"src","kind":"file","rankBy":"pageRank"}]}'
octocode graphQuery '{"queries":[{"operation":"impact","path":"src","target":"src/config.ts"}]}'
```

Set `path` to the build's scope. Omit `graph` to read the newest snapshot of that exact path, or pass the build's `id`. `octocode schema graphQuery --view query --select operation=<value>` lists the fields one operation takes.

| Question | `operation` | Required fields |
|---|---|---|
| Size, coverage, unlinked calls | `stats`, `unresolved`, `diagnostics`, `stale` | none |
| Find a node | `find`, `candidates`, `node`, `symbols` | `target` (except `find`, `symbols`) |
| Imports and importers | `deps`, `dependents` | `target` |
| Callers and callees | `callers`, `callees`, `walk` | `target` |
| How A reaches B | `path` | `target`, `to` |
| Most depended on | `hubs` | none. `rankBy`: `degree` (default) or `pageRank` |
| Import loops | `cycles` | none |
| Dead code, choke points, layering | `issues` | none. Optional `baseline` marks findings `new` or `existing` |
| What depends on a change | `impact` | one of `target`, `changed`, `since` |
| Structural change between snapshots | `drift` | `baseline` |
| Files changed together in Git | `cochange` | `target` |
| Every occurrence behind an edge | `witnesses` | none |

Narrowing: `kind` (`file`, `symbol` or `package`), `confidence` (see [Read the answer](#read-the-answer)), `edges` (`contains`, `imports`, `uses`, `calls`, `inherits`, `passes`), `direction` and `depth` for traversal, and `limit` (rows per page, default 50; `responseChars` can end a page first).

## Read the answer

**Weights.** Each signal has one meaning. None is a probability or a runtime frequency.

| Signal | Field | Meaning |
|---|---|---|
| Binding confidence | `confidence` | `high` (default): strong syntax binding. `medium`: adds scoped or ambiguous candidates. `low`: adds a bare call's only declaration of its name. Ask for `medium` before you read an empty answer as absence. |
| Connection strength | `sites` | Source occurrences behind an edge. On `hubs`, summed over the counted edges. |
| Structural importance | `pageRank` | With `rankBy: "pageRank"`: rank over the admitted edges, each weighted by `sites`. All nodes sum to 1. |
| Change coupling | `commits`, `share`, `linked` | `cochange`: commits that changed both files, divided by the target's commits. `linked: false` means coupling with no graph edge. |
| Runtime frequency | none | Not modeled. It needs profiling data. |

**Impact.** Rows are dependents, not breakage. `risk` gives the distance from the change: `changed` (0), `direct` (1, inspect first), `likely` (2), `transitive` (3 or more), and `candidate` (an import-only importer that no linked reference proves affected). `summary.testsToRun` counts every test file the change reaches, even past `depth`, and `next.testsToRun` lists each with the test functions reached in it. `next.affectedEntrypoints` lists the entrypoints to look at. Only tests or execution prove a break. Dynamic dispatch, reflection and string loading are not modeled.

**Issues.** Findings are ranked hypotheses, each with `evidence` and the `controls` applied:

```
score = severity × confidence × (0.5 + 0.5 × rank)      ×1.1 (at most 1) per other detector on the same subject
```

`rank` is the highest PageRank percentile among the subject's files. `critical-file` lists every production file with five or more transitive runtime dependents, so paging reaches all of them.

**Edges.** `calls` come from call sites. A named function passed as an argument gives a `passes` edge: an argument slot, not proof that it runs. Other value uses (assignment, return) give no edge. A bare call of a parameter, local variable or closure links to no same-named declaration and stays unresolved with reason `local-binding`. `unresolved` lists the unlinked sites and their reasons.

**Pages.** A reply with a top-level `next` is unfinished: run it until none is left. A stated `total` is exact. See [continuations](TOOL_DATA_CONTRACT.md#executable-continuations).

## Prove

Before you cite callers, an absence or dead code, run `lspSearch` `references` at the row's `path`, `symbolName` and `line`, then read the source. The [graph skill](../skills/octocode-graph/SKILL.md) follows the same routine. To see a whole repository, use [Graph View](../skills/octocode-graph-view/).
