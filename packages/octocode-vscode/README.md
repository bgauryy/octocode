# Octocode MCP for VS Code

`octocode-mcp-vscode` is the management extension for installing and operating
Octocode MCP from VS Code-compatible editors. It manages GitHub OAuth, reports
server status, and writes MCP configuration for supported clients.

The extension does not implement research tools. It installs and controls the
`octocode-mcp` server.

## Requirements

- VS Code 1.107 or later
- The extension declares Node.js `^24.15.0`. The npm MCP launcher it runs (`npx -y octocode-mcp@latest`) requires Node.js 22 or later.

## Commands

Open the command palette and search for `Octocode MCP`:

- Sign in to or sign out of GitHub.
- Show authentication or server status.
- Start or stop the MCP server.
- Install the MCP server for supported clients.
- Install for Cline, Roo Code, Trae, or every detected client.

| VS Code setting | Default | What it controls |
|---|---|---|
| `octocode.autoInstallMcp` | `true` | Automatically configures Octocode in supported editors. |
| `octocode.ideBridge.enabled` | `true` | Lets Octocode request language intelligence from this editor. |
| `octocode.githubToken` | Empty | Optional personal token in the editor's settings. |

Prefer the OAuth
sign-in command over storing a token in `octocode.githubToken`. The extension
writes the signed-in token as `GH_TOKEN` into the Octocode entry's `env` block of
each configured client (files are written owner-only), and removes it on sign-out.

## IDE bridge

While `octocode.ideBridge.enabled` is on (the default), the extension lets
Octocode's native language-intelligence tools (`lspSearch`) ask this editor's
own language providers for definitions, references, hover, symbols, call and
type hierarchy, and diagnostics, instead of starting a separate language
server. It listens on `127.0.0.1` only, requires a random bearer token, and
publishes the port and token in an owner-only file,
`~/.octocode/ide/<pid>.json` (`OCTOCODE_HOME` moves it), which it removes when
the editor stops the extension or you turn the setting off. It runs no
research tools and no repository code. Turn the setting off to disable it;
errors appear in the `Octocode MCP` output channel.

## Development

From the repository root:

```bash
yarn workspace octocode-mcp-vscode build
yarn workspace octocode-mcp-vscode test
yarn workspace octocode-mcp-vscode lint
yarn workspace octocode-mcp-vscode typecheck
```

Create a local VSIX with:

```bash
yarn workspace octocode-mcp-vscode package
```

See the repository [MCP guide](../../docs/OCTOCODE_MCP.md) and
[configuration reference](../../docs/CONFIGURATION.md).

## License

MIT
