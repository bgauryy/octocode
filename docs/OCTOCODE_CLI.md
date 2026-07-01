# Octocode CLI: One Terminal for Code Research

The Octocode CLI is the terminal version of the same research system exposed by
the Octocode MCP server. It gives you one command, `npx octocode`, for finding
code, reading exact files, browsing repositories, tracing symbols, inspecting
packages, studying PRs and commits, unpacking archives, configuring MCP clients,
and running every Octocode tool directly from a shell.

If MCP is Octocode inside your AI assistant, the CLI is Octocode in your
terminal, scripts, CI jobs, and debugging sessions.

```text
octocode CLI quick command -> same core runners -> GitHub, local files, npm, LSP, binaries
octocode CLI tools <name>  -> same tool runner  -> the MCP tool catalog
octocode MCP tool call     -> same core runners -> GitHub, local files, npm, LSP, binaries
```

That alignment is the point: the CLI is not a separate implementation. It is a
human- and script-friendly interface over the same schemas, metadata, security
checks, sanitization, pagination, and tool execution used by `octocode-mcp`.

## Why Use The CLI

- Use one command for local code, GitHub repos, npm packages, PRs, commits,
  archives, diffs, and semantic symbol navigation.
- Move from discovery to proof quickly: search cheaply, then read exact files,
  line ranges, matches, or symbols.
- Turn remote repos into local research targets with `clone`, `cache fetch`, or
  `search --repo`, then use local AST and LSP workflows.
- Script everything with `--json`, keep agent output small with `--compact`, and
  inspect exact schemas before raw tool calls.
- Configure MCP clients, auth, skill installs, caches, and language servers from
  the same binary.

## Quick Start

Run without installing globally:

```bash
npx octocode --help
npx octocode status --json
npx octocode search ./src --tree
npx octocode search "createServer" ./src --lang ts --view discovery
npx octocode search ./src/index.ts --content-view exact
npx octocode search ./src/index.ts --op documentSymbols
npx octocode tools
npx octocode tools localSearchCode --scheme
```

When you do install the package globally, replace `npx octocode` with
`octocode`.

## The Command Surface

| Command | What it is for |
|---|---|
| `search` | The main research command: code search, file reads, trees, file discovery, LSP semantics, GitHub repos, npm packages, PRs, commits, artifacts, diffs, OQL, research packets, graph proof, and materialization. |
| `unzip` | Unpack an archive into Octocode storage, then research the extracted tree with local search, tree, content, and LSP commands. |
| `clone` | Clone a GitHub repo or sparse subtree locally for repeated reads, AST search, or LSP work. |
| `cache` | Fetch remote files, trees, or repos into local Octocode storage; also inspect or clear cached materialization. |
| `tools` | List every Octocode MCP tool, read exact tool schemas, and run raw tool calls from the terminal. |
| `context` | Print the agent protocol, MCP system prompt, tool descriptions, and schemas. Also available as top-level `--context`. |
| `skill` | List, install, update, or preview Octocode and GitHub Agent Skill folders for supported local agent clients. |
| `install` | Write or check MCP client configuration for supported IDEs and agent hosts. |
| `auth` | Manage GitHub auth with `login`, `logout`, `refresh`, and `status` subcommands. |
| `login` | Top-level shortcut for GitHub login. |
| `logout` | Top-level shortcut for clearing stored GitHub credentials. |
| `status` | Show auth, token source, cache, install, and optional MCP sync health. |
| `lsp-server` | List, inspect, install, uninstall, or clean language servers used by semantic search. |

Use `npx octocode <command> --help` for the live command help.

## Search Is The Main Research Command

`search` is the unified command for read-only work. It accepts friendly
shorthand for common tasks and full OQL JSON when you need precise control.

