# Octocode MCP server

`octocode-mcp` exposes Octocode research tools over the Model Context Protocol
using a stdio transport. It is a thin launcher for `octocode mcp`, the MCP
server in the native binary (`@octocodeai/octocode-native`), which owns the
protocol, tool contracts, execution, policy, and response shaping. A missing
platform binary is a startup error; there is no JavaScript execution fallback.
Without Node, run `octocode mcp` directly.

## Requirements

- Node.js `>=22` for this npm launcher
- An MCP client with stdio server support

The standalone native `octocode mcp` command does not require Node.js.

## Run

```bash
npx -y octocode-mcp
```

For guided installation into a supported client, use the CLI:

```bash
npx octocode install
```

The server registers the configured tool set in `tools/list`. Start with
[Configuration](../../docs/CONFIGURATION.md#tool-availability-cli-and-mcp)
for the default catalog, graph/clone/classification opt-ins, and tool filters.
The [MCP guide](../../docs/OCTOCODE_MCP.md) covers client setup and protocol
behavior. Credentials and provider readiness are execution requirements;
see [authentication](../../docs/AUTHENTICATION.md).

## Distribution

- `dist/index.js` is the stdio server binary.
- `dist/public.js` is the programmatic public entry.
- `manifest.json` describes the desktop-extension distribution. Its `tools`
  list is generated from the public catalog (`yarn sync:manifest`);
  `check:manifest` fails `lint` and `build` when it is stale.
- `server.json` describes the MCP registry package.

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
