# Context — CLI ↔ MCP Command Map

This skill is **CLI-first**. The **octocode CLI** and **octocode MCP tools** use the same runners, but the CLI quick commands are easier to validate, easier to share in reports, and expose `--json`/`--compact` consistently. Use the MCP column only when the host provides MCP tools and the CLI is unavailable or explicitly requested.

Canonical, always-current CLI docs: **[docs/cli/REFERENCE.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)**. This file is the condensed map for engineering work; run `octocode <cmd> --help` for exact quick-command flags, and run `octocode tools <name> --scheme` before every raw-tool call. If the global `octocode` binary is missing, use `npx octocode <cmd>` for the same commands.

For OQL patterns, surface selection, `--repo` shortcut, graph/reachability algorithm, and evidence rules see **[workflow.md](./workflow.md)**.

---

## Transport probe

```bash
octocode --version              # CLI available
npx octocode --version          # no global install/PATH fallback
octocode context                # agent protocol/context
octocode tools                  # list raw runnable tools
octocode tools <name> --scheme  # schema for one raw tool on demand
```

If neither `octocode` nor `npx octocode` works but MCP tools are available, use the MCP column with the same evidence rules. If neither CLI path nor MCP works, stop and ask the user to install/run the CLI with `npx octocode` or register the MCP server. Do not substitute native search for Octocode research.

---

## Tool map

| Job | CLI command | MCP tool | Routes to |
|-----|-------------|----------|-----------|
| Map a directory / repo tree | `octocode search <path\|owner/repo> --tree --depth N` | `localViewStructure` / `ghViewRepoStructure` | structure |
| Find files by name / size / mtime | `octocode search <q> [path] --search path --regex/--size-greater/--modified-within` | `localFindFiles` through OQL `target:"files"` | file metadata |
| Text / regex search | `octocode search <kw> <path\|owner/repo> [--view discovery]` | `localSearchCode` / `ghSearchCode` through OQL `target:"code"` | content |
| **AST** structural search | `octocode search <path> --pattern '<shape>' --lang <lang>` / `octocode search <path> --rule '<yaml>' --lang <lang>` | `localSearchCode(mode:"structural")` through OQL `target:"code"` | Octocode structural search (local or clone-backed `--repo`) |
| Read / minify a file | `octocode search <f> --content-view symbols\|exact --match-string` | `localGetFileContent` / `ghGetFileContent` | content |
| **Symbols** (file outline) | `octocode search <file> --symbols` or `octocode search <file> --op documentSymbols` | `localGetFileContent(minify:"symbols")` or `lspGetSemantics(type:"documentSymbols")` | outline |
| **LSP** semantic nav | `octocode search <f> --op <t> --symbol S --line N` | `lspGetSemantics(type=…)` | LSP |
| PR list / deep-dive | `octocode pr <owner/repo[#N]> --concise`, then `--patches --file`, `--comments`, or `--deep` | `ghHistoryResearch` | history |
| Commit history | `octocode search <owner/repo/path> --target commits` | `ghHistoryResearch(type=commits)` | history |
| Discover repos | `octocode search <keywords> --target repositories --stars '>1000'` | `ghSearchRepos` | GitHub |
| Package → repo | `octocode search <package> --target packages` | `npmSearch` | npm |
| Smart repo research | `octocode search --query '{"target":"research",...}'` | OQL `target:"research"` | candidate reachability/dependency flow |
| Inspect archive / binary | `octocode search <f> --target artifacts --list/--strings/--decompress` | `localBinaryInspect` | binary |
| Unpack archive to dir | `octocode unzip <archive>` | `localBinaryInspect(unpack)` | binary |
| Clone repo / subtree locally | `octocode clone <owner/repo[/path][@branch]>` | `ghCloneRepo` | clone (needs `ENABLE_CLONE=true`) |

Global CLI flags: `--json` (raw envelope), `--compact` (lean), `--concise` (paths/titles only for search), `--no-color`. Unknown flags are rejected with a near-miss suggestion.

---

## `search --pattern/--rule` — structural search

AST shape search via Octocode structural search. Structure-aware — comments and strings never false-match. It runs locally, including clone-backed `--repo` paths; for a GitHub repo, use `--repo`, `octocode clone`, or `cache fetch` first.

```
octocode search <path> --pattern '<ast>' --lang <lang>
octocode search <path> --rule '<yaml>' --lang <lang>
octocode search <repo-relative-path> --repo <owner/repo[@ref]> --pattern '<ast>' --lang <lang>
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
octocode search <file> --op <type> --symbol <name> --line <n>
    --op     definition|references|callers|callees|callHierarchy|hover|typeDefinition|implementation
    --symbol <name>           required
    --line <n>                required — get a REAL value from search/symbols/ast first; never guess
    --format structured|compact
    --context-lines <n>
    --depth <n>               call hierarchy depth
    --json
```

- File outline uses `octocode search <file> --symbols` or `octocode search <file> --op documentSymbols`. For raw semantic document symbols, use `octocode tools lspGetSemantics --scheme` then `type:"documentSymbols"` — no `--line` needed.
- Semantic misses (`symbolNotFound`, `noReferences`, `noCalls`, …) exit `3` so shell scripts fail fast.
- `references`/`callers`/`implementation` are open-file-scoped — empty ≠ unused. Load the consumer file (MCP: batch a `documentSymbols`/`definition` query on it in the same call; CLI: use `search <file> --symbols` or `search <file> --content-view exact` first), then re-query.
- `callers`/`callees`/`callHierarchy`: TS/JS/Go/Rust only. Python/C++ → use `references`.

---

## `--repo` — remote as local

`search` accepts `--repo <owner/repo[@ref]>`. Materializes the repo or subpath under `.octocode`, runs the local tool against saved files, and returns `location` (absolute path). Reuse `location` with plain local `search` calls — files stay materialized.

```bash
octocode search "registerTool" packages/react --repo facebook/react --json --compact
octocode search src --repo owner/repo --pattern 'useMemo($$$ARGS)' --json   # AST on remote repo
octocode search "*.test.ts" . --repo owner/repo --search path --json
octocode search src/index.ts --repo owner/repo@main --content-view exact --json
```

AST/structural search on a remote repo **requires** `--repo` or a prior clone; GitHub code-search cannot evaluate AST predicates. Path argument is repo-relative when `--repo` is set.

---

## Raw tools and OQL

Use raw tools only when a quick command cannot express the needed selector:

```bash
octocode tools <name> --scheme
octocode tools <name> --queries '<json>' --json
```

Use OQL when one typed query should route across code/content/files/structure:

```bash
octocode search --scheme
octocode search --query '<oql-json>' --explain --json
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
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
