# Code graph

Use `graphBuild` and `graphQuery` to find importers, trace dependencies or see which files a change might affect.

## Turn it on

The CLI includes both tools. For MCP, enable `local.beta` or set `OCTOCODE_BETA=true`, then restart the server. Snapshots go to `.octocode/graph` in the workspace. Set `local.graphHome` in your home configuration to store them elsewhere. [Configuration](CONFIGURATION.md#all-settings-and-defaults)

## Build

```bash
octocode graphBuild '{"queries":[{"path":"src"}]}'
```

Choose a directory that contains the files you want to connect. An unchanged directory reuses its snapshot. If a build pauses, follow `next.expandScan`.

## Query

```bash
octocode graphQuery '{"queries":[{"operation":"hubs","path":"src","kind":"file","rankBy":"pageRank"}]}'
octocode graphQuery '{"queries":[{"operation":"impact","path":"src","target":"src/config.ts"}]}'
```

Use the same `path` as the build. In the second example, replace `src/config.ts` with a file in your project. To pin a query to one snapshot, set `graph` to the build receipt's `graph` value.

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
