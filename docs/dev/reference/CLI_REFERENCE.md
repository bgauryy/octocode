# Octocode CLI Reference

`octocode` is the terminal interface for code research:

- Run Octocode MCP tools directly from the shell.
- Use smart research commands for files, trees, search, PRs, packages, and LSP workflows.
- Use raw tools for lower-level GitHub/local research and exact schema-driven calls.
- Manage Octocode setup for IDEs when needed.

## Usage

```bash
octocode <command> [options]
octocode tools
octocode tools <name>
octocode tools <name> --queries '<json>'
octocode repo <keywords...>
octocode files <query> [path|owner/repo]
octocode pkg <package>
octocode symbols <file|path>
octocode lsp <file> --type <type>
octocode context
```

## Agent Flow

Agents should use this order:

1. `octocode context`
2. `octocode tools`
3. `octocode tools <name> --scheme`
4. `octocode tools <name> --queries '<json>'`

Use `octocode context --full` only when every inline JSON schema is needed.

## Global Options

| Option | Meaning |
|--------|---------|
| `--help` | Show help. |
| `--version` | Show version. |
| `--json` | Print raw JSON MCP envelope for tool runs. |
| `--compact` | Print lean tool output. |
| `--no-color` | Disable ANSI color. Also honors `NO_COLOR=1`. |

## Tool Runner

`octocode tools` imports the canonical public catalog from `octocode-mcp/public`; the CLI does not maintain separate tool schemas.

`--queries` accepts:

```json
{ "path": ".", "keywords": "runCLI" }
```

```json
[{ "path": ".", "keywords": "runCLI" }]
```

```json
{ "queries": [{ "path": ".", "keywords": "runCLI" }] }
```

Direct CLI runs auto-fill `id`, `mainResearchGoal`, `researchGoal`, and `reasoning` when omitted.

### Tools

| Category | Tools |
|----------|-------|
| GitHub | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghSearchPRs`, `ghCloneRepo` |
| Local | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent` |
| LSP | `lspGetSemantics` |
| Package | `npmSearch` |

Examples:

```bash
octocode tools localSearchCode
octocode tools localSearchCode --scheme
octocode tools localSearchCode --queries '{"path":".","keywords":"runCLI"}'
octocode tools ghSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
```

## Commands

| Command | Purpose |
|---------|---------|
| `get` | Fetch and minify local or GitHub file content. |
| `tree` | View local or GitHub directory structure. |
| `files` | Find file paths and content matches locally or on GitHub. |
| `search` | Search local or GitHub code. |
| `pr` | Search or deep-dive pull requests. |
| `repo` | Discover GitHub repositories by keyword, topic, owner, and quality filters. |
| `pkg` | Research npm package metadata and source repositories. |
| `symbols` | Show semantic symbol outlines for local files or directories. |
| `lsp` | Run LSP semantic navigation for a local source file. |
| `tools` | List tools, show schemas, or run tools. |
| `context` | Print agent protocol and tool schemas. |
| `install` | Configure `octocode-mcp` for an IDE/client. |
| `auth` | Auth menu and auth subcommands. |
| `login` | GitHub OAuth login. |
| `logout` | Remove stored Octocode auth. |
| `status` | Show health status. |
| `token` | Print the resolved token. |
| `skills` | Search, read, install, remove, list, or sync skills. |

### files

```bash
octocode files <query> [path|owner/repo] [--owner <owner> --repo <repo>] [--source auto|local|github] [--search path|content|both] [--ext <list>] [--path <subpath>] [--limit <n>] [--page <n>] [--json]
```

Examples:

```bash
octocode files auth src --source local --search path --ext ts
octocode files executeDirectTool . --source local --search content --ext ts
octocode files auth bgauryy/octocode-mcp --source github --search path --ext ts
octocode files auth --owner bgauryy --repo octocode-mcp --source github
octocode files executeDirectTool bgauryy/octocode-mcp --source github --search content
octocode files auth . --search both --limit 20
```

