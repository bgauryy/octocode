---
name: octocode-graph
description: "Query code relationships or explore a repository map. Find callers, imports, dependency paths, cycles and change impact, or build, serve and replay a graph with source evidence."
---

# Octocode Graph

For a relationship question, query the smallest scope that contains both endpoints and the consumers you care about. For example, to see what a file imports in the current project:

```sh
octocode-graph graphQuery '{"queries":[{"path":".","operation":"deps","target":"src/main.ts"}]}'
```

The graph MCP server accepts the same `graphQuery` and `graphBuild` inputs as the CLI. With no graph ID, the query uses the newest snapshot for that path. Build when the snapshot is missing or the question needs fresher or wider coverage. Pin the returned ID for comparisons. [Native operations](references/operations.md) · [Builds and pagination](references/build-and-read.md)

| If you have or need | Use |
|---|---|
| Source text from one or several known files | Batch paths and line ranges in `localFetch`. For uncertain filenames in a saved graph, `files <file...>` resolves locations and returns source queries. |
| A saved export | `octocode-graph query <result-dir> "deps src/main.ts"` works offline. [Saved queries](references/query.md) |
| Graph files and a brief | `octocode-graph build <file-or-directory>`. Use a directory for cross-file relationships. [Output files and HTML](output.md) |
| An interactive map | `octocode-graph serve <file-or-directory>` streams the build. Use `--replay <result-dir>` to reopen an export. [Viewer commands](references/commands.md) |
| The exact target of an ambiguous call | Use `lspSearch` at the call site or inspect the deciding source span. For a syntax pattern, use `astSearch`. [Evidence](references/evidence.md) |

Install `@octocodeai/octocode-graph` in the workspace for the CLI. For MCP, `octocode-graph mcp` exposes native snapshots. Use `octocode-graph channel` for saved exports. Its `query_graph` accepts `{"result":"/path/to/saved-run","query":"impact src/main.ts src/lib.ts depth 2"}` without opening a browser.

For impact, cycles, rankings, migration order or runtime evidence, choose the [flow that matches the question](references/analysis-flows.md). Use [chat](references/chat.md) for viewer questions, [community notes](references/wiki.md) for durable findings, or an [architecture overlay](references/modeling.md) to annotate the brief.
