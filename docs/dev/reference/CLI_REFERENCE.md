# Octocode CLI Reference

Two things in one binary: **manage** Octocode configuration and **run tools** directly from the terminal.

## Usage

```bash
# Manage Octocode (install, auth, skills, MCP marketplace, sync, cache)
octocode-cli <command> [options]

# Run any Octocode tool directly (agents and humans)
octocode-cli --tool <toolName> --queries '<json>'
octocode-cli --tool <toolName> --help
octocode-cli --tools-context
```

## Global Flags

| Flag | Effect |
|---|---|
| `--help` / `-h` | Show help |
| `--version` / `-v` | Show version |
| `--tools-context` | Print MCP instructions + all tool schemas |
| `--tool <name> --queries '<json>'` | Run one Octocode tool |
| `--tool <name> --help` | Show tool name, description, input/output schema |
| `--json` / `--output json` | Raw JSON result |

## Tools

Run any Octocode tool directly — for agents, scripts, and humans. `--queries` accepts a JSON object, array of objects, or `{ "queries": [...] }`. Fields `id`, `researchGoal`, `reasoning`, `mainResearchGoal` are auto-filled.

| Tool | Category | Description |
|---|---|---|
| `githubSearchCode` | GitHub | Search code across repositories |
| `githubGetFileContent` | GitHub | Fetch file content (supports matchString, line ranges) |
| `githubViewRepoStructure` | GitHub | List repo directory tree |
| `githubSearchRepositories` | GitHub | Search repos by keywords/topics |
| `githubSearchPullRequests` | GitHub | Search pull requests |
| `githubCloneRepo` | GitHub + Local | Clone GitHub repos/subtrees for local + LSP analysis |
| `localSearchCode` | Local | Ripgrep search in local files |
| `localGetFileContent` | Local | Read local file content |
| `localFindFiles` | Local | Find files by glob/pattern |
| `localViewStructure` | Local | View local directory tree |
| `lspGotoDefinition` | LSP | Go to symbol definition |
| `lspFindReferences` | LSP | Find all references to a symbol |
| `lspCallHierarchy` | LSP | Trace call hierarchy |
| `packageSearch` | Package | Search npm or Python packages |

Output schema (all tools):

```json
{ "content": [{ "type": "text", "text": "..." }], "structuredContent": {}, "isError": false }
```

Examples:

```bash
octocode-cli --tool githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
octocode-cli --tool githubGetFileContent --queries '{"owner":"facebook","repo":"react","path":"packages/react/src/React.js","matchString":"useState"}'
octocode-cli --tool githubCloneRepo --queries '{"owner":"facebook","repo":"react"}'
octocode-cli --tool localSearchCode --queries '{"path":".","pattern":"runCLI"}'
octocode-cli --tool packageSearch --queries '{"name":"react","ecosystem":"npm"}'
```

## Commands

Manage Octocode installation, authentication, skills, marketplace, sync, and cache.

| Command | Alias | What it does |
|---|---|---|
| `install` | `i`, `setup` | Configure octocode-mcp for an IDE |
| `auth` | `a`, `gh` | Auth menu or `auth login/logout/status/token` |
| `login` | `l` | GitHub OAuth login |
| `logout` | - | Remove Octocode auth |
| `status` | `s` | GitHub auth status |
| `token` | `t` | Print token (`--type`, `--hostname`, `--source`, `--json`) |
| `sync` | `sy` | Sync MCP configs (`--force`, `--dry-run`, `--status`) |
| `mcp` | - | Marketplace: `list`, `install`, `remove`, `status` |
| `skills` | `sk` | Skills: `list`, `install`, `remove` |
| `cache` | - | Cache: `status`, `clean` |

### install

```bash
octocode-cli install --ide <client> [--method <npx|direct>] [--force]
```

Supported: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`.

### auth / login / logout / status / token

```bash
octocode-cli auth [login|logout|status|token]
octocode-cli login [--hostname <host>] [--git-protocol <ssh|https>]
octocode-cli token [--type <auto|octocode-cli|gh>] [--hostname <host>] [--source] [--json]
```

### sync

```bash
octocode-cli sync [--force] [--dry-run] [--status]
```

### mcp

```bash
octocode-cli mcp list [--search <text>] [--category <name>] [--installed] [--client <client>|--config <path>]
octocode-cli mcp install --id <mcp-id> [--client <client>] [--env KEY=VALUE] [--force]
octocode-cli mcp remove --id <mcp-id> [--client <client>]
```

### skills

```bash
octocode-cli skills list
octocode-cli skills install [--skill <name>] [--targets <list>] [--mode <copy|symlink>] [--force]
octocode-cli skills remove --skill <name> [--targets <list>]
```

See [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md).

### cache

```bash
octocode-cli cache [status|clean] [--repos] [--skills] [--logs] [--all] [--tools|--local|--lsp|--api]
```

## Environment

| Variable | Meaning |
|---|---|
| `GITHUB_TOKEN` / `GH_TOKEN` | GitHub token |
| `OCTOCODE_HOME` | Override data directory |

## Exit Codes

`0` = success, `1` = error.
