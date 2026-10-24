# Octocode CLI

`octocode` runs the same native tools and input contract as MCP. The npm package is a launcher; native owns validation, behavior, output and exit codes.

## Quick start

```bash
npx -y octocode --help
npx -y octocode schema localSearch --view query
npx -y octocode localSearch '{"queries":[{"path":"src","matchString":["createServer"],"wholeWord":true}]}'
npx -y octocode localFetch '{"queries":[{"path":"src/index.ts","ranges":[{"startLine":1,"endLine":40}]}]}'
```

## Commands

### Tool commands

Use `octocode <tool> '<JSON envelope>'`. All 14 tool names are listed in the [tools reference](OCTOCODE_TOOLS.md). `ghCloneRepo` is CLI-only; public clasify requires beta and a classification credential. Authenticated GitHub endpoints require a token; inspect `octocode auth status`.

### System commands

`schema`, `config`, `auth`, `mcp`, `install`, `skill`, `graph` and `lsp-server` own their command-specific options. Run their help for available operations. These options do not create aliases for research-tool fields.

## Flags

Research inputs are JSON envelopes. Use `--input <file>` for saved requests and `--json` for structured output. There are no positional query shorthands, field flags, short aliases or path-and-line mini-languages.

### Schema discovery

```bash
octocode schema
octocode schema localFetch --view query
octocode schema localFetch --view full
```

The catalog describes availability; the query view describes authored input; the full view includes variants and rules. Defaults are owned by the contract. Discovery never requires copying private progress state.

### The JSON form

```json
{"queries":[{"path":"src/index.ts"}],"responseChars":12000}
```

Copy an emitted continuation's `input` as the entire next request:

```bash
octocode localFetch '{"cursor":"<issued cursor>"}'
```

## Output

### Text output

Text mode renders readable evidence on stdout and guidance on stderr. Emitted next steps use the same JSON input as MCP. Use JSON mode for programmatic reconstruction and exact row metadata.

### Reading a JSON result

`results[]` carries the original query `index`, `data` and an exceptional status when relevant. Pagination uses `page`. Complete `responsePagination.next` before the tool-level `next.*` calls. `related[]` starts new work; `hints[]` contains prose. [Data and handoff contract](TOOL_DATA_CONTRACT.md) owns shapes, paths, evidence and continuation semantics.

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

## `graph` — persisted code graph

Parse a repository once, then answer structural questions in milliseconds
without re-reading source. Local tools must be enabled. Unlike tool commands,
`graph` takes positional arguments and flags. `graph --help`,
`graph ingest --help` and `graph query --help` are the full reference.

```bash
octocode graph ingest .                         # build a snapshot (an unchanged tree is reused)
octocode graph query stats                      # size, languages, hubs, gaps
octocode graph query find createServer          # locate a node id
octocode graph query callers 'src/server.ts#createServer'
octocode graph query impact --since origin/main # blast radius and tests to run
octocode graph query issues                     # ranked possible problems
octocode graph query stale                      # changed since ingest? re-ingest
```

| Question | Op |
|---|---|
| What is in this repo? | `stats`, `hubs`, `symbols <file>` |
| Where is X? | `find <text> [--kind file\|symbol\|package]`, `node <ref>` |
| What does X use, or who uses X? | `deps`, `dependents`, `callers`, `callees` |
| How are A and B connected? | `path <a> <b>`, `walk <ref> --depth n` |
| What breaks if X changes? | `impact <ref>`, `impact --changed a,b`, `impact --since <rev>` |
| What looks wrong? | `issues`, `cycles`, `diagnostics` |
| What changed between two trees? | `drift --graph <head> --baseline <baseline>` (ingest both first) |
| Is the snapshot current? | `stale` |

- Node ids: `src/a.ts` (file), `src/a.ts#Class.method` (symbol), `pkg:react`
  (package). An ambiguous reference exits `2` and lists `candidates`.
- A cut list adds `truncated` and `next`, a paste-ready command; the exit stays
  `0`. An answer over a snapshot whose scan was cut carries
  `graph.partialReasons` and a `graph.reingest` command.
- `ingest --language-glob <lang>=<glob>` (repeatable) picks a parser by
  root-relative glob, such as `cpp=include/**/*.h`.
- `dependents <file>` also lists files that use its exports through a
  re-exporting module (rows with `edge: uses, via: reexport`).
- Answers that follow calls carry `coverage.callInternalRecall`. Below 0.9 they
  add a `warning`: treat those caller lists as a lower bound.
