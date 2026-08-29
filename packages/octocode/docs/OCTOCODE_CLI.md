# Octocode CLI

The Octocode CLI is the terminal interface over the same research engine used by
the Octocode MCP server. One binary — `npx octocode` — covers code search, exact
file reads, directory trees, LSP symbol navigation, GitHub repos, npm packages,
PRs, commits, MCP client setup, and GitHub auth.

```text
octocode CLI tools <name>   ──► same tool runner   ──► the MCP tool catalog
octocode MCP tool call      ──► same core runners  ──► GitHub, local, npm, LSP
```

CLI and MCP share logic, schemas, security, sanitization, and tool execution.
They are not separate implementations.

## Commands

| Command | Purpose |
|---|---|
| `tools` | List every Octocode MCP tool, read exact tool schemas, and run raw tool calls from the terminal. This is the primary research surface. |
| `clone` | Clone a GitHub repo or sparse subtree locally for repeated reads, AST search, or LSP work. |
| `cache` | Fetch remote files, trees, or repos into local Octocode storage; also inspect or clear cached materialization. |
| `context` | Print agent protocol and tool descriptions. Use `--minimal`, default compact, or `--full` depending on context budget. |
| `install` | Write or check MCP client configuration for supported IDEs and agent hosts. |
| `auth` | Manage GitHub auth with `login`, `logout`, `refresh`, and `status` subcommands. |
| `login` | Top-level shortcut for GitHub login. |
| `logout` | Top-level shortcut for clearing stored GitHub credentials. |
| `status` | Show auth, token source, cache, install, and optional MCP sync health. |
| `lsp-server` | List, inspect, install, uninstall, or clean language servers used by semantic search. |
| `skill` | List, install, check, inspect, or remove bundled Octocode Agent Skills. |

Use `npx octocode <command> --help` for the live command help for any command.

## Quick Start

```bash
npx octocode --help
npx octocode status --json
npx octocode tools
npx octocode tools localViewStructure --scheme
npx octocode tools localSearchCode --scheme
npx octocode tools localSearchCode --queries '{"path":"./src","searchText":"createServer"}'
npx octocode tools localGetFileContent --queries '{"path":"./src/index.ts","fullContent":true}'
npx octocode skill list
npx octocode skill install octocode-research --platform pi
```

Replace `npx octocode` with `octocode` when the package is installed globally.

---

## `tools` — The Research Command

`tools` is the unified command for read-only research. Every capability MCP
clients get — GitHub code/repo/PR/commit search, local text/AST search, file
reads, directory trees, npm lookup, and LSP semantics — is one named tool away.

```bash
npx octocode tools
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --scheme --json --compact --pretty
npx octocode tools localSearchCode --queries '{"path":"./src","searchText":"runCLI"}' --compact
```

**Always read the schema before a raw call:**

```bash
npx octocode tools <name> --scheme
```

| Category | Tools |
|---|---|
| GitHub | `ghSearchCode` · `ghSearchRepos` · `ghSearchPullRequests` · `ghSearchIssues` · `ghSearchCommits` · `ghGetFileContent` · `ghViewRepoStructure` · `ghCloneRepo` |
| Local Code | `localSearchCode` · `localFindFiles` · `localAnalyzeGraph` · `localGetFileContent` · `localViewStructure` · `lspGetSemantics` |
| Package | `npmSearch` |

### Research loop

```text
map cheaply → search narrowly → read exact evidence → follow symbols or history
```

```bash
npx octocode tools localViewStructure --queries '{"path":"./packages/octocode/src"}'
npx octocode tools localSearchCode --queries '{"path":"./packages/octocode/src","searchText":"executeDirectTool","mode":"discovery"}'
npx octocode tools localGetFileContent --queries '{"path":"./packages/octocode/src/cli/tool-command.ts","matchString":"executeDirectTool"}'
npx octocode tools lspGetSemantics --queries '{"uri":"./packages/octocode/src/cli/tool-command.ts","type":"references","symbolName":"executeToolCommand","lineHint":90}'
```

### Key flags for `tools`

| Flag | Meaning |
|---|---|
| `--scheme` | Print the tool's input schema: fields, types, bounds, defaults. Read this before any unfamiliar call. |
| `--scheme --json` | Machine-readable schema. |
| `--queries '<json>'` | Run the tool. Accepts a single query object or `{"queries":[...]}` for a batch (up to 5). |
| `--json` | Full `CallToolResult` envelope. |
| `--compact` | Lean `structuredContent` only — cheapest output for agents. |
| `--pretty` | Pretty-print compact JSON for humans; useful with `--compact` when reading locally. |
| `--raw` | Content reads only: bare content without the envelope. |

---

## `clone` — Materialize a GitHub Repo

