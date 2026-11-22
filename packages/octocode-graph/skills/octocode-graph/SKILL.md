---
name: octocode-graph
description: "Query code relationships or explore a repository map. Find callers, imports, dependency paths, cycles and change impact, or build, serve and replay a graph with source evidence."
---

# Octocode Graph

Use the `octocode-graph` MCP server or the graph CLI. Run `octocode-graph schema` to list the native tools. The separate `octocode` engine supplies the local research tools used for source checks.

| Need | Entry point |
| --- | --- |
| Find who imports a file, how two modules connect, or what a change affects | Send a scoped `graphQuery`. With no graph ID it selects the newest snapshot for that path. [Native operations](references/operations.md) |
| Create or update a snapshot | Use `graphBuild` for a missing, stale or insufficient scope. Pin the returned ID for historical comparisons. [Snapshots and rows](references/build-and-read.md) |
| Identify the exact callee or caller | Use `lspSearch` at the declaration or call site. For a syntax pattern alone, use `astSearch`. |
| Explore a repository interactively | `node <skill>/scripts/serve.mjs <directory>` prints a local viewer URL. [Viewer commands](references/commands.md) |
| Save a stitched graph and brief | `node <skill>/scripts/build.mjs <directory>`. The run publishes `stitched.json` before LSP confirmation finishes, then writes `data.json`. |
| Use an existing export | `node <skill>/scripts/query.mjs <result-dir> "deps src/main.ts"` uses the [saved-graph query language](references/query.md). Reopen the viewer with `node <skill>/scripts/serve.mjs --replay <result-dir>`. |

Choose a scope containing the relevant files and consumers. A returned call site can establish a relationship even when another location was withheld. For an absence or deletion claim, check freshness, unresolved sites and consumers beyond that scope. Follow continuations when the answer needs all rows. [Evidence](references/evidence.md)

Viewer [chat and refresh](references/chat.md), [wiki notes](references/wiki.md) and [architecture overlays](references/modeling.md) are available when the task calls for them. [Saved artifacts](output.md)
