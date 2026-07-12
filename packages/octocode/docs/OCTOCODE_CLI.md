# Octocode CLI

The Octocode CLI is the terminal interface over the same research engine used by
the Octocode MCP server. One binary — `npx octocode` — covers code search, exact
file reads, directory trees, LSP symbol navigation, GitHub repos, npm packages,
PRs, commits, MCP client setup, GitHub auth, and agent skill
management.

```text
octocode CLI quick command  ──► same core runners ──► GitHub, local, npm, LSP
octocode CLI tools <name>   ──► same tool runner   ──► the MCP tool catalog
octocode MCP tool call      ──► same core runners  ──► GitHub, local, npm, LSP
```

CLI and MCP share logic, schemas, security, sanitization, and tool execution.
They are not separate implementations.

## Commands

| Command | Purpose |
|---|---|
| `search` | Unified research: code text/regex/AST, file reads, trees, file discovery, LSP semantics, GitHub repos, npm packages, PRs, commits, diffs, and materialization. |
| `clone` | Clone a GitHub repo or sparse subtree locally for repeated reads, AST search, or LSP work. |
| `cache` | Fetch remote files, trees, or repos into local Octocode storage; also inspect or clear cached materialization. |
| `tools` | List every Octocode MCP tool, read exact tool schemas, and run raw tool calls from the terminal. |
| `context` | Print the agent protocol, system prompt, tool descriptions, and schemas. |
| `skill` | List, install, update, or preview Octocode and GitHub Agent Skill folders for Claude, Cursor, Codex, Pi, and others. |
| `install` | Write or check MCP client configuration for supported IDEs and agent hosts. |
| `auth` | Manage GitHub auth with `login`, `logout`, `refresh`, and `status` subcommands. |
| `login` | Top-level shortcut for GitHub login. |
| `logout` | Top-level shortcut for clearing stored GitHub credentials. |
| `status` | Show auth, token source, cache, install, and optional MCP sync health. |
| `lsp-server` | List, inspect, install, uninstall, or clean language servers used by semantic search. |

Use `npx octocode <command> --help` for the live command help for any command.

## Quick Start

```bash
npx octocode --help
npx octocode status --json
npx octocode search ./src --tree --depth 2
npx octocode search "createServer" ./src --lang ts --view discovery
npx octocode search ./src/index.ts --content-view exact
npx octocode search ./src/index.ts --op documentSymbols
npx octocode tools
npx octocode tools localSearchCode --scheme
```

Replace `npx octocode` with `octocode` when the package is installed globally.

---

## `search` — The Research Command

`search` is the unified command for read-only work. It routes local paths to
local tools, `owner/repo[/path]` refs to GitHub tools, npm names to package
search, and `--op` requests to LSP.

| Need | Example | Backing capability |
|---|---|---|
| Search code text | `npx octocode search "runCLI" ./src --lang ts` | `localSearchCode`, `ghSearchCode` |
| Search with regex | `npx octocode search --regex "run[A-Z]\w+" ./src --lang ts` | Local regex search |
| Search structurally | `npx octocode search --pattern "eval($X)" ./src --lang ts` | Structural AST engine |
| Find files | `npx octocode search parser ./src --search path --ext ts` | `localFindFiles` |
| Read exact content | `npx octocode search ./src/index.ts --content-view exact` | `localGetFileContent`, `ghGetFileContent` |
| Read a proof slice | `npx octocode search ./src/index.ts --match-string "runCLI" --content-view exact` | Exact file with anchors |
| Browse structure | `npx octocode search ./src --tree --depth 2` | `localViewStructure`, `ghViewRepoStructure` |
| Trace symbols | `npx octocode search ./src/index.ts --op references --symbol runCLI --line 42` | `lspGetSemantics` |
| Discover repos | `npx octocode search "mcp server" --target repositories --lang TypeScript` | `ghSearchRepos` |
| Resolve npm packages | `npx octocode search zod --target packages` | `npmSearch` |
| Read PRs | `npx octocode search bgauryy/octocode#123 --target pullRequests --comments` | `ghHistoryResearch` |
| Read commits | `npx octocode search vercel/next.js/src --target commits --since 2024-01-01T00:00:00Z` | `ghHistoryResearch` |
| Diff files or refs | `npx octocode search src/a.ts src/b.ts --target diff` | OQL diff lane |
| Materialize remote code | `npx octocode search "useState" packages/next/src --repo vercel/next.js --materialize required` | `ghCloneRepo` + local tools |

### Search loop

```text
map cheaply → search narrowly → read exact evidence → follow symbols or history
```

```bash
npx octocode search ./packages/octocode/src --tree --depth 2
npx octocode search "executeDirectTool" ./packages/octocode/src --lang ts --view discovery
npx octocode search ./packages/octocode/src/cli/tool-command.ts --match-string "executeDirectTool" --content-view exact
npx octocode search ./packages/octocode/src/cli/tool-command.ts --op references --symbol executeToolCommand --line 90
```