```bash
npx octocode clone vercel/next.js
npx octocode clone vercel/next.js/packages/next
npx octocode clone vercel/next.js@canary/packages/next
```

Use clone when you need to inspect several files, run structural (AST) search, or
use LSP on remote code. Clone is enabled by default in the CLI unless
`ENABLE_CLONE=false`; MCP clone requires `ENABLE_CLONE=true`. After cloning, run
`tools localSearchCode`, `tools localViewStructure`, `tools localGetFileContent`,
or `tools lspGetSemantics` on the returned absolute local path.

---

## `cache` — Materialize Remote Files

```bash
npx octocode cache fetch vercel/next.js README.md --depth file
npx octocode cache fetch vercel/next.js packages/next --depth tree
npx octocode cache fetch vercel/next.js --depth clone --json
npx octocode cache status
npx octocode cache clear --all
```

Cache data is shared with the MCP server under the configured Octocode home:

| Bucket | Path | Contents |
|---|---|---|
| Clone | `tmp/clone/{owner}/{repo}/{branch}` | Reusable Git checkouts |
| Tree | `tmp/tree/{owner}/{repo}/{commitSha}` | Materialized repository trees |
| Response | `tmp/response/` | Eligible GitHub and npm response payloads |

Direct CLI tool execution performs the persisted maintenance due-check once per process and does not keep a background timer alive. The check is shared across processes, runs at most once per 24 hours when due, and never makes tool startup fail if cleanup cannot run. Help, schema, and context-only commands do not trigger maintenance.

