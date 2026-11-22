# Octocode MCP server

Connect an agent that supports stdio MCP to the same research tools as the [Octocode CLI](OCTOCODE_CLI.md). The `octocode-mcp` npm launcher needs Node.js 22 or later. With the standalone binary, set the command to `octocode` and the argument to `mcp`.

## Quick start

1. Run `npx -y octocode install --list` to see supported clients and their config paths.
2. Run `npx -y octocode install --ide cursor`, replacing `cursor` with your client ID. Add `--dry-run` to preview the entry, or `--scope workspace` for a project-level config.
3. Restart the client. In a project with a README, ask **“Use Octocode to read the first line of README.md.”** The agent should call `localFetch` and return that line.

For a client that takes an `mcpServers` JSON object, add this entry by hand. Set `WORKSPACE_ROOT` in `env` if the client starts servers outside your project.

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"]
    }
  }
}
```

## Tools and opt-ins

The contract has 17 tools. The MCP server starts with 12. Graph tools, cloning, `securityCheck` and classification need an opt-in. Your client's tool list shows what this server exposes. A listed GitHub tool still needs an account with access to the repository.

[Availability and opt-ins](CONFIGURATION.md#tool-availability-cli-and-mcp) · [GitHub sign-in](AUTHENTICATION.md)


The default result is JSON. To read more of a paged answer, pass the returned continuation's `input` to the tool it names. [Read tool results](TOOL_DATA_CONTRACT.md)

If the same workspace appears twice in your client, check whether both a plugin and a manual entry connect it. Keep one entry for that workspace and configuration.

### Select the tools for a workflow

Set `TOOLS_TO_RUN` to a comma-separated list in the existing server entry's `env`. This example keeps the three local reading tools.

```json
{
  "env": {
    "TOOLS_TO_RUN": "structureSearch,localSearch,localFetch"
  }
}
```

Keep the tools needed for follow-up reads as part of the list. For a project-level allowlist, set `tools.enabled` in `.octocode/.octocoderc` under the server's working directory. A tool still needs its opt-in, and `tools.disabled` takes precedence. Restart the MCP server after a change and check the new `tools/list`. [Configuration and opt-ins](CONFIGURATION.md#tool-availability-cli-and-mcp)

### Example: a localFetch call

Pass these as the tool **arguments**. The CLI accepts the same JSON after the tool name.

```json
{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}
```

## Troubleshooting

| Symptom | Check |
|---|---|
| The client cannot start the server | Check that the client can run `npx` and that the native package for your platform installed. Startup errors appear in the client's MCP log. |
| A tool is missing | Check `tools/list`, the tool's opt-in, `tools.enabled` and `tools.disabled`. [Availability](CONFIGURATION.md#tool-availability-cli-and-mcp) |
| The agent reads a different project | Check the client's launch directory and `WORKSPACE_ROOT`. Relative paths resolve against the workspace root. |
| A setting or key changed but has no effect | Restart the MCP server. |

[Configuration](CONFIGURATION.md) · [Security](SECURITY.md)
