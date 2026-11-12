# Octocode MCP server

Connect an agent to Octocode's local, GitHub and package research tools over stdio. This package launches the native `octocode mcp` command and requires Node.js 22 or later. The standalone native binary can serve MCP directly.

## Connect a client

Run `npx -y octocode install` in a terminal to choose a client. For a manual entry, configure your client to launch:

```sh
npx -y octocode-mcp
```

Restart the client. In a project with a README, ask your agent to read the first line of `README.md` with `localFetch`.

| Task | Where to look |
|---|---|
| Set the project directory or troubleshoot startup | [MCP setup guide](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md) |
| Inspect the available tools and workflows | Your client's `tools/list` and the [tool reference](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) |
| Enable graph, clone, advisory or classification tools | [Configuration defaults and opt-ins](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#tool-availability-cli-and-mcp) |
| Sign in to GitHub or use an existing token | [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) |

## Development

The package ships `dist/index.js` as its stdio launcher. `manifest.json` describes the desktop extension and `server.json` describes the MCP registry package. Run `yarn workspace octocode-mcp build` and `yarn workspace octocode-mcp test` from the repository root. [Launcher architecture](https://github.com/bgauryy/octocode/blob/main/packages/octocode-mcp/ARCHITECTURE.md)

## License

MIT