| Need | Example | Backing capability |
|---|---|---|
| Search code text | `npx octocode search "runCLI" ./packages/octocode/src --lang ts` | `localSearchCode`, `ghSearchCode`, `oqlSearch` |
| Search with regex | `npx octocode search --regex "run[A-Z]\\w+" ./src --lang ts` | Local regex search |
| Search structurally | `npx octocode search --pattern "eval($X)" ./src --lang ts` | Octocode engine structural search |
| Find files | `npx octocode search parser ./src --search path --ext ts` | `localFindFiles`, OQL file predicates |
| Read exact content | `npx octocode search ./src/index.ts --content-view exact` | `localGetFileContent`, `ghGetFileContent` |
| Read a proof slice | `npx octocode search ./src/index.ts --match-string "runCLI" --content-view exact` | Exact file content with anchors |
| Browse structure | `npx octocode search ./src --tree --depth 2` | `localViewStructure`, `ghViewRepoStructure` |
| Trace symbols | `npx octocode search ./src/index.ts --op references --symbol runCLI --line 42` | `lspGetSemantics` |
| Discover repos | `npx octocode search "mcp server" --target repositories --lang TypeScript` | `ghSearchRepos` |
| Resolve npm packages | `npx octocode search zod --target packages` | `npmSearch` |
| Read PRs | `npx octocode search bgauryy/octocode#123 --target pullRequests --comments` | `ghHistoryResearch` |
| Read commits | `npx octocode search facebook/react/packages/react/src --target commits --since 2024-01-01T00:00:00Z` | `ghHistoryResearch` |
| Inspect archives or binaries | `npx octocode search app.zip --target artifacts --list` | `localBinaryInspect` |
| Diff files or refs | `npx octocode search src/a.ts src/b.ts --target diff` | OQL diff lane |
| Materialize remote code | `npx octocode search "useState" packages/react --repo facebook/react --materialize required` | `ghCloneRepo` plus local tools |

The best search loop is:

```text
map cheaply -> search narrowly -> read exact evidence -> follow symbols or history
```

For example:

```bash
npx octocode search ./packages/octocode/src --tree --depth 2
npx octocode search "executeDirectTool" ./packages/octocode/src --lang ts --view discovery
npx octocode search ./packages/octocode/src/cli/tool-command.ts --match-string "executeDirectTool" --content-view exact
npx octocode search ./packages/octocode/src/cli/tool-command.ts --op references --symbol executeToolCommand --line 90
```

## Raw Tools: The MCP Catalog From The Terminal

The `tools` command exposes the same tool catalog that MCP clients call. This is
useful for agents, scripts, and advanced users who want exact tool inputs.

```bash
npx octocode tools
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --queries '{"path":"./src","keywords":"runCLI"}' --compact
```

Always read a schema before a raw call:

```bash
npx octocode tools <name> --scheme
```

The current tool set is:

| Category | Tools |
|---|---|
| GitHub | `ghSearchCode`, `ghSearchRepos`, `ghHistoryResearch`, `ghGetFileContent`, `ghViewRepoStructure`, `ghCloneRepo` |
| Local Code | `localSearchCode`, `localFindFiles`, `localGetFileContent`, `localViewStructure`, `lspGetSemantics`, `localBinaryInspect` |
| Package | `npmSearch` |
| Other | `oqlSearch` |

## How The CLI Aligns With MCP

| CLI surface | MCP alignment |
|---|---|
| `search` | Friendly shorthand over OQL and the same core runners used by MCP tools. |
| `tools <name>` | Direct terminal access to the same named tools exposed through MCP. |
| `tools <name> --scheme` | The schema contract for that tool. Do not guess fields. |
| `context` | The same agent-facing protocol, system prompt, and tool descriptions used to guide MCP/CLI research. |
| `install --ide <client>` | Writes MCP client configuration so editors and assistants can call `octocode-mcp`. |
| `auth`, `login`, `logout`, `status` | Manages credentials and environment state used by both CLI and MCP flows. |

The code boundary is intentionally thin:

