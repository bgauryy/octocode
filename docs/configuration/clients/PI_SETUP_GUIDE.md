# Using octocode-mcp with Pi

Pi (the coding agent from [earendil-works](https://github.com/earendil-works/pi)) has no built-in MCP support — this is an explicit non-goal. To run octocode-mcp inside Pi, install the community adapter **[`pi-mcp-adapter`](https://github.com/nicobailon/pi-mcp-adapter)** and point it at octocode-mcp.

The adapter exposes one ~200-token proxy tool (`mcp`) instead of injecting every MCP tool schema into the prompt — servers stay disconnected until the agent actually calls a tool.

---

## 1. Install the adapter

```bash
pi install npm:pi-mcp-adapter
```

Restart Pi after installation.

## 2. Add octocode-mcp to your MCP config

The adapter reads standard MCP files. Pick the scope that fits:

| File | Scope |
| --- | --- |
| `~/.config/mcp/mcp.json` | User-global, shared across MCP hosts (Cursor, Claude Code, Pi, …) |
| `.mcp.json` | Project-local, shared across MCP hosts |
| `~/.pi/agent/mcp.json` | Pi-only, user-global override |
| `.pi/mcp.json` | Pi-only, project override |

Precedence (highest first):

1. `~/.config/mcp/mcp.json`
2. `~/.pi/agent/mcp.json`
3. `.mcp.json`
4. `.pi/mcp.json`

Add the `octocode` entry under `mcpServers`:

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

> The `type: "stdio"` field used by some hosts (e.g. Cursor) is not required here — pi-mcp-adapter assumes stdio for `command` entries.

### Enabling local + clone tools

To unlock local FS / LSP / clone tools, pass env vars the same way you would for any other host:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"],
      "env": {
        "ENABLE_LOCAL": "true",
        "ENABLE_CLONE": "true"
      }
    }
  }
}
```

### Importing from an existing host

If you already have octocode configured for Cursor, Claude Code, Codex, etc., adopt those configs into Pi with the `/mcp setup` slash command **inside a Pi session**. The flow previews exactly what it will write before touching disk.

> `pi-mcp-adapter init` exists as a shell entrypoint (`~/.pi/agent/npm/node_modules/.bin/pi-mcp-adapter init`) but is interactive — it needs a TTY and produces no output in non-interactive shells. Prefer `/mcp setup` from inside Pi, or just write `~/.pi/agent/mcp.json` by hand as shown above.

## 3. Verify the server side (no Pi needed)

Before launching Pi, sanity-check that the `command` + `env` in your config actually produce a working MCP server. This handshake lists tools without going through the adapter:

```bash
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"verify","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | ENABLE_LOCAL=true ENABLE_CLONE=true npx -y octocode-mcp@latest 2>/dev/null \
  | grep -o '"name":"[^"]*"' | sort -u
```

Expected output includes the 13 Octocode tools below — this confirms `ENABLE_LOCAL` / `ENABLE_CLONE` are taking effect:

```
"name":"ghCloneRepo"
"name":"ghGetFileContent"
"name":"ghSearchCode"
"name":"ghSearchPRs"
"name":"ghSearchRepos"
"name":"ghViewRepoStructure"
"name":"localFindFiles"
"name":"localGetFileContent"
"name":"localSearchCode"
"name":"localViewStructure"
"name":"lspGetSemantics"
"name":"npmSearch"
```

If `ghCloneRepo` or local/LSP tools are missing, the env block in `mcp.json` is not being read — re-check JSON syntax.

## 4. Use octocode tools from inside Pi

Start the agent and discover tools through the proxy:

```
mcp({ search: "github" })
```

The adapter returns matching tool descriptions. Call one with a JSON-string `args` (not an object):

```
mcp({ tool: "octocode_ghSearchCode", args: "{\"queries\":[{\"id\":\"q1\",\"keywordsToSearch\":[\"useState\"]}]}" })
```

Servers are **lazy by default** — `octocode-mcp` only spawns when you first invoke one of its tools.

## 5. Verify inside Pi

```
/mcp
```

Pi lists detected MCP files and registered servers. You should see `octocode` listed under `~/.pi/agent/mcp.json` (or whichever scope you chose). The first tool call lazily spawns `npx octocode-mcp@latest` and streams its tool list into the adapter's cache — expect a one-time delay on the first `mcp({ tool: ... })` call.

To confirm the adapter is registered in Pi at all (without launching the TUI):

```bash
cat ~/.pi/agent/settings.json   # should contain "npm:pi-mcp-adapter" under "packages"
ls ~/.pi/agent/npm/node_modules/.bin/pi-mcp-adapter   # binary exists
```

---

## Alternative: skip MCP entirely

Pi's stated philosophy is "CLI tools with READMEs (Skills) over MCP." If you don't need every octocode tool exposed, install the octocode researcher skill instead:

```bash
npx skills add https://github.com/bgauryy/octocode-mcp --skill octocode-researcher
```

The skill drives octocode via the CLI and avoids the MCP transport layer entirely. Use the adapter route when you want the full 13-tool surface; use the skill route when a focused research workflow is enough.

## References

- pi-mcp-adapter — https://github.com/nicobailon/pi-mcp-adapter
- Pi (coding agent) — https://github.com/earendil-works/pi
- octocode-mcp tool reference — [GitHub Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md), [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md), [LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md)
