# Octocode Graph

Build, query and replay repository graphs with the bundled [graph skill](skills/octocode-graph/SKILL.md). The package uses the native engine supplied by its `octocode` dependency for graph construction, queries and language-server confirmation.

Install on Node 24.15 or later in the 24.x series. From the project to inspect, run:

```sh
npm install --save-dev @octocodeai/octocode-graph
npx octocode-graph-build . --out .octocode/first-run
npx octocode-graph-serve --replay .octocode/first-run
```

The package exports two native tools, `graphBuild` to create a snapshot and `graphQuery` to query it. Run them through the graph CLI from your project directory.

```sh
npx octocode-graph graphBuild '{"queries":[{"path":"."}]}' --json
npx octocode-graph graphQuery '{"queries":[{"path":".","operation":"stats"}]}' --json
npx octocode-graph schema graphQuery --view variants
```

To connect an MCP client, add a stdio server entry.

```json
{
  "mcpServers": {
    "octocode-graph": {
      "command": "npx",
      "args": ["-y", "@octocodeai/octocode-graph", "mcp"]
    }
  }
}
```

The graph server opts into native graph tools. Local access and tool filters still apply. In JavaScript, import `graphBuild`, `graphQuery`, or `startMcp` from `@octocodeai/octocode-graph`. Import `runGraphCli` from `@octocodeai/octocode-graph/cli` to delegate a CLI argument array and receive its exit code. Both tool functions accept the native input envelope and return the native response, including cursors. `@octocodeai/octocode-graph/skill` resolves to the bundled skill manifest.

Each new build needs a fresh result directory. Omit `--out` to allocate one automatically. Querying a saved result works offline.

| Command | Result |
| --- | --- |
| `octocode-graph-build <directory> [--out <result-dir>]` | Build a stitched graph, evidence and briefs. |
| `octocode-graph-query <result-dir> "deps src/main.ts"` | Query a saved graph or a live checkpoint. |
| `octocode-graph-serve <directory>` | Open an interactive graph. |
| `octocode-graph-serve --replay <result-dir>` | Reopen a saved run, including runs created by the former viewer skill. |

See the [viewer commands](skills/octocode-graph/references/commands.md) for scope, LSP options and progress.

Run the package tests with `yarn workspace @octocodeai/octocode-graph test`.
