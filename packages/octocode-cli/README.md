# Octocode CLI

<div align="center">

[![npm version](https://img.shields.io/npm/v/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![npm downloads](https://img.shields.io/npm/dm/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/LICENSE)

**Two things in one binary:**

1. **Manage** — install, authenticate, sync, and configure Octocode MCP, skills, and marketplace MCPs across IDEs.
2. **Run tools** — call any Octocode tool directly from the terminal, for agents and humans alike.

[Website](https://octocode.ai) | [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) | [GitHub](https://github.com/bgauryy/octocode-mcp)

<img src="https://raw.githubusercontent.com/bgauryy/octocode-mcp/main/packages/octocode-cli/assets/example.png" alt="Octocode CLI Demo" width="700" style="border-radius: 8px; margin: 20px 0;">

</div>

## Quick Start

```bash
npx octocode-cli install           # Interactive setup
octocode-cli install --ide cursor  # Non-interactive install
octocode-cli auth                  # GitHub authentication
```

## Run Tools

Call any Octocode tool directly — works for agents, scripts, and humans. Shared fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled.

```bash
octocode-cli --tool localSearchCode --queries '{"path":".","pattern":"runCLI"}'
octocode-cli --tool githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
octocode-cli --tool githubSearchCode --help       # Input/output schema
octocode-cli --tools-context                      # Full MCP instructions + all schemas
```

## Manage Octocode

| Command | What it does |
|---------|--------------|
| `install` | Configure octocode-mcp for an IDE |
| `auth` / `login` / `logout` | GitHub authentication |
| `token` / `status` | Token and auth info |
| `sync` | Sync MCP configs across IDEs |
| `skills` | Install/remove bundled skills |
| `mcp` | Manage MCP marketplace |
| `cache` | Inspect/clean cache and logs |

## Supported Clients

`cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`.

## Docs

- [Docs Index](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md) — all Octocode docs
- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) — full command and tool syntax
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) — bundled skills installation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — MCP env/config options the CLI writes
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) — install, Node.js, auth, and MCP startup issues

## Troubleshooting

```bash
octocode-cli status          # Check auth
octocode-cli token --source  # Debug token resolution
octocode-cli sync --status   # Check config consistency
```

## Privacy & Telemetry

De-identified telemetry (command usage, error rates) helps improve the CLI. Source code, env values, and repo contents are not collected.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) | [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

MIT. Copyright 2026 Octocode AI.
