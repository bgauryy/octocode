# Octocode CLI

`octocode` exposes all 16 research tools from the Rust engine used by MCP, plus setup and management commands. Tools use the same input contract on both interfaces. The npm package is a launcher; native owns validation, behavior, output and exit codes.

## Quick start

```bash
npx -y octocode --help
npx -y octocode schema localSearch --view query
npx -y octocode localSearch '{"queries":[{"path":"src","matchString":["createServer"],"wholeWord":true}]}'
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

## Commands

### Tool commands

Use `octocode <tool> '<JSON envelope>'`. All 16 tools are listed and enabled in the CLI, including `clasify`, `ghCloneRepo`, `graphBuild` and `graphQuery`. MCP catalog filters and opt-ins do not remove CLI commands. A call that needs a missing credential, provider or language server reports that failure at invocation; filesystem policy and storage requirements still apply. See [configuration and MCP availability](CONFIGURATION.md#tool-availability-cli-and-mcp), and the [tools reference](OCTOCODE_TOOLS.md) for each tool's behavior.

Each CLI call starts a new process. For many repeated calls, one long-lived `octocode mcp` session runs the same tools without that per-call startup.

### System commands

`schema`, `config`, `auth`, `mcp`, `install`, `skill`, `cache` and `lsp-server` own their command-specific options. Run their help for available operations. These options do not create aliases for research-tool fields.

### Clear tool caches

Stop separate MCP processes and other tool calls, then run:

```bash
octocode cache clear --json
```

The command drains and stops warm CLI servers, then removes GitHub response,
registry, outline, judgment, saved-page, continuation, section-assessment,
materialization and URL-capture caches under the configured home. It also removes
the current workspace's `.octocode/graph/.facts` extraction checkpoints. Repeat
from each workspace whose graph checkpoints you want removed. The JSON receipt
lists removed directories and `restartMcp: true`: separate MCP processes own their
RAM and must restart to drop it. An active process can otherwise recreate files.

Clones, published graph snapshots, credentials, settings, language-server installs,
rate-limit coordination and unrelated home files stay. Saved continuation handles
stop working. This explicit command can delete existing cache files even when
`storage.mode` is `memory`; changing the setting alone never deletes them. A
failure to drain a warm server or remove a directory reports an error.

## Flags

Research inputs are JSON envelopes. Use `--input <file>` for saved requests and `--json` for structured output. There are no positional query shorthands, field flags, short aliases or path-and-line mini-languages.

### Schema discovery

```bash
octocode schema
octocode schema localFetch --view query
octocode schema localFetch --view variants
```

The CLI catalog lists all tools; the query view describes authored input; the full view includes variants and rules. MCP clients inspect their server's `tools/list` for its configured subset. Defaults are owned by the contract. Discovery never requires copying private progress state.

### The JSON form

```bash
octocode localFetch '{"queries":[{"path":"src/index.ts"}],"responseChars":12000}'
```

Copy an emitted continuation's `input` as the entire next request:

```bash
octocode localFetch '{"cursor":"<issued cursor>"}'
```

## Output

### Text output

Text mode renders readable evidence on stdout and guidance on stderr, on a terminal or a pipe, for every tool. A single-row search, listing or outline prints one line per hit, file or declaration (`path:line:text`, `path:count  [term count …]`); any other result prints every field as indented `key: value` lines, a multi-line value as a `key: |` block. A batch prints each row under `## results[N]` and prefixes that row's stderr notes the same way. Nothing is clipped. Emitted next steps use the same JSON input as MCP. Use JSON mode for programmatic reconstruction and exact row metadata.

### Reading a JSON result

`results[]` carries the original query `index`, `data` and an exceptional status when relevant. Paging state lives only in the issued cursor; no request sets a page. Complete `responsePagination.next` before the tool-level `next.*` calls. `related[]` starts new work; `hints[]` contains prose. [Data and handoff contract](TOOL_DATA_CONTRACT.md) owns shapes, paths, evidence and continuation semantics.

### Errors

Input failures report `errorCode:"invalidInput"` and the invalid fields. Correct those fields using the schema; the CLI does not guess an alternative query. JSON mode writes structured failures to stdout. A launcher that cannot start native reports the failure on stderr.

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | Success, including more pages |
| 1 | Every row empty or out of range |
| 2 | Invalid input |
| 3 | Not found |
| 4 | Authentication or permission failure |
| 5 | Execution failure or unavailable saved state |
| 7 | Rate limited |
| 130 | Interrupted |

