# Octocode CLI

The Octocode CLI is the terminal interface over the same research engine used by
the Octocode MCP server. One binary — `npx octocode` — exposes every research
tool under its canonical tool name, plus a small set of system commands.

```text
octocode <toolName> '<json>'   ──► same tool runner   ──► the MCP tool catalog
octocode MCP tool call         ──► same core runners  ──► GitHub, local, npm, LSP
```

CLI and MCP share logic, schemas, security, sanitization, and tool execution.
They are not separate implementations, and the CLI has no per-tool flag
wrappers or aliases: the tool name and the JSON query are the entire interface.

## Commands

### Tools — one command per tool

Each tool command takes one positional raw JSON query (or `--input <file>`),
plus `--compact` for single-line JSON (default output is indented JSON).

| Command | Purpose |
|---|---|
| `localSearch` | Text/regex search across local files. |
| `localFetch` | Read a local file: pagination, line ranges, match filtering, minification. |
| `astSearch` | Structural (ast-grep) search, declarations, syntax/filesystem trees, import graph. |
| `astRewrite` | Structural find-and-replace; previews before writing. |
| `lspSearch` | Definitions, references, hover, call/type hierarchy, diagnostics. |
| `ghSearch` | GitHub repository, code, and tree search. |
| `ghGetFileContent` | Read a GitHub file without cloning. |
| `ghSearchHistory` | Search PRs, issues, and commits. |
| `ghGetHistoryItem` | Read one PR, issue, commit, or comparison. |
| `ghCloneRepo` | Clone a repository into the local cache for offline analysis. |
| `artifactSearch` | Package lookup/discovery across 8 registries. |
| `semanticAssess` | Apply Noul, Choice, or Score questions across `resources[] × questions[]`, or batch independent matrices in `queries[]`. Requires `OCTOCODE_JEV_KEY`. |

### System

| Command | Purpose |
|---|---|
| `scheme [tool]` | No name: compact catalog of every tool with availability. With a name: the public input contract. `--view query` prints the self-contained query schema; `--select FIELD=VALUE` keeps one union branch. Output validation schemas remain internal. |
| `config` | Show config file paths and set key names — values are never printed. `--check <KEY>` tests one key; `--json`. |
| `auth` | GitHub auth status (default; `--json`). `auth login` (device flow; `--refresh`, `--force`, `--hostname`), `auth logout`. |
| `install` | Write or check MCP client configuration for supported IDEs and agent hosts. |
| `skill` | List, install, check, inspect, or remove bundled Octocode Agent Skills. |
| `help` | Print help for any command. |

Hidden maintenance commands (still available, not part of the agent surface):
`cache <status|clear>` and `lsp-server <list|install|uninstall|clean|status|which>`.

Use `npx octocode <command> --help` for the live command help.

## Quick start

```bash
npx octocode --help
npx octocode auth --json
npx octocode scheme --compact
npx octocode scheme localSearch
npx octocode astSearch '{"operation":"tree","path":"/ABS/repo/src","reasoning":"Map the source tree."}'
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"createServer","resultView":"matchOnly","reasoning":"Locate the server entry."}'
npx octocode localFetch '{"path":"./src/index.ts","fullContent":true,"reasoning":"Read the entry file."}'
npx octocode skill list
npx octocode skill install octocode-research --platform pi --global
```

Replace `npx octocode` with `octocode` when the package is installed globally.

**Always read the schema before an unfamiliar call:**

```bash
npx octocode scheme <name>
npx octocode scheme <name> --view query                     # self-contained query schema
npx octocode scheme ghSearch --view query --select operation=code
```

For local tools, use absolute paths in agent or script calls. Relative paths
resolve from the command cwd, which may differ from the repository root.

