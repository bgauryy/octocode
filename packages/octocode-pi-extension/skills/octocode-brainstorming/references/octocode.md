# Octocode MCP and CLI

Load this when a task needs Octocode setup, transport choice, authentication, or command examples.

## Choose Transport

Use Octocode MCP tools directly when the host exposes them, such as `localSearchCode`, `ghSearchCode`, `ghGetFileContent`, `npmSearch`, `lspGetSemantics`, or `localBinaryInspect`. Read the tool description and input schema before calling.

When MCP tools are not exposed, prefer the CLI with `npx octocode`. Read live help before relying on flags, and read `npx octocode tools <name> --scheme` before raw tool calls.

## MCP Install

Configure the MCP server as:

```json
"octocode": {
  "command": "npx",
  "type": "stdio",
  "args": [
    "@octocodeai/mcp@latest"
  ]
}
```

Restart the host/editor after changing MCP configuration.

## CLI Usage

Run commands as `npx octocode <command>`.

Useful probes:

```bash
npx octocode --help
npx octocode auth status --json
npx octocode tools
npx octocode tools <name> --scheme
```

Use `npx octocode auth login` when GitHub or private data requires authentication. If neither MCP nor CLI is available, continue only with clearly degraded confidence or ask the user to enable one.