- `@octocodeai/octocode-core` owns tool and command metadata.
- `@octocodeai/octocode-tools-core` owns execution logic.
- `@octocodeai/octocode-engine` owns native primitives such as minify,
  structural search, LSP orchestration, binary inspection, and secret scanning.
- `octocode` renders commands in a terminal.
- `octocode-mcp` registers the same tools for MCP clients.

That keeps CLI behavior and MCP behavior from drifting apart.

## Command Details

### `search`

Use `search` for research. Most lanes are read-only; materialization lanes write
only to Octocode storage so remote code can be searched locally. It routes local
paths to local tools, `owner/repo[/path]` refs to GitHub tools, npm package
lookups to package search, and semantic operations to LSP.

Useful flags:

- `--tree` browses directory or repository structure.
- `--content-view exact|compact|symbols` controls file-read detail.
- `--match-string`, `--start-line`, and `--end-line` keep proof reads small.
- `--search path|content|both` switches between file discovery and code search.
- `--pattern` and `--rule` run structural AST search.
- `--op documentSymbols|definition|references|callers|callees|callHierarchy|hover|typeDefinition|implementation` runs semantic search.
- `--target repositories|packages|pullRequests|commits|artifacts|diff|research|graph|materialize` asks for a specific answer type.
- `--repo owner/repo[@ref]` materializes a remote repo or subtree first, then runs a local workflow.
- `--scheme` prints the OQL contract. Use `--scheme --compact` for a shorter agent guide.
- `--explain --dry-run` shows how a query will route without running it.

### `clone`

Clone a GitHub repo or subtree locally:

```bash
npx octocode clone facebook/react
npx octocode clone facebook/react/packages/react
npx octocode clone facebook/react@main/packages/react
```

Use clone when you expect to inspect several files, run structural search, or use
LSP on remote code. The CLI enables clone by default unless `ENABLE_CLONE=false`;
MCP clone tools require `ENABLE_CLONE=true`.

### `cache`

Materialize remote files, trees, or repos, then continue locally:

```bash
npx octocode cache fetch facebook/react README.md --depth file
npx octocode cache fetch facebook/react packages/react --depth tree
npx octocode cache fetch facebook/react --depth clone --json
npx octocode cache status
npx octocode cache clear --all
```

Use the returned absolute local path with `search`, `search --tree`,
`search --search path`, or `search --op`.

### `unzip`

Unpack archives once, then research the extracted tree:

```bash
npx octocode unzip app.zip
npx octocode search <localPath-from-output> --tree
```

For a single compressed stream or binary strings, use
`search --target artifacts` instead.

### `skill`

Install Agent Skill folders into supported local destinations:

```bash
npx octocode skill --list
npx octocode skill --name octocode-research
npx octocode skill --name octocode-research --platform codex
npx octocode skill --add owner/repo/skills/code-review --platform cursor,codex
npx octocode skill --install-all --platform all --dry-run
```

Supported platforms include `common`, `cursor`, `claude`, `codex`, `opencode`,
`pi`, `copilot`, `gemini`, and `all`.

### `install`

Configure an MCP client:

```bash
npx octocode install --ide cursor
npx octocode install --ide claude-code --check
npx octocode install --ide claude-desktop --force
```

Supported clients include Cursor, Claude Desktop, Claude Code, Windsurf, Zed,
VS Code Cline, VS Code Roo, VS Code Continue, OpenCode, Trae, Antigravity,
Codex, Gemini CLI, Goose, and Kiro.

### `auth`, `login`, `logout`, `status`

Use auth commands for GitHub access:

```bash
npx octocode auth status --json
npx octocode auth login
npx octocode auth refresh
npx octocode auth logout
npx octocode status --sync
```

Humans can run `login` once. Agents and CI should usually pass
`OCTOCODE_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN` through the environment.

### `lsp-server`

Semantic search depends on language servers. Inspect or pre-warm them with:

