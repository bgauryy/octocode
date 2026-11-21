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

| MCP operation | What the client gets |
|---|---|
| `initialize` | Instructions for the enabled tools; with `output.mcpResult: text` they end with a block that says how to read text replies. |
| `tools/list` | Enabled tool names, descriptions and input schemas. |
| `tools/call` | One text block with the JSON result (default), or with the text view when [`output.mcpResult`](CONFIGURATION.md) is `text`: content lines, `# key: value` fields, and each next call as `{"tool","input"}` JSON on its `# next.*` line. Check each row for errors, empty results and remaining evidence. |
| `resources/list`, `resources/read` | Research guides such as `octocode://research/tool-flow`, and output-field definitions at `octocode://contract/output/<tool>`. |

### Example: a localFetch call

Pass these as the tool **arguments**. The CLI accepts the same JSON after the tool name.

```json
{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}
```

To continue a paged result, pass the emitted continuation's `input` to the tool it names. [Reading results and continuations](TOOL_DATA_CONTRACT.md)

## Troubleshooting

| Symptom | Check |
|---|---|
| The client cannot start the server | Check that the client can run `npx` and that the native package for your platform installed. Startup errors appear in the client's MCP log. |
| A tool is missing | Check `tools/list`, the tool's opt-in, `tools.enabled` and `tools.disabled`. [Availability](CONFIGURATION.md#tool-availability-cli-and-mcp) |
| The agent reads a different project | Check the client's launch directory and `WORKSPACE_ROOT`. Relative paths resolve against the workspace root. |
| A setting or key changed but has no effect | Restart the MCP server. |
| Some batch rows failed | Read each row's `status` and error. `isError` is set when the call failed or every row failed. A mixed batch can hold both evidence and errors. |

The server accepts the `initialize` handshake and the `server/discover` handshake on the same stdio command. The client starts and stops the process. [Configuration](CONFIGURATION.md) · [Security](SECURITY.md)
