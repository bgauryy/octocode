# Octocode CLI

<div align="center">

[![npm version](https://img.shields.io/npm/v/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![npm downloads](https://img.shields.io/npm/dm/octocode-cli.svg?style=flat-square)](https://www.npmjs.com/package/octocode-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/LICENSE)

**Two things in one binary:** manage Octocode MCP across your IDEs, and run any Octocode tool directly from the terminal.

[Website](https://octocode.ai) · [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md) · [GitHub](https://github.com/bgauryy/octocode-mcp)

  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-mcp/assets/logo_white.png" width="400px" alt="Octocode Logo">


</div>

---

## Install

```bash
# Homebrew (macOS / Linux): installs the `octocode` command globally
brew install bgauryy/octocode/octocode

# equivalent short form
brew tap bgauryy/octocode && brew install octocode

# or npm (global)
npm install -g octocode-cli

# or run once without installing
npx octocode-cli install
```

Verify, then sign in:

```bash
octocode --version        # → octocode v1.5.0
octocode login            # GitHub OAuth (device flow)
```

The installed command is **`octocode`** (not `octocode-cli`). Homebrew pulls in Node automatically.

## Quick Start

```bash
octocode login                          # GitHub authentication
octocode install --ide cursor           # wire MCP into an editor
octocode install --ide cursor --check   # pre-flight: check writability & current state
octocode skills install --targets claude-code   # add Agent Skills
octocode tools                          # list every tool
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}'
```

---

## The 14 Tools

Call any tool directly from the terminal. Great for scripts, pipelines, and one-off queries. No MCP server required.

| Group | Tool | What it does |
|-------|------|--------------|
| **GitHub** | `githubSearchCode` | Search code across GitHub `[EXTERNAL]` |
| | `githubSearchRepositories` | Search repositories by keywords/topics `[EXTERNAL]` |
| | `githubSearchPullRequests` | Search pull requests `[EXTERNAL]` |
| | `githubGetFileContent` | Read file content (matchString, line ranges) `[EXTERNAL]` |
| | `githubViewRepoStructure` | List a repo's directory tree `[EXTERNAL]` |
| | `githubCloneRepo` | Clone a repo/subtree to disk for local + LSP analysis |
| **Local** | `localSearchCode` | Search code patterns with ripgrep (PCRE2) |
| | `localFindFiles` | Find files by name/metadata |
| | `localGetFileContent` | Read local file content |
| | `localViewStructure` | View a local directory tree |
| **LSP** | `lspGotoDefinition` | Navigate to a symbol's definition |
| | `lspFindReferences` | Find all usages of a symbol |
| | `lspCallHierarchy` | Trace function call relationships |
| **Package** | `packageSearch` | Resolve npm / PyPI packages to their source repo |

```bash
# Discover
octocode tools                                    # list all tools
octocode tools localSearchCode                    # show one tool's schema
octocode tools localSearchCode githubSearchCode   # batch schemas
octocode instructions                             # full MCP instructions + all schemas

# Run
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}'
octocode tools githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
octocode tools localSearchCode --queries '{"path":".","pattern":"TODO"}' --json   # machine-readable
```

The shared metadata fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled. Provide only tool-specific fields.

---

## GitHub Token

Octocode resolves a GitHub token in this order: **environment variable → Octocode-managed token (`octocode login`) → `gh` CLI**.

For env-based setup, any of these are accepted (checked in this priority): `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`.

```bash
export GITHUB_TOKEN=ghp_xxx        # or OCTOCODE_TOKEN / GH_TOKEN
octocode token --source           # show which token & source is being used
octocode token --validate         # ping the GitHub API to verify it
```

---

## Commands

| Command | Aliases | What it does |
|---------|---------|--------------|
| `install --ide <ide>` | `i`, `setup` | Configure octocode-mcp for an IDE |
| `install --ide <ide> --check` | | Pre-flight: verify config path is writable, show what would change |
| `install --ide <ide> -m <npx\|direct>` | | Choose install method (default `npx`; `direct` points at a local binary) |
| `install --ide <ide> --force` | | Overwrite an existing configuration |
| `install --ide <ide> --rollback` | | Restore the most recent backup configuration |
| `install --ide <ide> --rollback --backup-path <file>` | | Restore a specific backup file |
| `auth` / `login` / `logout` | `a`, `gh` / `l` / | GitHub authentication |
| `login --force` | | Log out the current session and re-authenticate in one step |
| `login --git-protocol <ssh\|https>` | | Set the git protocol used for clones |
| `login --hostname <host>` | | Target a GitHub Enterprise host |
| `logout --yes` | | Skip the confirmation prompt |
| `auth refresh` | | Refresh an Octocode-managed token (source-aware) |
| `token` | `t` | Print the resolved GitHub token |
| `token --type <auto\|octocode\|gh>` | | Force a specific token source instead of auto-resolution |
| `token --source` | | Show which source resolved the token |
| `token --validate` | | Ping the GitHub API to verify the token and show rate-limit |
| `status` | `s` | Full health check: auth + MCP clients + cache |
| `status --sync` | | Also include per-MCP sync analysis |
| `sync` | `sy` | Sync MCP configs across all IDEs |
| `sync --dry-run` | | Show what `sync` would do without writing anything |
| `sync --status` | | Show sync analysis without syncing |
| `sync --force` | | Auto-resolve conflicts (use first variant found) |
| `skills` | `sk` | Install / remove / list / search / read / sync skills |
| `skills search --direct` | | Search skills.sh directly (human-readable) |
| `skills search --direct --install` | | Fetch and install top result automatically |
| `skills read <path\|github:owner/repo/name>` | | Preview a SKILL.md from disk or GitHub (`--full` for untruncated) |
| `skills install --targets <t1,t2>` | | Install all bundled skills to targets |
| `skills install --skill <name> --targets <t>` | | Install one specific skill |
| `skills install --local <path> --targets <t>` | | Install a local skill folder |
| `skills install --dry-run` | | Preview installs without writing |
| `skills install --mode <copy\|symlink>` | | Symlink instead of copy (default `copy`) |
| `skills remove --skill <name> --targets <t>` | | Remove a skill |
| `skills list` / `skills list --target <t>` | | List installed skills (optionally filter to one target) |
| `skills sync <from> <to>` | | Copy skills between targets |
| `mcp list` | | Scan all OS MCP config files |
| `mcp list --client <id>` | | Search registry for a specific client |
| `mcp list --installed` | | List only installed MCPs with env-var status |
| `mcp list --search <text>` / `--category <name>` | | Filter the list |
| `mcp status --client <id>` | | Show servers in one config + env var status |
| `mcp install --id <id> --client <id>` | | Install one MCP server |
| `mcp install --id a,b,c` | | Batch-install MCPs (parallel preflight) |
| `mcp install --id <id> --config <path>` | | Install into a custom config file |
| `mcp install --id <id> --env K=V,K2=V2` | | Set env vars for the installed server |
| `mcp install --id <id> --force` | | Overwrite an existing entry |
| `mcp remove --id <id> --client <id>` | | Remove an MCP server |
| `cache status` | | Inspect cache sizes (repos, skills, logs) |
| `cache clean --all` | | Clean repos + skills + logs |
| `cache clean --repos` / `--skills` / `--logs` | | Clean specific targets |
| `cache clean --tools` | | Clean all tool caches (local + lsp + api) |
| `cache clean --local` / `--lsp` / `--api` | | Clean individual tool caches |
| `cache clean --all --dry-run` | | Show what would be freed without deleting |
| `cache clean --all --yes` | | Skip the confirmation prompt |
| `tools [name...]` | | List tools, show schema(s), or run with `--queries '<json>'` |
| `instructions` | | Print MCP instructions + every tool schema |

Most subcommands accept `--hostname <host>` for GitHub Enterprise.

---

## Global Flags

| Flag | Description |
|------|-------------|
| `--json`, `-j` | Raw JSON output (machine-readable) |
| `--version`, `-v` | Print the CLI version |
| `--help`, `-h` | Show help for the CLI or a command |

## Environment

| Variable | Purpose |
|----------|---------|
| `OCTOCODE_TOKEN` | GitHub token (checked first) |
| `GH_TOKEN` | GitHub token (checked second) |
| `GITHUB_TOKEN` | GitHub token (checked third) |
| `OCTOCODE_HOME` | Override the data directory (default `~/.octocode`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error |

---

## Supported Clients

`cursor`, `claude` / `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

---

## Troubleshooting

```bash
octocode status --sync                   # Full health check including sync analysis
octocode token --validate                # Verify your GitHub token against the API
octocode token --source                  # Debug token resolution chain
octocode install --ide cursor --check    # Pre-flight before installing
octocode sync --dry-run                  # Preview sync changes before applying
octocode cache clean --all --dry-run     # See what cache clean would free
```

---

## Docs

- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md): full command and flag reference
- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md): bundled skills installation
- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md)

---

## Privacy & Telemetry

De-identified telemetry (command usage, error rates) helps improve the CLI. Source code, env values, and repo contents are not collected.

[Privacy Policy](https://github.com/bgauryy/octocode-mcp/blob/main/PRIVACY.md) · [Terms of Usage](https://github.com/bgauryy/octocode-mcp/blob/main/TERMS.md)

MIT. Copyright 2026 Octocode AI.
