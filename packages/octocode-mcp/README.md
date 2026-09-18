# Octocode MCP server

`octocode-mcp` exposes Octocode research tools over the Model Context Protocol
using a stdio transport. It is a thin adapter: tool contracts come from
`@octocodeai/octocode-core`, execution comes from
`@octocodeai/octocode-tools-core`, and native operations come from
`@octocodeai/octocode-engine`.

## Requirements

- Node.js 24.15.x
- An MCP client with stdio server support

## Run

```bash
npx octocode-mcp
```

For guided installation into a supported client, use the CLI:

```bash
npx octocode install
```

The server registers the configured subset of GitHub, package, local search,
AST, rewrite, file-fetch, and LSP tools. GitHub operations require an available
authentication method. Local and clone capabilities follow the shared Octocode
configuration and security policy.

## Distribution

- `dist/index.js` is the stdio server binary.
- `dist/public.js` is the programmatic public entry.
- `manifest.json` describes the desktop-extension distribution.
- `server.json` describes the MCP registry package.
- `README.md` is owned by this package and is never replaced during build or
  publishing.

## Development

From the repository root:

```bash
yarn workspace octocode-mcp build
yarn workspace octocode-mcp test
yarn workspace octocode-mcp lint
yarn workspace octocode-mcp typecheck
```

See [architecture](ARCHITECTURE.md), the full
[MCP guide](../../docs/OCTOCODE_MCP.md), and
[configuration reference](../../docs/CONFIGURATION.md).

## License

MIT