### Key flags

| Flag | Meaning |
|---|---|
| `--tree` | Browse directory or repository structure. |
| `--content-view exact\|compact\|symbols` | Control file-read detail. |
| `--match-string`, `--start-line`, `--end-line` | Keep proof reads small. |
| `--search path\|content\|both` | Switch between file discovery and code search. |
| `--pattern`, `--rule` | Run structural AST search. |
| `--op documentSymbols\|definition\|references\|callers\|callees\|callHierarchy\|hover\|typeDefinition\|implementation` | Run semantic LSP search. |
| `--target repositories\|packages\|pullRequests\|commits\|diff\|research\|graph\|materialize` | Ask for a specific answer type. |
| `--repo owner/repo[@ref]` | Materialize a remote repo or subtree first, then run a local workflow. |
| `--scheme` | Print the OQL contract. Use `--scheme --compact` for a shorter agent guide. |
| `--explain --dry-run` | Show how a query routes without running it. |

---

## `tools` — Raw MCP Catalog

`tools` exposes the same tool catalog that MCP clients call. Useful for agents
and scripts that want exact tool inputs.

```bash
npx octocode tools
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --queries '{"path":"./src","keywords":"runCLI"}' --compact
```

**Always read the schema before a raw call:**

```bash
npx octocode tools <name> --scheme
```

| Category | Tools |
|---|---|
| GitHub | `ghSearchCode` · `ghSearchRepos` · `ghHistoryResearch` · `ghGetFileContent` · `ghViewRepoStructure` · `ghCloneRepo` |
| Local Code | `localSearchCode` · `localFindFiles` · `localGetFileContent` · `localViewStructure` · `lspGetSemantics` |
| Package | `npmSearch` |

---

## `skill` — Agent Skill Management

Install Agent Skill folders into supported agent destinations. Official Octocode
skills are **bundled inside the npm package** — no network call needed, works
offline, always the correct version for this release.

### Install a named Octocode skill

```bash
npx octocode skill --name octocode-research
npx octocode skill --name octocode-awareness
npx octocode skill --name octocode-research --platform cursor
npx octocode skill --name octocode-research --platform claude
npx octocode skill --name octocode-research --platform all --dry-run
```

`--name` checks the bundled skills first. If the skill is in the bundle it is
installed directly from there. If it is not in the bundle (e.g. a newer skill
published after this CLI release), the CLI falls back to fetching from GitHub.

### Install from an agent-known local skill path

```bash
npx octocode skill --add --path /absolute/path/to/skills --platform common
npx octocode skill --add --path /absolute/path/to/skills/octocode-awareness --platform codex --dry-run
npx octocode skill --add --path /absolute/path/to/skills/octocode-awareness/SKILL.md --platform pi
```

Use `--add --path` when the agent already knows the bundled skill location. The
path may point at a skill folder, its `SKILL.md`, or a local skills library
folder; a library path installs every direct child skill folder.

### Install all bundled Octocode skills

```bash
npx octocode skill --install-all
npx octocode skill --install-all --platform pi
npx octocode skill --install-all --platform all --mode hybrid --dry-run
```

### Install a skill from GitHub

```bash
npx octocode skill --add owner/repo/skills/code-review --platform cursor,codex
npx octocode skill --add https://github.com/owner/repo/tree/main/skills/review --platform common
```

`--add <github-path>` fetches from GitHub. Use `--add --path <local-path>` for a
local bundled or agent-known skill folder.

### List available skills

```bash
npx octocode skill --list
npx octocode skill --list --json
```

`--list` shows the skills bundled with this install (offline, matches
`--install-all` exactly). If no bundle is found (e.g. running unbuilt from
source), it falls back to the live marketplace listing from GitHub (cached 24
hours), then to a known-names list if that also fails.

### Platforms

| Platform | Destination |
|---|---|
| `common` (default) | `~/.agents/skills` |
| `cursor` | `~/.cursor/skills` |
| `claude` | Claude Code + Claude Desktop skill folders |
| `codex` | `~/.agents/skills` |
| `opencode` | `~/.config/opencode/skills` |
| `pi` | `~/.pi/agent/skills` |
| `copilot` | `~/.copilot/skills` |
| `gemini` | `~/.gemini/skills` |
| `all` | All platforms |

### Install modes

| Mode | Meaning |
|---|---|
| `symlink` (default) | Symlink each target to Octocode's skill-sources cache — one reinstall updates all targets. |
| `copy` | Embed a standalone copy in each target directory. |
| `hybrid` | Copy for Claude targets (where direct file access matters), symlink for everything else. |

### Useful flags