A batch reports its most severe outcome; inspect every row. Exit zero does not prove complete evidence coverage; follow required continuations before making an all-or-none claim.

## `ghCloneRepo` — materialize a GitHub repository

```bash
octocode ghCloneRepo '{"queries":[{"owner":"bgauryy","repo":"octocode"}]}'
```

Use the returned path with local tools. Clone storage requires persistent mode; configuration owns [cache storage](CONFIGURATION.md#cache-storage-and-lifecycle).

## Code graph: `graphBuild` and `graphQuery`

The graph tools are ordinary tool commands with the standard envelope; the CLI and
MCP take the same input and cut the same pages. CLI calls do not need beta.

```bash
octocode graphBuild --json '{"queries":[{"path":"."}]}'      # build or reuse a snapshot
octocode graphQuery --json '{"queries":[{"operation":"stats","path":"."}]}'
octocode graphQuery --json '{"queries":[{"operation":"find","path":".","target":"create server"}]}'
octocode graphQuery --json '{"queries":[{"operation":"impact","path":".","since":"origin/main"}]}'
octocode graphQuery --json '{"queries":[{"operation":"issues","path":"."}]}'
octocode graphQuery --json '{"queries":[{"operation":"unresolved","path":".","target":["src/a.ts","src/b.ts"],"reasons":["not-declared-in-repo"]}],"responseChars":50000}'
```

- `graphBuild` runs in the calling process and prints its progress on stderr;
  `graphQuery` runs on the warm server, so later pages reuse the decoded snapshot.
- Follow `responsePagination.next`, then `next.nextPage` and the other `next.*`
  continuations, passing each `input` back unchanged.
- `octocode schema graphQuery --view variants` lists every operation's fields.
  Operations, `issues` detectors and tiers, `impact` and recipes:
  [code graph](../packages/octocode-native/docs/engine/CODE_GRAPH.md#agent-graph-tools-cli-and-beta-mcp).

---

## `install` — MCP client setup

```bash
octocode install --list                              # every client: status and config path
octocode install --ide cursor
octocode install --ide cursor --scope workspace      # this project's .cursor/mcp.json
octocode install --ide claude-code --scope local     # this directory only, in ~/.claude.json
octocode install --ide claude-code --check
octocode install --ide claude-desktop --force
octocode install --ide cursor --dry-run              # preview; --json for the entry
octocode install --ide cursor --remove               # delete the entry (backup kept)
```

Clients: Cursor, Claude Desktop, Claude Code, Windsurf, Trae, Antigravity, VS
Code Cline/Roo/Continue, Zed, OpenCode, Gemini CLI, Kiro, Codex. Use an
exact id; a partial id such as `claude` is rejected with the ids it could mean.

`install --list [--scope …]` shows each client's entry: `installed`, `disabled`,
`custom` (an `octocode` entry install did not write), `absent`, `invalid`
(unreadable config) or `unsupported` (no such scope), with the config path.
`--scope` picks the file: `home` (default, the user config), `workspace` (the
project file under the current directory: Cursor, Claude Code, Continue, Kiro,
Codex) or `local` (Claude Code's per-directory section of `~/.claude.json`).

`--remove` deletes the entry and keeps the previous file as a rolling backup;
no entry is a successful no-op, and a `custom` entry needs `--force`. Other
flags: `--method npx|bunx|pnpm`, `--enable-local true|false`, `--pass-env`
(copies `GITHUB_TOKEN` and `OCTOCODE_ENABLE_LOCAL` from this shell into the
entry, so the client config stores the token in plain text), `--rollback
<file.bak>`. Without `--ide` on a terminal, the CLI lists clients with their
status and asks for a number. Press Enter to cancel. Piped or JSON invocations
require `--ide`.

---

## `auth` — GitHub authentication

```bash
octocode auth status --json     # authenticated, username, tokenSource
octocode auth login             # OAuth device flow (--hostname for GitHub Enterprise)
octocode auth login --refresh   # exchange the stored refresh token
octocode auth logout            # delete stored Octocode credentials
```

People run `auth login` once. Agents and CI set `GH_TOKEN` or `GITHUB_TOKEN`,
which take priority over stored logins. Resolution order, storage, refresh and
GitHub Enterprise: [authentication](AUTHENTICATION.md).

---

## `config` — settings and keys

```bash
octocode config --json                                # paths, key names, warnings
octocode config home                                  # Octocode home directory
octocode config get OCTOCODE_BETA                     # resolved value; exit 1 when unset
octocode config set OCTOCODE_BETA true                # or a setting path: config set local.beta true
octocode config set OCTOCODE_CLASSIFICATION_API --stdin
octocode config check OCTOCODE_CLASSIFICATION_API     # exit 0 set, 1 unset; never prints the value
octocode config unset OCTOCODE_BETA
octocode config view                                  # local settings page
```

- `set`, `unset` and `check` take an environment key or its setting path
  (`classification.api`); `set` and `unset` edit the global `.env` only. They
  keep unrelated lines, lock and replace the file atomically, and never print
  values. Internal limits such as timeouts, retries and
  page sizes are not settings.
- `get` masks secrets as `[REDACTED-<KEY>]` and names where the value lives.
  `check --json` on an unset GitHub token key adds `githubTokenSource`, the
  credential GitHub calls use instead.
- Process variables and a workspace `.octocode/.env` still override a global
  value. Precedence, home-only keys, and every setting:
  [configuration](CONFIGURATION.md).

---

## `lsp-server` — language servers

```bash
octocode lsp-server list                 # managed downloads and install status
octocode lsp-server status src/main.rs   # which language and server a file resolves to
octocode lsp-server install rust-analyzer
octocode lsp-server install --all
```

Use it when `lspSearch` reports that a server is unavailable. Managed downloads
cover `rust-analyzer` and `clangd`. Built-in routes and custom servers:
[language servers](OCTOCODE_TOOLS.md#language-servers).

---

## `skill` — agent skills

The native executable embeds the public skills from this repository's
`skills/` directory. `install` copies a skill into the durable store
`$OCTOCODE_HOME/skills/<name>` atomically, then links each selected platform's
skill directory to that copy. Links never target an npm or `npx` cache.

```bash
octocode skill list
octocode skill info octocode-research
octocode skill install octocode-research --platform pi --global
octocode skill install --all --platform pi,cursor --global
octocode skill install octocode-research --platform codex --project-dir "$PWD"
octocode skill install --add ./skills/my-skill --platform claude --global
octocode skill check --json
octocode skill remove octocode-research --platform pi
```

| Flag | Meaning |
|---|---|
| `--platform pi,cursor,claude,codex,opencode,copilot,gemini,all` | Agent skill directories (`codex` is the shared `.agents/skills` location). `all` selects all seven. |
| `--global` / `--project-dir <dir>` | Link scope; use exactly one with `--platform`. |
| `--add <path>` | Install a local skill directory (or its `SKILL.md`). Not combinable with `--all`. The skill name is the one `name: token` frontmatter line (bare or double-quoted) and must match the folder; no YAML parser runs, so any other form (block scalar, single quotes, trailing comment) needs a name override: `skill install --add <path> <name>`. |
| `--path <dir>` | Use a custom store instead of `$OCTOCODE_HOME/skills`. Not combinable with `--platform`. |
| `--mode symlink\|copy\|auto` | Install strategy; `symlink` is the default, `copy` a portability fallback. |
| `--force` | install: replace differing content (kept by default). remove: also delete real directories. |
| `--upgrade` | Refresh changed bundled content in the store. A copy refreshes only while it still matches the previous content; other drift stays a conflict. |
| `--dry-run` | Preview `install`, `remove`, or `check --fix` without writing. |
| `--fix` | `check`: refresh stale or broken installs in place; never adds platforms. A failed repair exits 1. |
| `--workspace` | `check --fix`: also repair existing `<cwd>/.agents/skills` installations. |
| `--no-env` | `check`: skip environment readiness. |

`skill list` marks installs that differ from the bundled copy `[stale]` and
broken links `[broken]`. `skill check` reports `ok`, `stale`, `broken` or
`not-installed`, plus readiness: `needs-config` (a required setting is missing)
or `partial` (an optional setting would unlock more). A GitHub token from
`auth login` or a `gh` login counts as set. `check` exits 1 on stale, broken or
needs-config. Every skill command works from the native binary alone, without
Node, npm, network access, or another `octocode` on `PATH`.

---

## How the CLI aligns with MCP

Core owns one schema and vocabulary; config generates it; native validates and executes it. Both transports accept the same JSON envelopes and return the same evidence. The CLI lists all tool commands; MCP applies the [configured availability controls](CONFIGURATION.md#tool-availability-cli-and-mcp). The launchers contain no tool logic.
