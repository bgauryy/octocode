---
name: octocode-graph
description: "Query code relationships or explore a repository map. Search names, find callers, imports, dependency paths, cycles, change impact and critical code, diff snapshots, or build, serve and replay a graph with source evidence."
---

# Octocode Graph

Answer structural questions from a saved graph: one load answers many questions in milliseconds, offline, and every row carries `path:line`.

```sh
octocode-graph query . "search parseArgs"     # newest saved graph of this directory; stderr names it
octocode-graph build .                         # none yet; --lsp off skips call confirmation (76 of 127 s on 3,516 Rust files)
octocode-graph refresh <result-dir>            # after edits: rebuilds only the chunks holding a change
```

## Fast loop

| Step | Query | Returns |
|---|---|---|
| 1. Locate a name you partly know | `search <text> [--files\|--symbols] [--all]` | Ranked by match (exact, prefix, word initials, substring, words, path, letters), then call sites and PageRank; the best 20, with the `--all` form for the rest |
| 2. See everything around it | `about <file\|file:line\|symbol>` | Metrics, rank, community, direct dependencies and dependents, declarations; for a symbol, callers and callees |
| 3. Ask the relationship | `impact`, `deps`, `dependents`, `path`, `cycle`, `order`, `community`, `top`, `trace`, `callers`, `callees` | Complete rows with depth, weight and the witness line. [Saved queries](references/query.md) |
| 4. Find what is critical | `critical functions`, `critical points`, `critical path [<file>] [--up]` | Functions much code calls into, files that alone hold parts together, the longest dependency chain |
| 4b. Connect docs and configs | `links [<file>]`, `links --unresolved`, `about <file>` (`documentedBy`) | Doc and manifest lines that name a file or symbol; what they name that resolves to no single target, for you to settle with `octocode-graph link` ([Links](references/links.md)) |
| 5. Prove it | `localFetch` the cited lines | The source behind the claim. [Evidence](references/evidence.md) |

Batch independent questions in one load, one per line: `printf '%s\n' 'search x' 'about a.ts' | octocode-graph query . --stdin --json`. Write queries naturally: `callers of x`, a typo or `top pr` is read as one exact query and `read as:` shows it. An ambiguous or unknown name returns candidates as re-runnable queries. When stderr reports files changed since the build, refresh before trusting their edges.

## Which tool decides

| Question | Use | Measured 2026-11-23 |
|---|---|---|
| Structure: impact, paths, cycles, rankings, communities, critical code, layering | The saved graph | The only tool here answering cross-package structure in one call (697 files across 11 crates, 0.36 s) |
| Every caller of one function | `lspSearch callers` at its declaration; a graph `callers` answer from a build that did not probe every callable is `isPartial` (`callersNotProbed`) and carries that call as `confirm` | Callers: LSP recall 97% Rust / 100% TS; graph 100% TS; Rust 97% with the default `--lsp smart` (138 of 143 sites; 69 of 70 from other crates) and 95% with no language server (`--lsp off`, 136 of 143); calls inside macros in test code need `lspSearch callers`; a build whose user settings allow build scripts and proc macros also takes rust-analyzer SCIP targets (`via: scip`) |
| One fresh scope, or native detectors (`issues`, `cochange`, `drift`, `witnesses`, `unresolved`) | `octocode-graph graphQuery '{"queries":[{"path":".","operation":"issues"}]}'` | [Native operations](references/operations.md) · [Builds and pagination](references/build-and-read.md) |
| A spelling or literal text | `localSearch` (or `rg`) | Text is not structure |

## Go further

| Need | Read |
|---|---|
| Judge code quality or risk, retest, migration order, runtime evidence; what each metric means | [Algorithms, checks and flows](references/algorithms.md) |
| What a change, branch or refactor did to the structure | [Diff snapshots](references/diff.md) |
| Docs, configs and manifests to code; record a link the graph did not find | [Links](references/links.md) |
| Build options, the viewer, replay and HTML | [Commands](references/commands.md) · [Output](output.md) |
| Viewer chat, durable notes, architecture layers | [Chat](references/chat.md) · [Wiki](references/wiki.md) · [Modeling](references/modeling.md) |

MCP: `octocode-graph mcp` serves the saved-graph loop (`build_graph` → `query_graph` with `result` → `refresh_graph` → `diff_graph`, and `link_graph`) beside the engine's `graphBuild` and `graphQuery`; `octocode-graph channel` adds the live viewer. In code: `buildGraph`, `openGraph(dir).query(text)`, `refreshGraph`, `diffFrom`, `proposeLink` from `@octocodeai/octocode-graph`.
