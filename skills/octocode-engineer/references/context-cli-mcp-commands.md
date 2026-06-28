# Context — CLI ↔ MCP Command Map

This skill is **CLI-first when MCP is not registered**. The **octocode CLI** and **octocode MCP tools** use the same runners. Use MCP directly when the host provides tools such as `localSearchCode`, `ghSearchCode`, `npmSearch`, `lspGetSemantics`, or `oqlSearch`; otherwise use the CLI because it is easy to validate, easy to share in reports, and exposes `--json`/`--compact` consistently.

Canonical, always-current CLI docs: **[docs/cli/REFERENCE.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)**. This file is the condensed map for engineering work; run `npx octocode <cmd> --help` for exact command flags, and run `npx octocode tools <name> --scheme` before every raw-tool call.

For OQL patterns, surface selection, `--repo` shortcut, graph/reachability algorithm, and evidence rules see **[workflow.md](./workflow.md)**.

---

## Transport probe

```bash
npx octocode --version          # CLI available
npx octocode context                # agent protocol/context
npx octocode tools                  # list raw runnable tools
npx octocode tools <name> --scheme  # schema for one raw tool on demand
npx octocode auth status --json     # narrow auth check
```

If `npx octocode` is unavailable but MCP tools are available, use the MCP column with the same evidence rules. If neither CLI nor MCP works, stop and ask the user to install/run the CLI with `npx octocode`, then authenticate with `npx octocode auth login` when GitHub access is needed, or register the MCP server as `{"octocode":{"command":"npx","type":"stdio","args":["@octocodeai/mcp@latest"]}}`. Do not substitute native search for Octocode research.

---

## Tool map

| Job | CLI command | MCP tool | Routes to |
|-----|-------------|----------|-----------|
| Map a directory / repo tree | `npx octocode search <path\|owner/repo> --tree --depth N` | `localViewStructure` / `ghViewRepoStructure` | structure |
| Find files by name / size / mtime | `npx octocode search <q> [path] --search path --regex/--size-greater/--modified-within` | `localFindFiles` through OQL `target:"files"` | file metadata |
| Text / regex search | `npx octocode search <kw> <path\|owner/repo> [--view discovery]` | `localSearchCode` / `ghSearchCode` through OQL `target:"code"` | content |
| **AST** structural search | `npx octocode search <path> --pattern '<shape>' --lang <lang>` / `npx octocode search <path> --rule '<yaml>' --lang <lang>` | `localSearchCode(mode:"structural")` through OQL `target:"code"` | Octocode structural search (local or clone-backed `--repo`) |
| Read / minify a file | `npx octocode search <f> --content-view symbols\|exact --match-string` | `localGetFileContent` / `ghGetFileContent` | content |
| **Symbols** (file outline) | `npx octocode search <file> --symbols` or `npx octocode search <file> --op documentSymbols` | `localGetFileContent(minify:"symbols")` or `lspGetSemantics(type:"documentSymbols")` | outline |
| **LSP** semantic nav | `npx octocode search <f> --op <t> --symbol S --line N` | `lspGetSemantics(type=…)` | LSP |
| PR list / deep-dive | `npx octocode search <owner/repo[#N]> --target pullRequests --concise`, then `--patches --file`, `--comments`, or `--deep` | `ghHistoryResearch` | history |
| Commit history | `npx octocode search <owner/repo/path> --target commits` | `ghHistoryResearch(type=commits)` | history |
| Discover repos | `npx octocode search <keywords> --target repositories --stars '>1000'` | `ghSearchRepos` | GitHub |
| Package → repo | `npx octocode search <package> --target packages` | `npmSearch` | npm |
| Smart repo research | `npx octocode search --query '{"target":"research",...}'` | OQL `target:"research"` | candidate reachability/dependency flow |
| Inspect archive / binary | `npx octocode search <f> --target artifacts --list/--strings/--decompress` | `localBinaryInspect` | binary |
| Unpack archive to dir | `npx octocode unzip <archive>` | `localBinaryInspect(unpack)` | binary |
| Clone repo / subtree locally | `npx octocode clone <owner/repo[/path][@branch]>` | `ghCloneRepo` | clone (needs `ENABLE_CLONE=true`) |

Current CLI commands: `search`, `unzip`, `clone`, `cache fetch|status|clear`, `tools`, `context`, `skill`, `install`, `auth`, `login`, `logout`, `status`, and `lsp-server`. Global CLI flags: `--json` (raw envelope), `--compact` (lean), `--concise` (paths/titles only for search), `--no-color`. Unknown commands or removed aliases are rejected; use `search` lanes instead of legacy grep/cat/ls/find/LSP/PR/package/repo/binary/diff shortcuts.

