# Octocode MCP server

`octocode-mcp` exposes Octocode research tools over the Model Context Protocol
using a stdio transport. It is a thin adapter: tool contracts come from
`@octocodeai/octocode-core`, while `@octocodeai/octocode-native` owns tool
execution, policy, and response shaping. A missing native runtime is a startup
error; there is no JavaScript execution fallback.

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
AST, file-fetch, and LSP tools. GitHub operations require an available
authentication method. Local capabilities follow the shared Octocode
configuration and security policy.

## Distribution

- `dist/index.js` is the stdio server binary.
- `dist/public.js` is the programmatic public entry.
- `manifest.json` describes the desktop-extension distribution. Its `tools`
  list is generated from the public catalog (`yarn sync:manifest`);
  `check:manifest` fails `lint` and `build` when it is stale.
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