`--search path` uses metadata/path search (`localFindFiles` or `ghSearchCode match:path`).
`--search content` uses content search (`localSearchCode` or `ghSearchCode match:file`).
Local path filters include `--name`, `--path-pattern`, `--regex`, `--entry`, depth, size, time, permission, and metadata flags.
Local content filters include ripgrep-style flags such as `--include`, `--exclude`, `--fixed-string`, `--perl-regex`, case, word, hidden, count, match, and sort controls.
GitHub filters include `--owner`, `--repo`, `--filename`, `--path`, `--ext`, pagination, and `--verbose`.
Use `octocode get <returned-path> --match-string <query> --mode none` to fetch exact evidence.

### get

```bash
octocode get <path|github-ref> [--mode none|standard|symbols] [--branch <ref>] [--match-string <s>] [--match-regex] [--match-case-sensitive] [--start-line <n>] [--end-line <n>] [--context-lines <n>] [--page-size <n>] [--page <n>] [--char-offset <n>] [--char-length <n>] [--full-content] [--content-type file|directory] [--force-refresh] [--json]
```

Examples:

```bash
octocode get src/cli/commands/search.ts --match-string executeDirectTool --mode none
octocode get bgauryy/octocode-mcp/packages/octocode-cli/src/cli/commands/search.ts --match-string executeDirectTool --mode none
octocode get src/index.ts --start-line 40 --end-line 90 --mode none
octocode get src/index.ts --mode symbols
```

### repo

```bash
octocode repo <keywords...> [--topic <list>] [--language <lang>] [--owner <owner>] [--stars <range>] [--forks <range>] [--good-first-issues <range>] [--license <spdx>] [--created <range>] [--updated <range>] [--size <range>] [--match name,description,readme] [--sort stars|forks|help-wanted-issues|updated|best-match] [--archived true|false] [--visibility public|private] [--limit <n>] [--page <n>] [--verbose] [--json]
```

Examples:

```bash
octocode repo react state --language TypeScript --stars '>1000'
octocode repo --topic mcp,agents --sort stars --limit 10
octocode repo --owner vercel --language TypeScript --verbose
```

Use results with `octocode tree <owner/repo>`, `octocode search <pattern> <owner/repo>`, or `octocode get <owner/repo/path>`.

### pkg

```bash
octocode pkg <package> [--page <n>] [--json]
```

Examples:

```bash
octocode pkg zod
octocode pkg @modelcontextprotocol/sdk
```

### lsp

```bash
octocode lsp <file> --type <type> [--symbol <name>] [--line <n>] [--workspace-root <path>] [--page <n>] [--page-size <n>] [--context-lines <n>] [--depth <n>] [--format structured|compact] [--json]
```

Supported `--type` values: `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`.

`documentSymbols` only needs a file. All other types require `--symbol` and `--line`.

Examples:

```bash
octocode lsp src/index.ts --type documentSymbols
octocode lsp src/index.ts --type references --symbol runCLI --line 42
octocode lsp src/index.ts --type hover --symbol runCLI --line 42
```

### symbols

```bash
octocode symbols <file|path> [--ext <list>] [--kind <kind>] [--limit <n>] [--depth <n>] [--page-size <n>] [--json]
```

For a file, `symbols` runs `lspGetSemantics` with `type=documentSymbols`.
For a directory, it uses `localFindFiles` to discover source files, then batches
`lspGetSemantics type=documentSymbols` over those files.

Examples:

```bash
octocode symbols src/index.ts
octocode symbols src --ext ts,tsx --limit 10
octocode symbols src/index.ts --kind function
```

### install

```bash
octocode install --ide <client> [--method npx] [--force] [--check] [--rollback] [--backup-path <path>] [--json]
```

Supported clients: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`.

Only `npx` is supported as an install method.

### auth

```bash
octocode auth [login|logout|status|token|refresh] [--hostname <host>] [--json]
octocode login [--hostname <host>] [--git-protocol <ssh|https>] [--force] [--json]
octocode logout [--hostname <host>] [--yes] [--json]
octocode status [--hostname <host>] [--sync] [--json]
octocode token [--type auto|octocode|gh] [--hostname <host>] [--source] [--validate] [--reveal] [--json]
```

Token priority for `auto`: `OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, encrypted Octocode credentials, then `gh auth token`.

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