`cache status` reports the total `tmp` size plus clone, tree, and response usage. `cache clear --clone` and `cache clear --tree` are selective. `cache clear --all` removes the entire `tmp` directory, including response entries and maintenance metadata; there is currently no response-only clear flag. See [Cache storage and lifecycle](../../../docs/CONFIGURATION.md#cache-storage-and-lifecycle) for expiry rules and configuration.

Use the returned absolute local path with `tools localSearchCode`,
`tools localViewStructure`, `tools localFindFiles`, `tools localGetFileContent`,
or `tools lspGetSemantics`.

---

## `install` — MCP Client Setup

```bash
npx octocode install --ide cursor
npx octocode install --ide claude-code --check
npx octocode install --ide claude-desktop --force
```

Supported clients: Cursor, Claude Desktop, Claude Code, Windsurf, Zed, VS Code
Cline/Roo/Continue, OpenCode, Trae, Antigravity, Codex, Gemini CLI, Goose, Kiro.

---

## `auth` / `login` / `logout` / `status`

```bash
npx octocode auth status --json
npx octocode auth login
npx octocode auth refresh
npx octocode auth logout
npx octocode status --sync
```

Humans: run `login` once. Agents and CI: pass `OCTOCODE_TOKEN`, `GH_TOKEN`, or
`GITHUB_TOKEN` through the environment.

---

## `lsp-server` — Language Server Management

```bash
npx octocode lsp-server list
npx octocode lsp-server status src/main.rs
npx octocode lsp-server install rust-analyzer
npx octocode lsp-server install --all
```

Use when `tools lspGetSemantics` reports an LSP server is unavailable.

---

## `skill` — Bundled Agent Skills

The `octocode` package bundles the canonical Octocode skills from this repo's
`skills/` directory at build/publish time. Install copies a skill into the
canonical Octocode home, then optionally links it into agent-specific skill
directories.

```bash
npx octocode skill list
npx octocode skill info octocode-research
npx octocode skill install octocode-research --platform pi
npx octocode skill install --all --platform pi,cursor
npx octocode skill check --json
npx octocode skill remove octocode-research --platform pi
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--platform pi,cursor,claude,codex,opencode,copilot,gemini,common,all` | Link installed skills into one or more agent skill directories. |
| `--workspace` / `--repo` | Link into `<cwd>/.agents/skills/`. |
| `--path <dir>` | Install directly into a custom directory instead of Octocode home. |
| `--mode symlink\|copy\|hybrid` | Link strategy; `hybrid` copies Claude targets and symlinks the rest. |
| `--keep` | Preserve existing installs; default behavior overwrites with the bundled copy. |
| `--dry-run` | Preview actions without writing. |
| `--fix` | `check` only: repair missing/broken installed locations. |
| `--no-env` | `check` only: skip skill environment-readiness checks. |

Legacy forms such as `npx octocode skill --list` and
`npx octocode skill --name octocode-research` still route to `list` and
`install`, but the subcommands above are preferred.

---

## `context` — Agent Protocol

```bash
npx octocode context --minimal   # cheapest: protocol + active tool names
npx octocode context             # compact protocol + short descriptions
npx octocode context --full      # full MCP prompt + long descriptions
npx octocode context --json
```

Prints the research protocol and active tool descriptions. Use `--minimal` for tight agent budgets, default `context` for normal agents, and `--full` only when debugging full guidance.

---

## Recommended Workflows

### Orient in a local codebase

```bash
npx octocode tools localViewStructure --queries '{"path":"./src"}'
npx octocode tools localSearchCode --queries '{"path":"./src","searchText":"parseArgs","mode":"discovery"}'
npx octocode tools localGetFileContent --queries '{"path":"./src/cli/parser.ts","matchString":"parseArgs"}'
```

### Remote repo to local proof

GitHub code search can return zero rows when a provider has not indexed a repo.
Treat that as a provider gap, not proof of absence.

```bash
npx octocode tools ghViewRepoStructure --queries '{"owner":"vercel","repo":"next.js","path":"packages/next"}'
npx octocode clone vercel/next.js/packages/next
npx octocode tools localSearchCode --queries '{"path":"<clone localPath>/src","searchText":"useState"}'
```

### Symbols and references

Get line anchors first, then trace the symbol:

```bash
npx octocode tools lspGetSemantics --queries '{"uri":"./src/index.ts","type":"documentSymbols"}'
npx octocode tools lspGetSemantics --queries '{"uri":"./src/index.ts","type":"references","symbolName":"runCLI","lineHint":42}'
```

### Package to source

```bash
npx octocode tools npmSearch --queries '{"packageName":"zod"}'
npx octocode tools ghSearchCode --queries '{"keywords":["ZodObject"],"owner":"colinhacks","repo":"zod"}'
```

### Pull requests and history

```bash
npx octocode tools ghSearchPullRequests --queries '{"owner":"bgauryy","repo":"octocode","state":"merged","limit":10}'
npx octocode tools ghSearchPullRequests --queries '{"owner":"bgauryy","repo":"octocode","prNumber":123,"content":{"patches":{"mode":"all"},"comments":{"discussion":true}}}'
npx octocode tools ghSearchCommits --queries '{"owner":"bgauryy","repo":"octocode","path":"packages/octocode/src","since":"2024-01-01T00:00:00Z"}'
```

### Agent or script mode

```bash
npx octocode context --minimal
npx octocode context --json
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --scheme --json --compact --pretty
npx octocode tools localSearchCode --queries '{"path":"./src","searchText":"runCLI"}' --json --compact
```

---

## Output, Flags, and Exit Codes

### Common flags

| Flag | Meaning |
|---|---|
| `--help` | Show command help. |
| `--version` | Show CLI version. |
| `--json` | Structured JSON output. |
| `--compact` | Leaner output for agents and scripts. |
| `--pretty` | Pretty-print compact JSON for humans. |
| `--minimal` | `context` only: cheapest protocol + active tool names. |
| `--raw` | Bare file content where supported. |
| `--no-color` | Disable ANSI color. `NO_COLOR=1` works too. |

### Exit codes

| Code | Meaning |
|---:|---|
| `0` | Success. |
| `1` | General error. |
| `2` | Invalid input or unsupported flags. |
| `3` | Not found: unknown command/tool, missing symbol, or empty semantic result. |
| `4` | Authentication failure. |
| `5` | Tool or API execution error. |
| `7` | Rate limited. |

### Environment variables

| Variable | Meaning |
|---|---|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data and cache location. |
| `ENABLE_LOCAL` | Enable local filesystem tools. Defaults to `true`. |
| `ENABLE_CLONE` | Enable clone/materialization. CLI clone is enabled by default unless set to `false`; MCP clone requires `true`. |
| `NO_COLOR` | Disable terminal color. |

---

## How the CLI Aligns with MCP

| CLI surface | MCP alignment |
|---|---|
| `tools <name>` | Direct terminal access to the same named tools exposed through MCP. |
| `tools <name> --scheme` | The schema contract for that tool. Do not guess fields. |
| `context` | The same agent-facing protocol, system prompt, and tool descriptions used to guide MCP/CLI research. |
| `install --ide <client>` | Writes MCP client configuration so editors and assistants can call `octocode-mcp`. |
| `auth` | Manages credentials used by both CLI and MCP flows. |
| `skill` | Installs bundled Agent Skills locally; no MCP transport required. |

The code boundary is intentionally thin:
- `@octocodeai/octocode-tools-core` owns tool schemas, descriptions, and execution logic.
- `@octocodeai/octocode-core` currently supplies the shared system prompt and reusable output types.
- `@octocodeai/octocode-engine` owns native primitives (minify, structural search, LSP, secret scanning).
- `octocode` renders commands in a terminal.
- `octocode-mcp` registers the same tools for MCP clients.

---

## Further Reading

- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [MCP Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [All 13 Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
