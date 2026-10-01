# Octocode CLI

The Octocode CLI is the terminal interface over the same research engine used by
the Octocode MCP server. One binary — `npx octocode` — exposes every research
tool under its canonical tool name, plus a small set of system commands.

```text
octocode <toolName> '<json>'   ──► native Rust runtime ──► GitHub, local, packages, LSP
octocode-mcp tool call         ──► native Rust runtime ──► (same tools, same contracts)
```

CLI and MCP share logic, schemas, security, sanitization, and tool execution.
They are not separate implementations, and the CLI has no per-tool flag
wrappers or aliases: the tool name and the JSON query are the entire interface.

## Commands

### Tools — one command per tool

Each tool command takes one positional raw JSON query (or `--input <file>`)
and prints single-line JSON (`--pretty` indents).

| Command | Purpose |
|---|---|
| `localSearch` | Text/regex search across local files. |
| `localFetch` | Read a local file: pagination, line ranges, match filtering, minification. |
| `structureSearch` | Directory outlines and file discovery by name or metadata. |
| `astSearch` | Structural (ast-grep) search, declarations, and syntax trees. |
| `astTopology` | Dependency graph analysis for paths, cycles, reachability, dead code, and drift. |
| `astRewrite` | Structural find-and-replace; previews before writing. |
| `lspSearch` | Definitions, references, hover, call/type hierarchy, diagnostics. |
| `ghSearchRepo` | GitHub repository search. |
| `ghSearchCode` | GitHub indexed code search. |
| `ghStructure` | GitHub repository tree browsing. |
| `ghGetFileContent` | Read a GitHub file without cloning. |
| `ghSearchHistory` | Search PRs, issues, and commits. |
| `ghGetHistoryItem` | Read one PR, issue, commit, or comparison. |
| `ghCloneRepo` | Clone a repository into the local cache for offline analysis. |
| `artifactSearch` | Package lookup/discovery across 8 registries. |
| `clasify` | Apply Noul, Choice, or Score questions across `resources[] × questions[]`, or batch independent matrices in `queries[]`. Requires a classification key (`OCTOCODE_CLASSIFICATION_API`, or `OCTOCODE_JEV_KEY` for the default jev vendor). |

### System

