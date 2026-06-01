# Octocode CLI

<div align="center">

[![npm version](https://img.shields.io/npm/v/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![npm downloads](https://img.shields.io/npm/dm/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/LICENSE)

**Two things in one binary:** manage Octocode MCP across your IDEs, and run any Octocode tool directly from the terminal.

[Website](https://octocode.ai) · [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) · [GitHub](https://github.com/bgauryy/octocode-mcp)

<img src="https://raw.githubusercontent.com/bgauryy/octocode-mcp/main/packages/octocode-cli/assets/example.png" alt="Octocode CLI Demo" width="700" style="border-radius: 8px; margin: 20px 0;">

</div>

---

## Quick Start

```bash
npx octocode-cli install           # Interactive setup wizard
octocode-cli install --ide cursor  # Install for a specific IDE
octocode-cli install --ide cursor --check   # Pre-flight: check writability & current state
octocode-cli auth                  # GitHub authentication
```

---

## Run Tools

Call any of the 14 Octocode tools directly from the terminal — great for scripts, pipelines, and one-off queries.

```bash
# Discover tools
octocode-cli tools                    # list all tools with descriptions
octocode-cli tools localSearchCode    # show schema for a specific tool

# Run a tool
octocode-cli tools localSearchCode --queries '{"path":".","pattern":"TODO"}'
octocode-cli tools githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'

# Machine-readable output
octocode-cli tools localSearchCode --queries '{"path":".","pattern":"TODO"}' --json

# Full MCP instructions + all schemas (for agents)
octocode-cli instructions
```

Shared fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled — only provide tool-specific fields.

---

## Manage Octocode

| Command | What it does |
|---------|--------------|
| `install` | Configure octocode-mcp for an IDE |
| `install --check` | Pre-flight: verify config path is writable, show what would change |
| `install --rollback` | Restore the most recent backup configuration |
| `auth` / `login` / `logout` | GitHub authentication |
| `login --force` | Log out the current session and re-authenticate in one step |
| `logout --yes` | Skip the confirmation prompt |
| `auth refresh` | Refresh an Octocode-managed token (source-aware) |
| `token` | Print the resolved GitHub token |
| `token --validate` | Ping the GitHub API to verify the token and show rate-limit |
| `status` | Full health check: auth + MCP clients + cache |
| `status --sync` | Also includes per-MCP sync analysis |
| `sync` | Sync MCP configs across all IDEs |
| `sync plan` | Show what `sync` would do without writing anything |
| `sync --dry-run` | Same as `sync plan` |
| `skills` | Install / remove / list / search bundled skills |
| `skills search --direct` | Search skills.sh directly (human-readable) |
| `skills sync <from> <to>` | Copy skills between targets |
| `mcp` | Browse and manage the MCP marketplace |
| `mcp install --id a,b,c` | Batch-install MCPs (parallel preflight) |
| `cache` | Inspect and clean Octocode cache |
| `cache clean --dry-run` | Show what would be freed without deleting |

---

## Supported Clients

`cursor`, `claude` / `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

---

## Troubleshooting

```bash
octocode-cli status --sync          # Full health check including sync analysis
octocode-cli token --validate       # Verify your GitHub token against the API
octocode-cli token --source         # Debug token resolution chain
octocode-cli install --ide cursor --check   # Pre-flight before installing
octocode-cli sync plan              # Preview sync changes before applying
octocode-cli cache clean --all --dry-run    # See what cache clean would free
```

---

## Docs

- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) — full command and flag reference
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) — bundled skills installation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md)
- [Agents / automation guide](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/AGENTS.md) — non-interactive usage, `--json` flags, tool execution

---

## Privacy & Telemetry

De-identified telemetry (command usage, error rates) helps improve the CLI. Source code, env values, and repo contents are not collected.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) · [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

MIT. Copyright 2026 Octocode AI.