- Results are leads from syntax. Confirm identity with `lspSearch` before
  deleting or renaming. Reflection, dynamic dispatch and FFI edges are invisible.

How ingest links calls, the snapshot format, and size bounds:
[code graph architecture](../packages/octocode-native/docs/engine/CODE_GRAPH.md).

### `issues` — ranked hypotheses

Each finding carries `evidence`, the false-positive `controls` applied,
`verify` commands, and a `score` (severity × confidence × (0.5 + 0.5 × PageRank
percentile)).

| Group | Detectors |
|---|---|
| Structure | `cycle` (runtime imports; witness plus `suggestedCuts`) · `dir-cycle` · `unreachable-file` · `test-only` · `unused-export` (JS/TS/Python) · `export-only-local` (exported but used only in its own file) |
| Dependencies | `undeclared-dependency` (including hoisted phantoms) · `dev-dependency-in-production` · `boundary-violation` (deep imports between npm packages) · `unresolved-import` |
| Architecture | `god-file` · `critical-file` (PageRank) · `single-point-of-failure` (articulation points) · `unstable-dependency` · `main-sequence` · `misplaced-file` |

- Any edge keeps code alive. Only runtime imports between authored files prove
  cycles, hubs, or layering; type-only, dynamic, and Python function-level
  imports never do.
- Test edges (from test files, or Rust `#[cfg(test)]` items such as `mod
  tests`) never count toward production detectors, `cycles` or `hubs`;
  traversals (`deps`, `walk`, `impact`) still follow them. A snapshot from an
  older format asks for a re-ingest.
- `dir-cycle` ignores a Rust module and its own child directory (`tree.rs` or
  `tree/mod.rs` with `tree/child.rs`, and a crate root with its modules): one
  module tree is no directory coupling.
- `unused-export` and `export-only-local` judge only languages with
  named-import linking; files in other languages are counted in a `notes`
  entry by language instead of being reported clean.
- Go, JVM and .NET files share package scope, so file-level checks skip them.
  Python libraries skip reachability. Generated, bundled, vendored and unparsed
  files are never subjects.
- Each finding has a `tier`: `100` nothing references it; `90` no graph or code
  reference (bare prose words are listed in `evidence.textMentions`); `60` the
  file path, or every unused export, is referenced in code or qualified text
  such as `mod.run` (`evidence.mentionedIn`; some exports alone keep 90), or
  no mention scan ran. The default shows 90 and above; `--min-tier 60` shows
  all, and `summary.hiddenBelowTier` counts the rest.
- `--baseline <snapshot>` labels findings `new` or `existing` and lists
  `resolvedFindings`. `summary.health` reports Lakos NCCD.

### `impact` — blast radius

- A changed file affects every importer, transitively. A changed symbol affects
  only the `calls`, `uses` and `inherits` edges bound to it, plus importers that
  bind no name (`precision: "symbol"`). Barrel files pass changes through by
  symbol.
- Depth defaults to 3 (`--depth` widens it). Rows carry `depth`, `risk`
  (`direct` at depth 1, `likely` at 2, `transitive` at 3+), `packageDistance`,
  `confidence` and `typesOnly`.
- The summary (first page only) counts rows `byDepth` and lists
  `affectedEntrypoints`, `testsToRun`, `components`, and `testFunctions`
  (including Rust `mod tests` and pytest `test_*` functions). A list over 100
  entries previews 100, and `nextLists` pages the rest from entry 100.
- A manifest, lockfile, or tsconfig/pyproject/go.mod change affects everything
  below its directory; `allAffected` marks a root-level change.

### Recipes

| Goal | Walk |
|---|---|
| Who can reach a dangerous operation? | `impact <wrapper>` on the function that deletes, spawns or writes. `affectedEntrypoints` is the attack surface. |
| Unbounded recursion | `cycles --edge calls`; check each SCC that walks untrusted input for a depth bound. |
| Choke points | `issues --detector single-point-of-failure,critical-file`. |
| Is this cycle real? | `path <a> <b>` shows the witness; `issues --detector cycle` gives `suggestedCuts`. |
| Dead or orphaned code | `issues --detector unreachable-file,unused-export` (add `test-only` for files only tests reach; `--entry <ref,...>` names entrypoints), then `dependents <ref> --edge imports,uses,calls` before deleting. |

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
  values.
  with `invalidInput` (exit `2`). Internal limits such as timeouts, retries and
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

Core owns one schema and vocabulary; config generates it; native validates and executes it. Both transports accept the same JSON envelopes and return the same evidence. MCP lists only available tools and excludes CLI-only ghCloneRepo. The launchers contain no tool logic.