| Command | Purpose |
|---|---|
| `scheme [tool]` | No name: compact catalog of every tool with availability. With a name: the full input contract (default `--view full`). `--view variants` lists branch names, selectors, and examples; `--view query` prints the self-contained query schema, and `--select FIELD=VALUE` (or `variant=NAME`, `--view query` only) keeps one union branch. Output validation schemas remain internal. |
| `showConfig` | Print the global `.env` path; `--json` includes file existence. Honors `OCTOCODE_HOME`. |
| `config` | Inspect config paths and key names; `--check KEY`, `--add KEY VALUE`, `--remove KEY`, and `--json`. Values are never printed. |
| `auth` | GitHub auth: `status` (default; `--json`), `login` (device flow; `--refresh`, `--force`, `--hostname`, `--json`), `logout`. See [`auth`](#auth--github-authentication). |
| `install` | Write or check MCP client configuration for supported IDEs and agent hosts. |
| `skill` | List, install, check, inspect, or remove bundled Octocode Agent Skills. |
| `graph` | `graph ingest <path>` builds a persisted code graph under `<workspace>/.octocode/graph/`; `graph query <op>` answers bounded questions from it. See [`graph`](#graph--persisted-code-graph). |
| `help` | Print help for any command. |

Hidden maintenance commands (still available, not part of the agent surface):
`cache <status|clear>` and `lsp-server <list|install|uninstall|clean|status|which>`.

Use `npx octocode <command> --help` for the live command help.

Bare `npx octocode` and `npx octocode scheme` include agent instructions in the catalog's `instructions` field. Root `--help`, `-h`, and `help` show the same instructions after command usage. Instructions come from the shared MCP contract and reflect runtime tool availability; classification guidance is omitted when classification is disabled.

## Quick start

```bash
npx octocode --help
npx octocode auth --json
npx octocode scheme --compact
npx octocode scheme localSearch
npx octocode structureSearch '{"operation":"tree","path":"/ABS/repo/src","goal":"Map the source tree.","reasoning":"Map the source tree."}'
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"createServer","resultView":"matchOnly","goal":"Locate the server entry.","reasoning":"Locate the server entry."}'
npx octocode localFetch '{"path":"./src/index.ts","fullContent":true,"goal":"Read the entry file.","reasoning":"Read the entry file."}'
npx octocode skill list
npx octocode skill install octocode-research --platform pi --global
```

Replace `npx octocode` with `octocode` when the package is installed globally.

Choose the tool first, then load its schema only if it is unfamiliar or missing
from context. Use the query view for calls; select a known operation to omit
unrelated branches. The full view contains one complete input schema for the request envelope; the query view contains one standalone query schema. Output schemas remain internal.

```bash
npx octocode scheme <name> --view variants                  # branch names, selectors, examples
npx octocode scheme <name> --view query                     # self-contained query schema
npx octocode scheme ghSearchHistory --view query --select operation=commit
npx octocode scheme <name> --view full                      # default: descriptions, examples, rules
```

For local tools, use absolute paths in agent or script calls. Relative paths
resolve from the command cwd, which may differ from the repository root.

| Category | Default enabled tools |
|---|---|
| GitHub | `ghSearchRepo` · `ghSearchCode` · `ghStructure` · `ghGetFileContent` · `ghSearchHistory` · `ghGetHistoryItem` |
| Local Code | `localSearch` · `structureSearch` · `astSearch` · `localFetch` · `lspSearch` |
| Package | `artifactSearch` |

`ghCloneRepo` is available in the CLI with persistent storage. `astRewrite` and
`astTopology` are beta tools and require `OCTOCODE_BETA=true` or
`local.beta:true`. `clasify` is available when a classification key resolves
(`OCTOCODE_CLASSIFICATION_API`, else `OCTOCODE_JEV_KEY`; a present-but-blank
`OCTOCODE_CLASSIFICATION_API` disables it). All four remain discoverable in the
CLI catalog with availability metadata (`availability.enabled` and the gating
`envVar`). MCP omits unavailable tools and never registers `ghCloneRepo` or
`astRewrite`.

### Research loop

```text
map cheaply → search narrowly → read exact evidence → follow symbols or history
```

```bash
npx octocode structureSearch '{"operation":"tree","path":"/ABS/repo/crates/runtime/src","goal":"Map the runtime crate.","reasoning":"Map the runtime crate."}'
npx octocode localSearch '{"path":"/ABS/repo/crates/runtime/src","searchText":"ToolRuntime","resultView":"matchOnly","goal":"Find the runtime type.","reasoning":"Find the runtime type."}'
npx octocode localFetch '{"path":"/ABS/repo/crates/runtime/src/runtime/engine.rs","matchString":"ToolRuntime","goal":"Read the definition site.","reasoning":"Read the definition site."}'
npx octocode lspSearch '{"uri":"/ABS/repo/crates/runtime/src/runtime/engine.rs","operation":"references","symbolName":"ToolRuntime","lineHint":40,"goal":"Trace usages.","reasoning":"Trace usages."}'
```

Every query requires nonblank `goal` (what the query must find) and
`reasoning` (why it advances the goal) strings; a `next.*` continuation
already carries the brief of the query that produced it. Queries accept a single object, a JSON
array, or `{"queries":[…]}` for a batch (up to 5). Large queries avoid shell
quoting with `--input <file>`.

---

## `ghCloneRepo` — materialize a GitHub repository

```bash
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","goal":"Analyze locally.","reasoning":"Analyze locally."}'
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","sparsePath":"packages/next","goal":"Analyze one package.","reasoning":"Analyze one package."}'
```

Use `ghCloneRepo` when you need to inspect several files, run structural (AST)
search, or use LSP on remote code. Cloning is CLI-only and requires persistent
storage. After cloning, run `localSearch`,
`localFetch`, or `lspSearch` on the returned absolute local path.

The CLI and MCP server share cache data under the configured Octocode home:

| Bucket | Path | Contents |
|---|---|---|
| Clone | `tmp/clone/{owner}/{repo}/{branch}` | Reusable Git checkouts |
| Tree | `tmp/tree/{owner}/{repo}/{commitSha}` | Materialized repository trees |
| Response | `tmp/response/` | Eligible GitHub and npm response payloads |

`cache status` prints the cache directory and recent evictions; `cache clear`
removes cached GitHub responses. See
[Cache storage and lifecycle](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md#cache-storage-and-lifecycle).

---

## `graph` — persisted code graph

Parse a repository **once**, then answer structural questions in milliseconds
without re-reading source. Local tools must be enabled. `octocode graph --help`
prints this workflow for agents.

```bash
npx octocode graph ingest .                         # build a snapshot
npx octocode graph query stats                      # size, languages, hubs, gaps
npx octocode graph query find createServer          # locate a node id
npx octocode graph query callers 'src/server.ts#createServer'
npx octocode graph query impact --since origin/main # blast radius + tests to run
npx octocode graph query issues                     # ranked possible problems
npx octocode graph query stale                      # changed since ingest? re-ingest
```

### Which op answers what

| Question | Op |
|---|---|
| What is in this repo? | `stats`, `hubs`, `symbols <file>` |
| Where is X? | `find <text> [--kind file\|symbol\|package]`, `node <ref>` |
| What does X use or who uses X? | `deps <ref>`, `dependents <ref>`, `callers <ref>`, `callees <ref>` |
| How are A and B connected? | `path <a> <b> [--direction both]`, `walk <ref> --depth n` |
| What breaks if X changes? | `impact <ref>`, `impact --changed a,b`, `impact --since <rev>` |
| What looks wrong? | `issues [--detector a,b] [--min-score x] [--baseline <snapshot>]`, `cycles`, `diagnostics` |
| Is the snapshot current? | `stale` |

On files, `deps` and `dependents` follow imports; on symbols, they follow calls.
`--edge contains,imports,uses,calls,inherits` overrides either default.

### References, output, and exit codes

- **Node ids:** `src/a.ts` (file), `src/a.ts#Class.method` (symbol), and
  `pkg:react` (package). An `@<line>` suffix appears only when a name repeats.
  Absolute paths, bare names, and `Class.method` also resolve. An ambiguous
  reference exits 2 and lists `candidates`.
- **Output:** one JSON object; use `--pretty` for humans. Lists carry `total`
  and `results`. A cut adds `truncated` and `next`, a paste-ready command. Use
  `--limit` and `--offset` to page.
- **Exit codes:** `0` ok · `1` empty · `2` bad input or ambiguous · `3` no graph
  or node · `5` error · `6` more pages.
- **Snapshots:** `ingest` writes
  `<workspace>/.octocode/graph/<UTC time>-<scope>/{graph.bin,manifest.json}`
  and updates `latest`. It keeps 3 snapshots per scope (`--keep`). The
  workspace is the nearest `.git` ancestor, or `--workspace`. `--graph <id,
  id substring, or dir>` picks an older snapshot.
- **Unchanged trees are reused:** when the file set and every file's content
  digest match the latest snapshot of the same scope, `ingest` returns it with
  `reused: true` instead of re-parsing. `--force` rebuilds.
- **Coverage:** answers that follow calls (`callers`, `callees`, `impact`,
  `path`, and `walk`/`deps`/`dependents` over calls) include `coverage`. It
  carries the language's `callInternalRecall`: the share of call sites naming
  code in this repo that were linked. Below 0.9 it adds a `warning`, so treat
  those caller lists as a lower bound.

### What ingest understands

- **Nodes and edges:** file, symbol, and package nodes. `contains`, `imports`,
  `uses` (named import → declaration, through re-exports), `calls` (including
  JSX `renders`, decorators, and `new`), and `inherits` (`extends` and
  `implements`).
- **Resolution:**
  - JS/TS: tsconfig/jsconfig `paths`, `baseUrl`, and `extends`; workspace
    packages, with `exports`/`imports` maps and `dist` mapped back to `src`.
  - Rust: Cargo crates.
  - Go: go.mod modules.
  - Python: src-layout packages and submodules.
  - C/C++: include roots, `compile_commands.json`, and unique path-suffix matches.
- **Evidence:** every call edge records `via` and a `confidence`:
  - `local`, `import`, `namespace`: the name is bound in this file.
  - `type-qualified`: `Type.method` / `Type::method` resolved through the type.
  - `same-package` / `import-scope`: an ambiguous name settled by the caller's
    package or its imports.
  - `unique-name`: the only declaration with that name (low confidence).

  Calls qualified by an external package (`serde_json::…`, `fs.…`) never link
  to internal code. Method calls on a value of unknown type stay unlinked
  rather than guessed. The ingest receipt reports `calls.unresolvedByReason`
  and `callInternalRecall`. Confirm identity with `lspSearch` before deleting
  or renaming.
- **Project model:**
  - File roles: `test`, `generated`, `bundled`, `vendored`, `declaration`,
    `config`, `entry`, `unparsed`.
  - Entrypoints: manifests (`dist` mapped to `src`), framework routes,
    `main`s, shebang scripts, and C translation units.
  - Components: package.json, Cargo.toml, go.mod, and pyproject. Package
    imports are tagged `external`, `-dev`, `-builtin`, `-hoisted`, `-test`,
    `-undeclared`, or `-unknown`.
- **Large files:**
  - Files over 1 MB are not parsed. They stay as `unparsed` file nodes, and
    relative imports of them still link (`via: unparsed-target`).
  - Minified or bundled files under the bound keep their file node and imports
    but no symbols, and their call sites are not linked.
  - A 3.6 MB minified bundle plus a 9 MB file ingest in about 70 ms at 91 MB RSS.
- `.gitignore` is honored. Hidden directories are never scanned.

### `issues` — ranked hypotheses

Each finding carries `evidence`, the false-positive `controls` that were
applied, `verify` commands, and a `score`, computed as severity × confidence ×
(0.5 + 0.5 × PageRank percentile).

| Group | Detectors |
|---|---|
| Structure | `cycle` (runtime imports; witness plus Eades–Lin–Smyth `suggestedCuts`) · `dir-cycle` · `unreachable-file` · `test-only` · `unused-export` (JS/TS/Python) · `export-only-local` (exported but used only in its own file: could be un-exported) |
| Dependencies | `undeclared-dependency` (including hoisted phantoms) · `dev-dependency-in-production` · `boundary-violation` (deep imports between npm packages) · `unresolved-import` |
| Architecture | `god-file` (Arcan hub-like) · `critical-file` (PageRank) · `single-point-of-failure` (articulation points) · `unstable-dependency` · `main-sequence` · `misplaced-file` |

- **Liveness is optimistic and violations are pessimistic.** Any edge keeps
  code alive. Only runtime imports between authored files prove cycles, hubs,
  or layering; type-only, dynamic, and Python function-level imports never do.
- **Some code is not judged at file level.** Go, JVM, and .NET files share
  package scope, so file-level checks are skipped for them. Python libraries
  have no private modules, so reachability is skipped too. Generated, bundled,
  vendored, and unparsed files are never subjects.
- **Tiers (vulture-style):** each finding has a `tier`.
  - `100`: nothing references it anywhere.
  - `90`: no graph or text reference.
  - `60`: referenced only by text, such as a path string in a worker, config,
    or script (`evidence.mentionedIn`).

  The default view shows 90 and above; `--min-tier 60` shows everything.
  `summary.hiddenBelowTier` counts the rest. Dead-code findings are re-tiered by
  one `.gitignore`-aware scan of the repository's text (Meta SCARF's
  "mentioned anywhere" check).
- **Extra output:** `summary.health` reports Lakos NCCD. `summary.detectorMs`
  reports per-detector cost. `--baseline <snapshot>` labels findings
  `new`/`existing` and lists `resolvedFindings`.

### `impact` — blast radius

- **A changed file** affects every importer, transitively. This matches Jest
  `--findRelatedTests` and Nx `affected`.
- **A changed symbol** affects only the `calls`, `uses`, and `inherits` edges
  bound to it, plus importers that bind no name. These results report
  `precision: "symbol"`.
- **Depth:** 3 by default (`summary.maxDepth`; widen with `--depth`).
- **Barrels:** barrel files (re-export only) pass changes through by symbol,
  so importers of a barrel are affected only for the names they bind.
- **Rows:** `depth`, `risk` (`direct` / `likely` / `transitive`),
  `packageDistance`, `confidence`, and `typesOnly`.
- **Summary:**
  - `willBreak` (depth 1), `likely` (depth 2), `shouldTest` (depth 3 and beyond)
  - `affectedEntrypoints`, `testsToRun`, `components`
  - `testFunctions`: test functions reached, including Rust `mod tests`
    functions and pytest `test_*` functions inside production files
- **Config changes:** a manifest, lockfile, or tsconfig/pyproject/go.mod change
  affects everything below its directory. `allAffected` means a root-level
  change.

### Recipes: hunting bugs with the graph

These reverse-engineering walks (IDA/Ghidra xrefs, CodeQL-style source-to-sink
reachability) map onto the existing ops:

| Goal | Walk |
|---|---|
| Who can reach a dangerous operation? | Find the wrapper that deletes, spawns, or writes, then run `impact <wrapper>`. `affectedEntrypoints` is the attack surface. Wrappers with `testCount: 0` are untested destructive paths. |
| Unbounded recursion | `cycles --edge calls`. Check each SCC that walks untrusted input for a depth bound. |
| Choke points | `issues --detector single-point-of-failure,critical-file`. Validation belongs at the choke point. |
| Is this cycle real? | `path <a> <b>` shows the witness. `issues --detector cycle` gives `suggestedCuts`. |
| Dead or orphaned code | `issues --detector unreachable-file,unused-export`, then `dependents <ref> --edge imports,uses,calls` before deleting. |

Each result is a lead, not proof. Read the cited line, then confirm identity
with `lspSearch`. The graph cannot see edges created through reflection,
dynamic dispatch, or FFI.

### Performance

Release build, measured with `packages/octocode-native/scripts/graph-bench`:

| Repo | Files | Ingest | Snapshot | `issues` (wall) |
|---|---|---|---|---|
| excalidraw (TSX) | 694 | 0.3 s | 1.6 MB | 28 ms |
| django | 3,041 | 1.6 s | 10 MB | 52 ms |
| TypeScript compiler | 31,420 | 9.4 s | 52 MB | 0.27 s |
| rust-lang/rust | 38,197 | 12.7 s | 66 MB | 0.26 s |
| Linux (capped at 50k files) | 49,887 | 67 s | 797 MB | 3.0 s |

Other results:

- Ingest is deterministic: re-ingesting produces a byte-identical `graph.bin`.
- Sampled import and high-confidence call edges verify against their source
  lines at 89–100%.
- Seeded defects are recalled 5/5, with zero negative-control leaks.

---

## `install` — MCP client setup

```bash
npx octocode install --ide cursor
npx octocode install --ide claude-code --check
npx octocode install --ide claude-desktop --force
npx octocode install --ide cursor --dry-run   # print the config without writing
```

Supported clients (`install --list`): Cursor, Claude Desktop, Claude Code,
Windsurf, Zed, VS Code Cline/Roo/Continue, OpenCode, Trae, Antigravity, Codex,
Gemini CLI, Goose, Kiro. Other flags: `--method npx|bunx|pnpm`,
`--enable-local true|false`, `--backup` / `--rollback <file>`. Run
`octocode install --help` for the full list.

---

## `auth` — GitHub authentication

```bash
npx octocode auth --json            # same as `auth status --json`
npx octocode auth login             # OAuth device flow (interactive terminal)
npx octocode auth login --refresh   # exchange the stored refresh token
npx octocode auth logout            # delete stored Octocode credentials
```

Humans: run `auth login` once. Agents and CI: set a token variable
(`OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, …) in the environment; it takes
priority over stored logins. Resolution order, storage, refresh, GitHub
Enterprise, and troubleshooting are in
[Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md).

---

## `lsp-server` — language server management (maintenance)

```bash
npx octocode lsp-server list
npx octocode lsp-server status src/main.rs
npx octocode lsp-server install rust-analyzer
npx octocode lsp-server install --all
```

Use when `lspSearch` reports an LSP server is unavailable.

`lsp-server list` reports only the managed-download servers and their install
status. Use `lsp-server status FILE_PATH` to see which language and server a
file resolves to (overrides, project-local executables, packaged servers,
ecosystem locations, or managed downloads).

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
skill or `--add <local-path>` (a directory containing `SKILL.md`). It atomically materializes a durable
copy under `$OCTOCODE_HOME/skills/<name>`, then optionally links agent-specific
skill directories to that copy. Links never target an npm or `npx` cache.

```bash
npx octocode skill list
npx octocode skill info octocode-research
npx octocode skill install octocode-research --platform pi --global
npx octocode skill install --all --platform pi,cursor --global
npx octocode skill install octocode-research --platform codex --project-dir "$PWD"
npx octocode skill install --add ./skills/my-skill --platform claude --global
npx octocode skill check --json
npx octocode skill remove octocode-research --platform pi
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--platform pi,cursor,claude,codex,opencode,copilot,gemini,shared,common,agents,claude-desktop,codex-native,all` | Select agent skill directories. `claude-desktop` maps to `claude`; shared/common/agents/codex-native map to the current Codex `.agents/skills` location. `all` selects the seven distinct destinations. |
| `--global` | Install selected platform links in user scope. Use exactly one scope with `--platform`. |
| `--project-dir <dir>` | Install selected platform links in project scope. |
| `--add <path>` | Install a skill from a local directory (or its `SKILL.md`). Not combinable with `--all`. |
| `--path <dir>` | Use a custom canonical skill root instead of `$OCTOCODE_HOME/skills`. Not combinable with `--platform`. |
| `--mode symlink\|copy\|auto` | Install strategy. `symlink` is the default; `copy` is an explicit portability fallback. |
| `--force` | Replace existing canonical or destination content that differs. Existing content is preserved by default. |
| `--upgrade` | Refresh changed bundled content in the canonical store. Managed copies refresh only when they still match the previous canonical content; arbitrary destination drift remains a conflict. |
| `--dry-run` | Preview actions without writing (with `check --fix`, preview the fixes). |
| `--fix` | `check` only: refresh the canonical copy, relink broken or stale locations, and remove retired-skill installs. Never adds platforms (or the workspace, unless `--workspace`) and never replaces a fresh link. |
| `--workspace` | `check` only: also check the workspace `<cwd>/.agents/skills` directory. |
| `--no-env` | `check` only: skip skill environment-readiness checks. |

Skill actions use the subcommands above; removed flag forms are rejected with
the canonical command syntax.

`skill list` marks installs whose content differs from the bundled copy as
`[stale]` (and broken links as `[broken]`). `skill check` reports per-skill
`ok` / `stale` / `broken` / `not-installed` plus env readiness: `needs-config`
means a required setting is missing; `partial` means the skill works but an
optional setting would unlock more. A full `check` also lists installs of
retired skills (for example `octocode-clasify` → `octocode-research`) under
`retired`. `check` exits 1 on stale, broken, retired, or needs-config; run
`skill check --fix` to repair installs and remove retired ones.

---

## Recommended workflows

### Orient in a local codebase

```bash
npx octocode structureSearch '{"operation":"tree","path":"/ABS/repo/src","goal":"Map the tree.","reasoning":"Map the tree."}'
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"parseArgs","resultView":"matchOnly","goal":"Find the parser.","reasoning":"Find the parser."}'
npx octocode localFetch '{"path":"/ABS/repo/src/cli/parser.ts","matchString":"parseArgs","goal":"Read the parser.","reasoning":"Read the parser."}'
```

### Structure, blast radius, and risks (code graph)

```bash
npx octocode graph ingest /ABS/repo
npx octocode graph query dependents src/config.ts --depth 2   # who is affected
npx octocode graph query impact --since origin/main            # changed files -> tests to run
npx octocode graph query issues --min-score 0.3                # triage, then verify each lead
npx octocode lspSearch '{"operation":"references","uri":"/ABS/repo/src/config.ts","symbolName":"load","lineHint":12,"goal":"Prove the graph lead.","reasoning":"Prove the graph lead."}'
```

Use the graph for repo-wide structure (imports, callers, cycles, reachability,
blast radius). Use `lspSearch` to prove symbol identity before a destructive
change.

### Remote repo to local proof

GitHub code search can return zero rows when a provider has not indexed a repo.
Treat that as a provider gap, not proof of absence.

```bash
npx octocode ghStructure '{"owner":"vercel","repo":"next.js","path":"packages/next","goal":"Browse the package.","reasoning":"Browse the package."}'
npx octocode ghCloneRepo '{"owner":"vercel","repo":"next.js","sparsePath":"packages/next","goal":"Materialize for search.","reasoning":"Materialize for search."}'
npx octocode localSearch '{"path":"<clone localPath>/src","searchText":"useState","resultView":"matchOnly","goal":"Prove the usage.","reasoning":"Prove the usage."}'
```

### Symbols and references

Get line anchors first, then trace the symbol:

```bash
npx octocode lspSearch '{"uri":"/ABS/repo/src/index.ts","operation":"documentSymbols","goal":"List anchors.","reasoning":"List anchors."}'
npx octocode lspSearch '{"uri":"/ABS/repo/src/index.ts","operation":"references","symbolName":"runCLI","lineHint":42,"goal":"Trace callers.","reasoning":"Trace callers."}'
```

### Package to source

```bash
npx octocode artifactSearch '{"type":"npm","packageName":"zod","goal":"Locate the package.","reasoning":"Locate the package."}'
npx octocode ghSearchCode '{"keywords":["ZodObject"],"owner":"colinhacks","repo":"zod","goal":"Find the source.","reasoning":"Find the source."}'
```

### Pull requests and history

```bash
npx octocode ghSearchHistory '{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","state":"merged","pageSize":10,"goal":"Survey merged PRs.","reasoning":"Survey merged PRs."}'
npx octocode ghGetHistoryItem '{"operation":"pullRequest","owner":"bgauryy","repo":"octocode","number":123,"content":{"patches":{"mode":"all"},"comments":{"discussion":true}},"goal":"Read PR 123.","reasoning":"Read PR 123."}'
npx octocode ghSearchHistory '{"operation":"commit","owner":"bgauryy","repo":"octocode","path":"packages/octocode/src","since":"2024-01-01T00:00:00Z","goal":"Find recent commits.","reasoning":"Find recent commits."}'
npx octocode ghGetHistoryItem '{"operation":"compare","owner":"bgauryy","repo":"octocode","base":"v1.0.0","head":"v2.0.0","goal":"Diff releases.","reasoning":"Diff releases."}'
```

### Agent or script mode

```bash
npx octocode scheme --compact
npx octocode scheme localSearch --view query --compact
npx octocode localSearch '{"path":"/ABS/repo/src","searchText":"runCLI","resultView":"matchOnly","goal":"Locate the entry.","reasoning":"Locate the entry."}'
npx octocode clasify --input request.json
```

The CLI keeps `clasify` discoverable when the provider key is absent. Called
without a key, it exits `5` with a `missingConfiguration` error that names
`OCTOCODE_CLASSIFICATION_API` (and `OCTOCODE_JEV_KEY`). MCP instead omits the
tool from discovery until a key resolves.

---

## Global configuration

`octocode showConfig` prints `<HOME>/.octocode/.env`, or the `.env` under
`OCTOCODE_HOME` when overridden. It does not create or print the file.

```bash
octocode showConfig --json
octocode config --add OCTOCODE_BETA true
octocode config --add OCTOCODE_CLASSIFICATION_API --value-stdin
octocode config --check OCTOCODE_CLASSIFICATION_API --json
octocode config --remove OCTOCODE_BETA --json
```

`--add` replaces every assignment for the named key with one assignment.
`--value-stdin` reads a single line, up to 64 KiB, without placing the value in
command arguments. `--remove` is idempotent and only removes the global assignment.
Both preserve unrelated lines and comments, use a file lock and atomic replacement,
and never print values. On Unix, written files have owner-only permissions.
Symlinked config files and keys blocked by the shared dotenv policy are rejected.

Changes apply to subsequent commands. Existing process variables and applicable
project configuration can still override the global value; removing a global key
does not remove those overrides. `config --check KEY --json` returns a `set`
boolean and exits `1` when unset. The global mutation response reports `key`,
`action`, `path`, and `changed`.

---

## Output, flags, and exit codes

### Common flags

| Flag | Meaning |
|---|---|
| `--help` | Show command help. |
| `--version` | Show CLI version. |
| `--compact` | `scheme` only: single-line JSON (`scheme` is already compact when piped). Tool commands print single-line JSON by default and reject `--compact`. |
| `--pretty` | Indented JSON (tool output and `scheme`). |
| `--input <file>` | Read a tool's JSON query from a file. |
| `--json-errors` | Emit errors as `{"kind":"octocode.toolError","version":1,"error":"…"}` on stdout instead of stderr text — the same envelope as tool input-validation errors (`tool` and `details` when known). Covers argument/unknown-subcommand errors and `scheme`; exit codes are unchanged. |
| `--redact-emails` | Mask email addresses in GitHub tool output, such as commit authors. Same as `OCTOCODE_REDACT_EMAILS=true` or `output.redactEmails`. |
| `--no-color` | Disable ANSI color. `NO_COLOR=1` works too. |

### Exit codes

| Code | Meaning |
|---:|---|
| `0` | Successful execution, including a typed empty semantic payload. |
| `1` | Empty result / no matches. |
| `2` | Invalid input or unsupported flags, including any batch row rejected as `invalidInput` while other rows ran. |
| `3` | A command or tool execution failed with a classified not-found error. |
| `4` | Authentication or permission failure. |
| `5` | Tool or API execution error. |
| `6` | Partial result — the response carries a re-runnable `next.*`, `next.clasify`, or `responsePagination.next` continuation. |
| `7` | Rate limited. |
| `130` | Interrupted (Ctrl-C). |

For mixed batches, inspect every `results[].status`: exit `0` can include a
successful row alongside a runtime-error row. A rejected input row produces exit
`2`; a continuation produces exit `6`. Exit codes alone do not establish that
every row succeeded.

### Environment variables

The CLI and MCP server read the same settings. The ones most often set for
the CLI:

| Variable | Meaning |
|---|---|
| `OCTOCODE_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN` | GitHub token, in that priority order. See [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md). |
| `OCTOCODE_HOME` | Override Octocode data and cache location. |
| `ENABLE_LOCAL` | Enable local filesystem tools. Defaults to `true`. |
| `TOOLS_TO_RUN` | Strict allowlist for CLI and MCP tools. A nonempty allowlist replaces the default set. |
| `DISABLE_TOOLS` | Remove named tools from the default set when `TOOLS_TO_RUN` is unset. |
| `OCTOCODE_BETA` | Enable the beta tools `astTopology` and `astRewrite`. |
| `OCTOCODE_REDACT_EMAILS` | Mask email addresses in GitHub output (same as `--redact-emails`). |
| `NO_COLOR` | Disable terminal color. |

Unknown or removed tool names in `TOOLS_TO_RUN` / `DISABLE_TOOLS` are ignored,
not aliased. Every other setting is in the
[configuration reference](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md).

---

## How the CLI aligns with MCP

| CLI surface | MCP alignment |
|---|---|
| `<toolName> '<json>'` | Direct terminal access to the same named tools exposed through MCP, with identical query contracts. |
| `scheme <name>` | The schema contract for that tool. Do not guess fields. |
| `scheme` | The full tool catalog with availability. MCP clients see only the available tools, minus the CLI-only `ghCloneRepo` and `astRewrite`. |
| `install --ide <client>` | Writes MCP client configuration so editors and assistants can call `octocode-mcp`. |
| `auth` | Manages credentials used by both CLI and MCP flows. |
| `skill` | Installs bundled Agent Skills locally; no MCP transport required. |

The code boundary is intentionally thin:
- `@octocodeai/octocode-core` authors tool schemas, descriptions, and instructions; `@octocodeai/config` delivers them (plus generated types) to every interface.
- `@octocodeai/octocode-native` embeds that contract and owns validation and execution logic.
- `@octocodeai/octocode-native/engine` exposes native primitives (minify, structural search, LSP, secret scanning) from the internal engine crate.
- `octocode` launches the native binary in a terminal.
- `octocode-mcp` registers the same tools for MCP clients.

---

## Further reading

- [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)
- [Configuration](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)
- [MCP server](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
- [All tools](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
