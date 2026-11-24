---
name: octocode-graph
description: "Load this skill first when asked how code is connected, before tracing imports or callers by hand: find code by name or by what it does, callers, dependency paths, cycles, change impact, critical code, what an entry loads and why (runtime vs type-only, startup vs lazy, bytes), dependency rules for CI, or a repository map. Builds, saves and queries a cited graph across languages; not for literal text search."
---

# Octocode Graph

Answer structural questions from a saved graph: one load answers many questions in milliseconds, offline, and every row carries `path:line`. Run `octocode-graph <command>` when it is on PATH; otherwise (`command not found`) `npx -y @octocodeai/octocode-graph <command>`, the same commands and flags with nothing installed (inside a project or workspace that installs the package, npx runs that copy). The CLI's own hints name the form that works where it runs.

```sh
octocode-graph build .                              # into .octocode/octocode-graph/<run> (--out elsewhere); unchanged code restores from the shared cache
octocode-graph query . "search where are cycles counted"   # newest graph of this directory (or one built with --out); stderr names it
octocode-graph query . "reach src/worker.ts" --refresh     # --refresh rebuilds the chunks edited since the build first
octocode-graph check . --format sarif                      # dependency rules from .octocode/graph-rules.json; exit 1 on a new error
```

## Fast loop

| Step | Query | Returns |
|---|---|---|
| 1. Find it, by name or by what it does | `search <words> [--kind\|--lang\|--role\|--path <v>] [--exported] [--param\|--returns <type>] [--all]` | Exact and prefix names first, then BM25F over names, paths, doc comments, calls and signatures; `found` says why. Type filters keep recorded matches and count the rest (`--unsigned` keeps them) |
| 2. See what surrounds it | `about <file\|symbol>`; `context <file\|symbol...> --tokens N` | Metrics, community, dependencies and dependents, callers and callees; `context` ranks the neighbourhood under a token budget, `more:` continues |
| 3. Ask the relationship | `impact`, `deps`, `dependents`, `path [--all]`, `cycle`, `order`, `trace`, `callers`, `callees`, `top`, `community` | Complete rows with depth, weight and the witness line. [Saved queries](references/query.md) |
| 4. What runs, and what it costs | `--runtime` (drops `import type`), `--eager` (startup only) on any walk; `reach <entry\|dir>` files, bytes, packages, lazy boundaries; `why <entry> <target>`; `hubs [<dir>]` | Each step `a.ts:9 [kinds → load] → b.ts` is the import at that line of `a.ts` that pulls in the next file, and how it loads; `reach --forbid pkg:x` fails a rule with exit 1 |
| 5. What is critical | `critical functions`, `critical points`, `critical path [<file>] [--up]` | Code much depends on, files that alone hold parts together, the longest chain |
| 6. Prove it | `localFetch` the cited lines | The source behind the claim. [Evidence](references/evidence.md) |

Batch questions in one load, one per line: `printf '%s\n' 'search x' 'about a.ts' | octocode-graph query . --stdin --json`. Natural phrasing (`callers of x`, a typo, `top pr`) is read as one exact query and `read as:` shows it; an ambiguous name returns re-runnable candidates. Build a workspace from its root: one package's build keeps sibling packages as externals, and its stderr and `brief.txt` (`SCOPE`) say which. Builds share a content-addressed cache: `build --explain-cache` says why a chunk or probe was rebuilt.

## Which tool decides

| Question | Use | Measured |
|---|---|---|
| Structure: impact, paths, cycles, loads, rankings, layering | The saved graph | Cross-package structure in one call (697 files, 11 crates, 0.36 s) |
| Every caller of one function | `lspSearch callers`; a graph `callers` answer without full probes is `isPartial` (`callersNotProbed`) and carries that call as `confirm` | Graph callers recall 0.94–1.00 on 13 of 14 language battery rows |
| A fresh scope, or native detectors (`issues`, `cochange`, `drift`, `witnesses`, `unresolved`) | `octocode-graph graphQuery '{"queries":[{"path":".","operation":"issues"}]}'` | [Operations](references/operations.md) · [Builds and pagination](references/build-and-read.md) |
| A spelling or literal text | `localSearch` (or `rg`) | Text is not structure |

## Go further

| Need | Read |
|---|---|
| Dependency rules in CI: forbidden, allowed, required, reachable, circular; baseline, SARIF, `--since <rev>` | [Rules](references/rules.md) |
| Bundle bytes and "why is this in the bundle", CPU and heap profiles (`hot`), minified frames (`locate`), CodeQL or Joern data flow | [Providers](references/providers.md) |
| Quality, risk, retest and migration order, what each metric means; what a change did to the structure | [Algorithms](references/algorithms.md) · [Diff](references/diff.md) |
| Docs, configs and manifests to code (`links`); record a link the graph missed | [Links](references/links.md) |
| Build options, cache, registry, viewer, replay, HTML | [Commands](references/commands.md) · [Output](output.md) |
| Viewer chat, durable notes, architecture layers | [Chat](references/chat.md) · [Wiki](references/wiki.md) · [Modeling](references/modeling.md) |

MCP: `octocode-graph mcp` serves `build_graph` → `query_graph` (`result`, `refresh`) → `refresh_graph` → `diff_graph`, `check_graph`, `link_graph`, beside the engine's `graphBuild` and `graphQuery`; `channel` adds the live viewer. In code: `buildGraph`, `openGraph(dir).query(text)`, `openFreshGraph(dir)`, `refreshGraph`, `diffFrom` from `@octocodeai/octocode-graph`.