| Flag | Meaning |
|---|---|
| `--force` / `--update` | Overwrite existing installed skill folders. |
| `--dry-run` | Preview destinations and actions without installing. |
| `--verbose` | Show source URL and cache path. |
| `--json` | Structured JSON output. |
| `--branch <ref>` | Branch, tag, or SHA when fetching from GitHub. |
| `--path <path>` | Local skill folder, `SKILL.md`, or local skills library path; use with `--add`. |

---

## `clone` — Materialize a GitHub Repo

```bash
npx octocode clone vercel/next.js
npx octocode clone vercel/next.js/packages/next
npx octocode clone vercel/next.js@canary/packages/next
```

Use clone when you need to inspect several files, run structural (AST) search, or
use LSP on remote code. Clone is enabled by default in the CLI unless
`ENABLE_CLONE=false`; MCP clone requires `ENABLE_CLONE=true`.

---

## `cache` — Materialize Remote Files

```bash
npx octocode cache fetch vercel/next.js README.md --depth file
npx octocode cache fetch vercel/next.js packages/next --depth tree
npx octocode cache fetch vercel/next.js --depth clone --json
npx octocode cache status
npx octocode cache clear --all
```

Use the returned absolute local path with `search`, `search --tree`,
`search --search path`, or `search --op`.

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

Use when `search --op ...` reports an LSP server is unavailable.

---

## `context` — Agent Protocol

```bash
npx octocode context
npx octocode context --full
npx octocode context --json
```

Prints the research protocol and tool descriptions. Use for autonomous agents,
debugging tool guidance, or producing a compact machine-readable context block.

---

## Recommended Workflows

### Orient in a local codebase

```bash
npx octocode search ./src --tree --depth 2
npx octocode search "parseArgs" ./src --lang ts --view discovery
npx octocode search ./src/cli/parser.ts --match-string "parseArgs" --content-view exact
```

### Remote repo to local proof

GitHub code search can return zero rows when a provider has not indexed a repo.
Treat that as a provider gap, not proof of absence.

```bash
npx octocode search vercel/next.js/packages/next --tree --depth 2
npx octocode search "useState" packages/next/src --repo vercel/next.js --materialize required --lang ts
```

### Symbols and references

Get line anchors first, then trace the symbol:

```bash
npx octocode search ./src/index.ts --op documentSymbols
npx octocode search ./src/index.ts --op references --symbol runCLI --line 42
```

### Package to source

```bash
npx octocode search zod --target packages
npx octocode search "ZodObject" colinhacks/zod --lang ts
```

### Pull requests and history

```bash
npx octocode search bgauryy/octocode --target pullRequests --state merged --limit 10
npx octocode search bgauryy/octocode#123 --target pullRequests --patches --comments
npx octocode search bgauryy/octocode/packages/octocode/src --target commits --since 2024-01-01T00:00:00Z
```

### Agent or script mode

```bash
npx octocode context --json
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --queries '{"path":"./src","keywords":"runCLI"}' --json --compact
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

## Command Aliases (older → current)

| Old habit | Use now |
|---|---|
| `cat` | `search <file> --content-view exact` |
| `ls` | `search <dir> --tree` |
| `find` | `search <query> <path> --search path` |
| `grep` | `search "term" <path>` |
| `lsp` | `search <file> --op <operation>` |
| `pkg` | `search <package> --target packages` |
| `repo` | `search <keywords> --target repositories` |
| `pr` | `search owner/repo#N --target pullRequests` |
| `history` | `search owner/repo[/path] --target commits` |
| `diff` | `search <left> <right> --target diff` |

---

## How the CLI Aligns with MCP

| CLI surface | MCP alignment |
|---|---|
| `search` | Friendly shorthand over OQL and the same core runners used by MCP tools. |
| `tools <name>` | Direct terminal access to the same named tools exposed through MCP. |
| `tools <name> --scheme` | The schema contract for that tool. Do not guess fields. |
| `context` | The same agent-facing protocol, system prompt, and tool descriptions used to guide MCP/CLI research. |
| `install --ide <client>` | Writes MCP client configuration so editors and assistants can call `octocode-mcp`. |
| `auth` | Manages credentials used by both CLI and MCP flows. |

The code boundary is intentionally thin:
- `@octocodeai/octocode-core` owns tool and command metadata.
- `@octocodeai/octocode-tools-core` owns execution logic.
- `@octocodeai/octocode-engine` owns native primitives (minify, structural search, LSP, secret scanning).
- `octocode` renders commands in a terminal.
- `octocode-mcp` registers the same tools for MCP clients.

---

## Further Reading

- [Octocode Query Language](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) — cheatsheet, decision tree, common recipes, agent rules
- [OQL Language Reference](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) — full language spec: anatomy, targets, predicates, params, controls
- [OQL Results and Evidence](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md) — result envelope, evidence tiers, diagnostics, continuations, safe deletion
- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [MCP Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [All 13 Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
