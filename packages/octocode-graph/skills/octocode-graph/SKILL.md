---
name: octocode-graph
description: "Load this skill first when asked what code depends on what: callers, dependency paths, cycles, change impact, what an entry loads, CI dependency rules or a repo map. Builds and queries a cited graph."
---

# Octocode Graph

Answer structural questions from a saved graph; every row cites `path:line`. Run `octocode-graph <command>`; when it is not on PATH (`command not found`), `npx -y @octocodeai/octocode-graph <command>` takes the same commands and flags. The CLI's own hints name the form that works where it runs. Build a workspace from its root: one package's build keeps sibling packages as externals, and its stderr and `brief.txt` (`SCOPE`) name them. With no saved graph, `query` fails with the `build` command to run.

```sh
octocode-graph build .                              # into .octocode/octocode-graph/<run> (--out elsewhere); unchanged code restores from the shared cache
octocode-graph query . "search where are cycles counted"   # newest graph of this directory (or one built with --out); stderr names it
octocode-graph query . "reach src/worker.ts" --refresh     # --refresh rebuilds the chunks edited since the build first
octocode-graph check . --format sarif                      # dependency rules from .octocode/graph-rules.json; exit 1 on a new error
```

## Ask

| Question | Run |
|---|---|
| Find code by name or by what it does | `search <words>`; filter with `--kind`, `--lang`, `--role`, `--path`, `--exported`, `--param`, `--returns` |
| What surrounds a file or symbol | `about <file\|symbol>`; `context <file\|symbol...> --tokens N` ranks the neighbourhood under a token budget (`more:` continues) |
| Who depends on it or calls it, what it uses | `impact`, `dependents`, `deps`, `callers`, `callees` |
| Every caller of one function | `lspSearch callers`; a graph `callers` answer without full probes is `isPartial` (`callersNotProbed`) and carries that call as `confirm` |
| How A reaches B, loops, prerequisite order | `path [--all]`, `cycle`, `order`, `trace` |
| What an entry loads, why, at what cost | `reach <entry\|dir>`, `why <entry> <target>`, `hubs [<dir>]` |
| What is critical | `critical functions`, `critical points`, `critical path [<file>] [--up]`, `top <metric>`, `community` |
| A fresh scope, or native detectors (`issues`, `cochange`, `drift`, `witnesses`, `unresolved`) | `octocode-graph graphQuery '{"queries":[{"path":".","operation":"issues"}]}'` · [Operations](references/operations.md) · [Builds and pagination](references/build-and-read.md) |
| A spelling or literal text | `localSearch` (or `rg`), not the graph |

Add `--runtime` (drops `import type`) or `--eager` (startup only) to any walk. [Saved queries](references/query.md) owns the flags and routes to each query's fields. Batch questions in one load, one per line: `printf '%s\n' 'search x' 'about a.ts' | octocode-graph query . --stdin --json`. An ambiguous name returns re-runnable candidates.

## Go further

| Need | Read |
|---|---|
| CI dependency rules: forbidden, allowed, required, reachable, circular; baseline, SARIF, `--since <rev>` | [Rules](references/rules.md) · [fields](references/rule-fields.md) · [CI](references/rules-ci.md) |
| Bundle bytes, CPU and heap profiles (`hot`), minified frames (`locate`), CodeQL or Joern data flow | [Providers](references/providers.md) · [bundles](references/provider-bundle.md) · [profiles, locate](references/provider-profiles.md) · [data flow](references/provider-data-flow.md) |
| What a metric means · quality, risk, retest and migration order | [Algorithms](references/algorithms.md) · [Quality checks](references/quality-checks.md) |
| What a change did to the structure | [Diff](references/diff.md) |
| Docs, configs and manifests linked to code (`links`); record a missed link | [Links](references/links.md) |
| Build options, replay · build phases, cache, registry · viewer server, events | [Commands](references/commands.md) · [Pipeline](references/build-pipeline.md) · [Viewer and events](references/viewer-events.md) |
| Saved files, coverage lines, HTML export | [Output](output.md) |
| Viewer chat · durable notes · architecture layers | [Chat](references/chat.md) · [Wiki](references/wiki.md) · [Modeling](references/modeling.md) |

Before stating a claim, `localFetch` the cited lines ([evidence](references/evidence.md)); run the `confirm:` call before claiming every caller. Report each answer with its `path:line` rows and the `brief.txt` coverage lines that limit it.

MCP: `octocode-graph mcp` serves `build_graph` → `query_graph` (`result`, `refresh`) → `refresh_graph` → `diff_graph`, `check_graph`, `link_graph`, beside the engine's `graphBuild` and `graphQuery`; `channel` adds the live viewer. In code: `buildGraph`, `openGraph(dir).query(text)`, `openFreshGraph(dir)`, `refreshGraph`, `diffFrom` from `@octocodeai/octocode-graph`.
