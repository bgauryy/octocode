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

## Install

```bash
# Homebrew (macOS / Linux) — installs the `octocode` command
brew install bgauryy/octocode/octocode

# or npm (global)
npm install -g octocode-cli

# or run once without installing
npx octocode-cli install
```

The installed command is **`octocode`** (e.g. `octocode --help`).

## Quick Start

```bash
npx octocode-cli install           # Interactive setup wizard
octocode install --ide cursor  # Install for a specific IDE
octocode install --ide cursor --check   # Pre-flight: check writability & current state
octocode auth                  # GitHub authentication
```

---

## GitHub Token

Octocode resolves a GitHub token in this order: **environment variable → Octocode-managed token (`auth login`) → `gh` CLI**.

For env-based setup, any of these are accepted (checked in this priority): `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`.

```bash
export GITHUB_TOKEN=ghp_xxx        # or OCTOCODE_TOKEN / GH_TOKEN
octocode token --source        # show which token & source is being used
octocode token --validate      # ping the GitHub API to verify it
```

---

## Run Tools

Call any of the 14 Octocode tools directly from the terminal — great for scripts, pipelines, and one-off queries.

```bash
# Discover tools
octocode tools                    # list all tools with descriptions
octocode tools localSearchCode    # show schema for a specific tool
octocode tools localSearchCode githubSearchCode   # batch schemas

# Run a tool
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}'
octocode tools githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'

# Machine-readable output
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}' --json

# Full MCP instructions + all schemas (for agents)
octocode instructions
```

Shared fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled — only provide tool-specific fields.

---

## Manage Octocode

| Command | What it does |
|---------|--------------|
| `install --ide <ide>` | Configure octocode-mcp for an IDE |
| `install --ide <ide> --check` | Pre-flight: verify config path is writable, show what would change |
| `install --ide <ide> --rollback` | Restore the most recent backup configuration |
| `install --ide <ide> --rollback --backup-path <file>` | Restore a specific backup file |
| `auth` / `login` / `logout` | GitHub authentication |
| `login --force` | Log out the current session and re-authenticate in one step |
| `login --git-protocol <ssh\|https>` | Set the git protocol used for clones |
| `logout --yes` | Skip the confirmation prompt |
| `auth refresh` | Refresh an Octocode-managed token (source-aware) |
| `token` | Print the resolved GitHub token |
| `token --type <auto\|octocode\|gh>` | Force a specific token source instead of auto-resolution |
| `token --source` | Show which source resolved the token |
| `token --validate` | Ping the GitHub API to verify the token and show rate-limit |
| `status` | Full health check: auth + MCP clients + cache |
| `status --sync` | Also includes per-MCP sync analysis |
| `sync` | Sync MCP configs across all IDEs |
| `sync plan` / `sync --dry-run` | Show what `sync` would do without writing anything |
| `sync --status` | Show sync analysis without syncing |
| `sync --force` | Auto-resolve conflicts (use first variant found) |
| `skills` | Install / remove / list / search / read bundled skills |
| `skills search --direct` | Search skills.sh directly (human-readable) |
| `skills search --direct --install` | Fetch and install top result automatically |
| `skills read <path\|github:owner/repo/name>` | Preview a SKILL.md from disk or GitHub |
| `skills install --targets <t1,t2>` | Install all bundled skills to targets |
| `skills install --skill <name> --targets <t>` | Install one specific skill |
| `skills install --local <path> --targets <t>` | Install a local skill folder |
| `skills install --dry-run` | Preview installs without writing |
| `skills install --mode symlink` | Symlink instead of copy |
| `skills remove --skill <name> --targets <t>` | Remove a skill |
| `skills list` | List all installed skills across all targets |
| `skills list --target <t>` | Filter to one target |
| `skills sync <from> <to>` | Copy skills between targets |
| `mcp list` | Scan all OS MCP config files |
| `mcp list --client <id>` | Search registry for a specific client |
| `mcp list --installed` | List only installed MCPs with env-var status |
| `mcp list --search <text>` | Filter by id / name / description / tags |
| `mcp list --category <name>` | Filter by category |
| `mcp status --client <id>` | Show servers in one config + env var status |
| `mcp install --id <id> --client <id>` | Install one MCP server |
| `mcp install --id a,b,c` | Batch-install MCPs (parallel preflight) |
| `mcp install --id <id> --config <path>` | Install into a custom config file |
| `mcp install --id <id> --env K=V,K2=V2` | Set env vars for the installed server |
| `mcp install --id <id> --force` | Overwrite existing entry |
| `mcp remove --id <id> --client <id>` | Remove an MCP server |
| `cache status` | Inspect cache sizes (repos, skills, logs) |
| `cache clean --all` | Clean repos + skills + logs |
| `cache clean --repos` / `--skills` / `--logs` | Clean specific targets |
| `cache clean --tools` | Clean all tool caches (local + lsp + api) |
| `cache clean --local` / `--lsp` / `--api` | Clean individual tool caches |
| `cache clean --all --dry-run` | Show what would be freed without deleting |
| `cache clean --all --yes` | Skip confirmation prompt |

---

## Supported Clients

`cursor`, `claude` / `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

---

## Troubleshooting

```bash
octocode status --sync          # Full health check including sync analysis
octocode token --validate       # Verify your GitHub token against the API
octocode token --source         # Debug token resolution chain
octocode install --ide cursor --check   # Pre-flight before installing
octocode sync plan              # Preview sync changes before applying
octocode cache clean --all --dry-run    # See what cache clean would free
```

---

## Docs

- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) — full command and flag reference
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) — bundled skills installation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md)
---

## Privacy & Telemetry

De-identified telemetry (command usage, error rates) helps improve the CLI. Source code, env values, and repo contents are not collected.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) · [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

MIT. Copyright 2026 Octocode AI.