---

## `search --pattern/--rule` — structural search

AST shape search via Octocode structural search. Structure-aware — comments and strings never false-match. It runs locally, including clone-backed `--repo` paths; for a GitHub repo, use `--repo`, `npx octocode clone`, or `cache fetch` first.

```
npx octocode search <path> --pattern '<ast>' --lang <lang>
npx octocode search <path> --rule '<yaml>' --lang <lang>
npx octocode search <repo-relative-path> --repo <owner/repo[@ref]> --pattern '<ast>' --lang <lang>
    --pattern <ast>      shape; metavars $X (one node), $$$ARGS (a list)
    --rule <yaml>        relational rule — not/inside/has/all/any (mutually exclusive with pattern)
    --lang <ext>         ts, py, go, rs, …
    --context-lines <n>  context around each match (default 0)
    --max-matches <n>    per file
    --limit <n>          max files (default 10)
    --page <n> / --items-per-page <n>
    --json
```

MCP/raw-tool equivalent: `localSearchCode({ mode:"structural", pattern:"…" | rule:{…}, path, langType })`.

Pattern library (the former presets) and pitfalls: [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md).

---

## `search --op` — semantic navigation

```
npx octocode search <file> --op <type> --symbol <name> --line <n>
    --op     definition|references|callers|callees|callHierarchy|hover|typeDefinition|implementation
    --symbol <name>           required
    --line <n>                required — get a REAL value from search/symbols/ast first; never guess
    --format structured|compact
    --context-lines <n>
    --depth <n>               call hierarchy depth
    --json
```

- File outline uses `npx octocode search <file> --symbols` or `npx octocode search <file> --op documentSymbols`. For raw semantic document symbols, use `npx octocode tools lspGetSemantics --scheme` then `type:"documentSymbols"` — no `--line` needed.
- Semantic misses (`symbolNotFound`, `noReferences`, `noCalls`, …) exit `3` so shell scripts fail fast.
- `references`/`callers`/`implementation` are open-file-scoped — empty ≠ unused. Load the consumer file (MCP: batch a `documentSymbols`/`definition` query on it in the same call; CLI: use `search <file> --symbols` or `search <file> --content-view exact` first), then re-query.
- `callers`/`callees`/`callHierarchy`: TS/JS/Go/Rust only. Python/C++ → use `references`.

---

## `--repo` — remote as local

`search` accepts `--repo <owner/repo[@ref]>`. Materializes the repo or subpath under `.octocode`, runs the local tool against saved files, and returns `location` (absolute path). Reuse `location` with plain local `search` calls — files stay materialized.

```bash
npx octocode search "registerTool" packages/react --repo facebook/react --json --compact
npx octocode search src --repo owner/repo --pattern 'useMemo($$$ARGS)' --json   # AST on remote repo
npx octocode search "*.test.ts" . --repo owner/repo --search path --json
npx octocode search src/index.ts --repo owner/repo@main --content-view exact --json
```

For remote AST/structural search, materialize with `--repo`, `clone`, or `cache fetch` first; GitHub code search cannot evaluate AST predicates. Path argument is repo-relative when `--repo` is set.

---

## Raw tools and OQL

Use raw tools only when a quick command cannot express the needed selector:

```bash
npx octocode tools <name> --scheme
npx octocode tools <name> --queries '<json>' --json
```

Use OQL when one typed query should route across code/content/files/structure:

```bash
npx octocode search --scheme
npx octocode search --query '<oql-json>' --explain --json
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
```

For partial OQL targets (PRs, commits, artifacts, direct diffs, package/repo continuations), prefer quick commands or raw tools and say which fallback produced the evidence. `target:"research"` is also partial by design: it returns candidate reachability/package-drift rows, not deletion proof.

---

## Environment

| Variable | Meaning |
|----------|---------|
| `ENABLE_LOCAL` | local filesystem tools (default `true`) |
| `ENABLE_CLONE` | `clone` / `ghCloneRepo` (default `false`) |
| `OCTOCODE_TOKEN` → `GH_TOKEN` → `GITHUB_TOKEN` | GitHub token resolution order |
| `OCTOCODE_HOME` | data dir (default `~/.octocode`) |

Local tools are confined to `$HOME` (+ `ALLOWED_PATHS`); paths outside are rejected.

---

## Docs

- [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md)
