# Code graph

Use `graphBuild` and `graphQuery` to find importers, trace dependencies or see which files a change might affect.

## Turn it on

The CLI includes both tools. For MCP, run `octocode config set local.beta true` with local tools enabled, then restart MCP. Builds save snapshots under the graph workspace’s `.octocode/graph/` by default. [Change the snapshot location](CONFIGURATION.md#all-settings-and-defaults)

## Build

```bash
octocode graphBuild '{"queries":[{"path":"src"}]}'
```

Replace `src` with a directory in your project. If the build pauses, follow `next.expandScan`. Keep the returned `graph` value to query that exact snapshot.

## Query

```bash
octocode graphQuery '{"queries":[{"operation":"hubs","path":"src","kind":"file","rankBy":"pageRank"}]}'
octocode graphQuery '{"queries":[{"operation":"impact","path":"src","target":"src/config.ts"}]}'
```

Use the build’s `path` in both queries. Replace `src/config.ts` with a file from the snapshot. You can also pass the saved `graph` value.

| Question | Operation |
|---|---|
| Which files depend on this file? | `dependents` |
| Which files does this file import? | `deps` |
| Which files are most connected? | `hubs` |
| Where are the import cycles? | `cycles` |
| Which files or tests could this change affect? | `impact` |
| Has the source changed since the build? | `stale` |

A file's dependents can include code that uses its exports through another module. Set `edges: ["imports"]` to follow imports only. Run `octocode schema graphQuery --view variants` for more operations and examples.

## Read the answer

`sites` counts source occurrences behind a connection. `unresolved` lists sites the graph could not link. Impact results can lead you to tests and entrypoints worth checking. For a caller or unused-code finding, open the source location and check symbol references with `lspSearch`. [Local code tools](OCTOCODE_TOOLS.md#local-code-tools-reference)
