# Octocode CLI Reference

`octocode` is the terminal interface for two jobs:

- Manage Octocode configuration: install, auth, MCP marketplace, skills, sync, cache.
- Run any Octocode MCP tool directly from the shell.

## Usage

```bash
octocode <command> [options]
octocode tools
octocode tools <name>
octocode tools <name> --queries '<json>'
octocode instructions
octocode --agent
```

Legacy aliases still work for tool runs:

```bash
octocode --tool <name> --queries '<json>'
octocode --tool <name> --help
```

## Agent Flow

Agents should use this order:

1. `octocode --agent`
2. `octocode tools`
3. `octocode tools <name>`
4. `octocode tools <name> --queries '<json>'`

Use `octocode --agent --full` only when every inline JSON schema is needed.

## Global Options

| Option | Meaning |
|--------|---------|
| `-h`, `--help` | Show help. |
| `-v`, `--version` | Show version. |
| `--json` | Print raw JSON MCP envelope for tool runs. |
| `--compact` | Print lean tool output. |
| `--no-color` | Disable ANSI color. Also honors `NO_COLOR=1`. |

## Tool Runner

`octocode tools` imports the canonical public catalog from `octocode-mcp/public`; the CLI does not maintain separate tool schemas.

`--queries` accepts:

```json
{ "path": ".", "pattern": "runCLI" }
```

```json
[{ "path": ".", "pattern": "runCLI" }]
```

```json
{ "queries": [{ "path": ".", "pattern": "runCLI" }] }
```

Direct CLI runs auto-fill `id`, `mainResearchGoal`, `researchGoal`, and `reasoning` when omitted.

### Tools

| Category | Tools |
|----------|-------|
| GitHub | `githubSearchCode`, `githubGetFileContent`, `githubViewRepoStructure`, `githubSearchRepositories`, `githubSearchPullRequests`, `githubCloneRepo` |
| Local | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent` |
| LSP | `lspGetSemanticContent` |
| Package | `packageSearch` |

Examples:

```bash
octocode tools localSearchCode
octocode tools localSearchCode --queries '{"path":".","pattern":"runCLI"}'
octocode tools githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
```

## Commands

| Command | Aliases | Purpose |
|---------|---------|---------|
| `install` | `i`, `setup` | Configure `octocode-mcp` for an IDE/client. |
| `auth` | `a`, `gh` | Auth menu and auth subcommands. |
| `login` | `l` | GitHub OAuth login. |
| `logout` | - | Remove stored Octocode auth. |
| `status` | `s` | Show auth status. |
| `token` | `t` | Print the resolved token. |
| `sync` | `sy` | Sync MCP configs across clients. |
| `skills` | `sk` | Search, read, install, remove, list, or sync skills. |
| `mcp` | - | List/install/remove/check marketplace MCP servers. |
| `cache` | - | Inspect or clean Octocode cache. |

### install

```bash
octocode install --ide <client> [--method npx|direct] [--force]
```

Supported clients: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`.

Use `npx` unless you specifically need a direct local binary path.

### auth

```bash
octocode auth [login|logout|status|token]
octocode login [--hostname <host>] [--git-protocol ssh|https]
octocode logout [--hostname <host>]
octocode status [--hostname <host>]
octocode token [--type auto|octocode|octocode-cli|gh] [--hostname <host>] [--source] [--json]
```

Token priority for `auto`: `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, encrypted Octocode credentials, then `gh auth token`. `octocode-cli` is accepted as an alias for `octocode`.

### sync

```bash
octocode sync [--force] [--dry-run] [--status]
```

### mcp

```bash
octocode mcp list [--search <text>] [--category <name>] [--installed]
octocode mcp install --id <mcp-id> [--client <client>|--config <path>] [--env KEY=VALUE] [--force]
octocode mcp remove --id <mcp-id> [--client <client>|--config <path>]
octocode mcp status [--client <client>|--config <path>]
```

### skills

```bash
octocode skills search <query> [--direct]
octocode skills read <path|url>
octocode skills list
octocode skills install [--skill <name>|--local <path>] [--targets <list>] [--mode copy|symlink] [--force]
octocode skills remove [--skill <name>|--local <path>] [--targets <list>]
octocode skills sync <from> <to>
```

Supported targets include `claude-code`, `claude-desktop`, `cursor`, `codex`, and `opencode`.

Skills guide: [docs/dev/SKILLS_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md).

### cache

```bash
octocode cache status
octocode cache clean [--repos] [--skills] [--logs] [--tools|--local|--lsp|--api] [--all]
```

## Environment

| Variable | Meaning |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data directory. |
| `NO_COLOR` | Disable terminal color. |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success. |
| `1` | General error. |
| `2` | Invalid input or unsupported flags. |
| `3` | Unknown tool or command. |
| `4` | Authentication failure. |
| `5` | Tool/API execution error. |
| `7` | Rate limited. |
