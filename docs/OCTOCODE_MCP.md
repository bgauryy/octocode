# Octocode MCP server

Connect an agent that supports stdio MCP to the same research tools as the [Octocode CLI](OCTOCODE_CLI.md). The npm launcher `octocode-mcp` requires Node.js 22 or later. With the standalone binary, configure the command as `octocode` with argument `mcp`.

## Quick start

1. From the project directory, run `npx -y octocode install --list` to see supported clients and config paths.
2. Run `npx -y octocode install --ide cursor` (replace `cursor` with your client ID). Add `--dry-run` to preview the entry or `--scope workspace` for a supported project-level install.
3. Restart the MCP client. In a project with a README, ask **“Use Octocode to read the first line of README.md.”** Confirm that the agent calls `localFetch` and returns that line.

For a client that uses an `mcpServers` JSON object, this is the manual entry. Set `WORKSPACE_ROOT` in the entry's `env` if the client launches servers outside your project.

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

## Tool catalog

The contract has 17 tools. MCP lists 12 by default. Graph tools, cloning, advisory checks and classification have separate opt-ins. Use your client's tool list to see which tools this server actually exposes. A listed GitHub tool may still need an account with access to the requested repository. [Availability and opt-ins](CONFIGURATION.md#tool-availability-cli-and-mcp) · [GitHub sign-in](AUTHENTICATION.md)

| MCP operation | What the client gets |
|---|---|
| `tools/list` | Enabled tool names, descriptions and input schemas. |
| `tools/call` | One text content block containing the JSON result. Check each indexed row for errors, empty results or remaining evidence. |
| `resources/list` and `resources/read` | Research guides such as `octocode://research/tool-flow`, and output-field definitions at `octocode://contract/output/<tool>` for listed tools. |

### Example: a localFetch call

Call `localFetch` with these **arguments**. The CLI accepts the identical JSON after the tool name.

```json
{"queries":[{"path":"README.md","ranges":[{"startLine":1,"endLine":1}]}]}
```

Pass an emitted continuation's complete `input` to the tool it names. An MCP session can retain cursors in memory for its lifetime. [Reading results and following continuations](TOOL_DATA_CONTRACT.md)

## Startup and troubleshooting

| Symptom | Check |
|---|---|
| Client cannot start the server | Verify that the client can run `npx` and that npm installed the native package for its platform. Startup errors appear in the client's MCP log. |
| Client connects but a tool is missing | Inspect `tools/list`, the tool's opt-in and any include or disable lists. An allowlist containing only unrecognized names can leave the server with no tools and cause startup to fail. |
| The agent reads a different project | Inspect the client's launch directory and `WORKSPACE_ROOT`. Relative paths use the effective workspace root. |
| A key or setting changed | Restart the server to reload the process environment and configuration files. Stored GitHub logins and GitHub CLI accounts are resolved for requests. |
| Some batch rows failed | Inspect each row's `status` and error. `isError` describes a failed call or a batch in which every row failed. A mixed batch can contain both evidence and errors. |

The native server supports the `initialize` handshake and the 2026-07-28 `server/discover` handshake on the same stdio command. The client manages the process and closes it when the session ends. [Configuration](CONFIGURATION.md) · [Security](SECURITY.md)