| Category | Default enabled tools |
|---|---|
| GitHub | `ghSearch` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` |
| Local Code | `localSearch` · `astSearch` · `astRewrite` · `localFetch` · `lspSearch` |
| Package | `artifactSearch` |

`ghCloneRepo` is opt-in with `ENABLE_CLONE=true`. `semanticAssess` is available
when `OCTOCODE_JEV_KEY` is nonblank. Both remain discoverable in the CLI catalog.

### Research loop

```text
map cheaply → search narrowly → read exact evidence → follow symbols or history
```

```bash
npx octocode astSearch '{"operation":"tree","path":"/ABS/repo/crates/runtime/src","reasoning":"Map the runtime crate."}'
npx octocode localSearch '{"path":"/ABS/repo/crates/runtime/src","searchText":"ToolRuntime","resultView":"discovery","reasoning":"Find the runtime type."}'
npx octocode localFetch '{"path":"/ABS/repo/crates/runtime/src/runtime/engine.rs","matchString":"ToolRuntime","reasoning":"Read the definition site."}'
npx octocode lspSearch '{"uri":"/ABS/repo/crates/runtime/src/runtime/engine.rs","operation":"references","symbolName":"ToolRuntime","lineHint":40,"reasoning":"Trace usages."}'
```

`semanticAssess` requires a nonblank `reasoning` string. Ordinary tools accept
it as optional context and reject a supplied blank value. Queries accept a
single object or a JSON array for a batch (up to 5). Large queries avoid shell
quoting with `--input <file>`.

---

## `ghCloneRepo` — materialize a GitHub repository

```bash
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","reasoning":"Analyze locally."}'
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","sparsePath":"packages/next","reasoning":"Analyze one package."}'
```

Use `ghCloneRepo` when you need to inspect several files, run structural (AST)
search, or use LSP on remote code. Cloning is opt-in in both CLI and MCP with
`ENABLE_CLONE=true`. After cloning, run `localSearch`,
`localFetch`, or `lspSearch` on the returned absolute local path.

The CLI and MCP server share cache data under the configured Octocode home:

| Bucket | Path | Contents |
|---|---|---|
| Clone | `tmp/clone/{owner}/{repo}/{branch}` | Reusable Git checkouts |
| Tree | `tmp/tree/{owner}/{repo}/{commitSha}` | Materialized repository trees |
| Response | `tmp/response/` | Eligible GitHub and npm response payloads |

`cache status` reports cache location and recent evictions; `cache clear`
removes cached GitHub responses. See
[Cache storage and lifecycle](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#cache-storage-and-lifecycle).

---

## `install` — MCP client setup

```bash
npx octocode install --ide cursor
npx octocode install --ide claude-code --check
npx octocode install --ide claude-desktop --force
```

Supported clients: Cursor, Claude Desktop, Claude Code, Windsurf, Zed, VS Code
Cline/Roo/Continue, OpenCode, Trae, Antigravity, Codex, Gemini CLI, Goose, Kiro.

---

## `auth` — GitHub authentication

```bash
npx octocode auth --json
npx octocode auth login
npx octocode auth login --refresh
npx octocode auth logout
```

Humans: run `auth login` once. Agents and CI: pass `OCTOCODE_TOKEN`,
`GH_TOKEN`, or `GITHUB_TOKEN` through the environment.

---

## `lsp-server` — language server management (maintenance)

```bash
npx octocode lsp-server list
npx octocode lsp-server status src/main.rs
npx octocode lsp-server install rust-analyzer
npx octocode lsp-server install --all
```

Use when `lspSearch` reports an LSP server is unavailable.

`lsp-server list` reports the managed-download servers, the
toolchain-required servers, and a note naming packaged servers. It does not list
every resolve-if-installed command. Use `lsp-server status FILE_PATH` to inspect
the complete resolution ladder for one extension, including overrides,
project-local executables, packaged servers, ecosystem locations, and managed
downloads.

Managed installation supports `rust-analyzer` and `clangd`. Built-in routes
cover 11 families: JavaScript, TypeScript, Python, Rust, Go, Java, C, C++, CUDA,
C#, and Scala. CUDA uses `clangd`. Assembly parsing is first-class, but semantic
navigation requires an explicit trusted `.octocode/lsp-servers.json` entry
because there is no truthful generic built-in Assembly server route. Other
extensions likewise require explicit trusted configuration.

---

## `skill` — agent skills

The `octocode` package bundles the complete canonical Octocode skill suite from
this repo's `skills/` directory at build/publish time. Install can use a bundled
skill or `--add <local-or-GitHub-source>`. It atomically materializes a durable
copy under `$OCTOCODE_HOME/skills/<name>`, then optionally links agent-specific
skill directories to that copy. Links never target an npm or `npx` cache.

```bash
npx octocode skill list
npx octocode skill info octocode-research
npx octocode skill install octocode-research --platform pi --global
npx octocode skill install --all --platform pi,cursor --global
npx octocode skill install octocode-research --platform codex --project-dir "$PWD"
npx octocode skill install --add octocodeai/octocode/skills/octocode-research
npx octocode skill check --json
npx octocode skill remove octocode-research --platform pi
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--platform pi,cursor,claude,codex,opencode,copilot,gemini,shared,common,agents,claude-desktop,codex-native,all` | Select agent skill directories. `claude-desktop` maps to `claude`; shared/common/agents/codex-native map to the current Codex `.agents/skills` location. `all` selects the seven distinct destinations. |
| `--global` | Install selected platform links in user scope. Use exactly one scope with `--platform`. |
| `--project-dir <dir>` | Install selected platform links in project scope. |
| `--add <source>` | Install a skill from a local path or GitHub source. |
| `--path <dir>` | Use a custom canonical skill root instead of `$OCTOCODE_HOME/skills`. |
| `--mode symlink\|copy\|auto` | Install strategy. `symlink` is the default; `copy` is an explicit portability fallback. |
| `--force` | Replace existing canonical or destination content that differs. Existing content is preserved by default. |
| `--upgrade` | Refresh changed bundled content in the canonical store. Managed copies refresh only when they still match the previous canonical content; arbitrary destination drift remains a conflict. |
| `--dry-run` | Preview actions without writing. |
| `--fix` | `check` only: repair missing/broken installed locations. |
| `--no-env` | `check` only: skip skill environment-readiness checks. |

Skill actions use the subcommands above; removed flag forms are rejected with
the canonical command syntax.

---

## Recommended workflows

### Orient in a local codebase

```bash
npx octocode astSearch '{"operation":"tree","path":"/ABS/repo/src","reasoning":"Map the tree."}'
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"parseArgs","resultView":"discovery","reasoning":"Find the parser."}'
npx octocode localFetch '{"path":"/ABS/repo/src/cli/parser.ts","matchString":"parseArgs","reasoning":"Read the parser."}'
```

### Remote repo to local proof

GitHub code search can return zero rows when a provider has not indexed a repo.
Treat that as a provider gap, not proof of absence.

```bash
npx octocode ghSearch '{"operation":"tree","owner":"vercel","repo":"next.js","path":"packages/next","reasoning":"Browse the package."}'
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","sparsePath":"packages/next","reasoning":"Materialize for search."}'
npx octocode localSearch '{"path":"<clone localPath>/src","searchText":"useState","resultView":"matchOnly","reasoning":"Prove the usage."}'
```

### Symbols and references

Get line anchors first, then trace the symbol:

```bash
npx octocode lspSearch '{"uri":"/ABS/repo/src/index.ts","operation":"documentSymbols","reasoning":"List anchors."}'
npx octocode lspSearch '{"uri":"/ABS/repo/src/index.ts","operation":"references","symbolName":"runCLI","lineHint":42,"reasoning":"Trace callers."}'
```

### Package to source

```bash
npx octocode artifactSearch '{"type":"npm","packageName":"zod","reasoning":"Locate the package."}'
npx octocode ghSearch '{"operation":"code","keywords":["ZodObject"],"owner":"colinhacks","repo":"zod","reasoning":"Find the source."}'
```

### Pull requests and history

```bash
npx octocode ghSearchHistory '{"operation":"pullRequests","owner":"bgauryy","repo":"octocode","state":"merged","pageSize":10,"reasoning":"Survey merged PRs."}'
npx octocode ghGetHistoryItem '{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","number":123,"content":{"patches":{"mode":"all"},"comments":{"discussion":true}},"reasoning":"Read PR 123."}'
npx octocode ghSearchHistory '{"operation":"commits","owner":"bgauryy","repo":"octocode","path":"packages/octocode/src","since":"2024-01-01T00:00:00Z","reasoning":"Find recent commits."}'
npx octocode ghGetHistoryItem '{"operation":"compare","owner":"bgauryy","repo":"octocode","base":"v1.0.0","head":"v2.0.0","reasoning":"Diff releases."}'
```

### Agent or script mode

```bash
npx octocode scheme --compact
npx octocode scheme localSearch --view query --compact
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"runCLI","resultView":"matchOnly","reasoning":"Locate the entry."}' --compact
npx octocode semanticAssess --input request.json
```

The CLI keeps `semanticAssess` discoverable when the provider key is absent. If
called without a nonblank `OCTOCODE_JEV_KEY`, it exits with an actionable error
that names the variable and tells the caller to set it. MCP instead omits the
tool from discovery until the key is available.

---

## Output, flags, and exit codes

### Common flags

| Flag | Meaning |
|---|---|
| `--help` | Show command help. |
| `--version` | Show CLI version. |
| `--compact` | Single-line JSON (default is indented JSON). |
| `--input <file>` | Read a tool's JSON query from a file. |
| `--json-errors` | Emit `{"success":false,"error":"…"}` on stdout instead of stderr text. |
| `--no-color` | Disable ANSI color. `NO_COLOR=1` works too. |

### Exit codes

| Code | Meaning |
|---:|---|
| `0` | Successful execution, including a typed empty semantic payload. |
| `1` | Empty result / no matches. |
| `2` | Invalid input or unsupported flags. |
| `3` | A command or tool execution failed with a classified not-found error. |
| `4` | Authentication failure. |
| `5` | Tool or API execution error. |
| `6` | Partial result — the response carries a re-runnable `next.*` continuation. |
| `7` | Rate limited. |
| `130` | Interrupted (Ctrl-C). |

### Environment variables

| Variable | Meaning |
|---|---|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data and cache location. |
| `ENABLE_LOCAL` | Enable local filesystem tools. Defaults to `true`. |
| `ENABLE_CLONE` | Enable clone/materialization. Defaults to `false` on CLI and MCP. |
| `TOOLS_TO_RUN` | Strict allowlist for CLI and MCP tools. |
| `DISABLE_TOOLS` | Remove named tools from the default set when `TOOLS_TO_RUN` is unset. |
| `NO_COLOR` | Disable terminal color. |

`ghSearch` and `localSearch` are the only discovery entry points. A nonempty
allowlist replaces the default set, so include every tool that the CLI or MCP
client must retain. Removed compatibility names are rejected.

---

## How the CLI aligns with MCP

| CLI surface | MCP alignment |
|---|---|
| `<toolName> '<json>'` | Direct terminal access to the same named tools exposed through MCP, with identical query contracts. |
| `scheme <name>` | The schema contract for that tool. Do not guess fields. |
| `scheme` | The tool catalog with availability — the same tool set MCP clients see. |
| `install --ide <client>` | Writes MCP client configuration so editors and assistants can call `octocode-mcp`. |
| `auth` | Manages credentials used by both CLI and MCP flows. |
| `skill` | Installs bundled Agent Skills locally; no MCP transport required. |

The code boundary is intentionally thin:
- `@octocodeai/octocode-native` owns tool schemas, descriptions, and execution logic.
- `@octocodeai/octocode-core` supplies reusable output types.
- `@octocodeai/octocode-native/engine` exposes native primitives (minify, structural search, LSP, secret scanning) from the internal engine crate.
- `octocode` launches the native binary in a terminal.
- `octocode-mcp` registers the same tools for MCP clients.

---

## Further reading

- [Authentication Setup](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [MCP Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [All tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