```bash
npx octocode lsp-server list
npx octocode lsp-server status src/main.rs
npx octocode lsp-server install rust-analyzer
npx octocode lsp-server install --all
```

Use this when `search --op ...` reports that an LSP server is unavailable.

### `context`

Print the research protocol and tool descriptions:

```bash
npx octocode context
npx octocode context --full
npx octocode context --json
```

Use this for autonomous agents, debugging tool guidance, or producing a compact
machine-readable context block.

## Recommended Workflows

### Local Code

```bash
npx octocode search ./src --tree --depth 2
npx octocode search "parseArgs" ./src --lang ts --view discovery
npx octocode search ./src/cli/parser.ts --match-string "parseArgs" --content-view exact
```

### Remote Repo To Local Proof

GitHub code search can return zero rows when a provider has not indexed a repo
or path. Treat that as provider evidence, not proof of absence.

```bash
npx octocode search facebook/react/packages/react --tree --depth 2
npx octocode search "useState" packages/react --repo facebook/react --materialize required --lang js
npx octocode cache fetch facebook/react packages/react --depth tree
npx octocode search "useState" <localPath-from-cache-output> --lang js
```

### Symbols And References

Get line anchors first, then trace the symbol:

```bash
npx octocode search ./src/index.ts --op documentSymbols
npx octocode search ./src/index.ts --op references --symbol runCLI --line 42
```

### Package To Source

```bash
npx octocode search zod --target packages
npx octocode search "ZodObject" colinhacks/zod --lang ts
```

### Pull Requests And History

```bash
npx octocode search bgauryy/octocode --target pullRequests --state merged --limit 10
npx octocode search bgauryy/octocode#123 --target pullRequests --patches --comments
npx octocode search bgauryy/octocode/packages/octocode/src --target commits --since 2024-01-01T00:00:00Z
```

### Archives And Binaries

```bash
npx octocode search app.zip --target artifacts --list
npx octocode search app.zip --target artifacts --extract src/index.ts
npx octocode search lib.node --target artifacts --inspect
npx octocode search lib.node --target artifacts --strings --min-length 12
```

### Agent Or Script Mode

```bash
npx octocode context --json
npx octocode tools --json --compact
npx octocode tools localSearchCode --scheme --json --compact
npx octocode tools localSearchCode --queries '{"path":"./src","keywords":"runCLI"}' --json --compact
```

## Output, Flags, And Exit Codes

Common flags:

| Flag | Meaning |
|---|---|
| `--help` | Show command help. |
| `--version` | Show CLI version. |
| `--json` | Print structured JSON output. |
| `--compact` | Print leaner output for agents and scripts. |
| `--raw` | Print bare file content where supported. |
| `--no-color` | Disable ANSI color. `NO_COLOR=1` works too. |

Exit codes:

| Code | Meaning |
|---:|---|
| `0` | Success. |
| `1` | General error. |
| `2` | Invalid input or unsupported flags. |
| `3` | Not found: unknown command/tool, missing symbol, or empty semantic result. |
| `4` | Authentication failure. |
| `5` | Tool or API execution error. |
| `7` | Rate limited. |

Environment variables:

| Variable | Meaning |
|---|---|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data and cache location. |
| `ENABLE_LOCAL` | Enable local filesystem tools. Defaults to `true`. |
| `ENABLE_CLONE` | Enable clone/materialization. CLI clone is enabled by default unless explicitly set to `false`; MCP clone tools require `true`. |
| `NO_COLOR` | Disable terminal color. |

## From Older Shortcuts To `search`

Older read-only shortcut names are intentionally folded into `search`.

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
| `binary` | `search <file> --target artifacts` |
| `diff` | `search <left> <right> --target diff` |

## Further Reading

- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md)
- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
- [MCP Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [GitHub Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#github-tools-reference)
- [Local Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#local-code-tools-reference)
- [LSP Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#lsp-tools-reference)
- [Binary Tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md#binary-tools-reference)
