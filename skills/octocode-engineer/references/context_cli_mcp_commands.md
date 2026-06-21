# Context — CLI ↔ MCP Command Map

This skill is **CLI-first**. The **octocode CLI** and **octocode MCP tools** use the same runners, but the CLI quick commands are easier to validate, easier to share in reports, and expose `--json`/`--compact` consistently. Use the MCP column only when the host provides MCP tools and the CLI is unavailable or explicitly requested.

Canonical, always-current CLI docs: **[docs/cli/REFERENCE.md](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)**. This file is the condensed map for engineering work; run `octocode <cmd> --help` for exact quick-command flags, and run `octocode tools <name> --scheme` before every raw-tool call. If the global `octocode` binary is missing, use `npx octocode <cmd>` for the same commands.

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
| Map a directory / repo tree | `octocode ls <path\|owner/repo> --depth N` | `localViewStructure` / `ghViewRepoStructure` | structure |
| Find files by name / size / mtime | `octocode find <q> [path] --regex/--size-greater/--modified-within` | `localFindFiles` | file metadata |
| Text / regex search | `octocode grep <kw> <path\|owner/repo> [--mode discovery]` | `localSearchCode` / `ghSearchCode` | content |
| **AST** structural search | `octocode grep <path> --pattern '<shape>'` / `octocode grep <path> --rule '<yaml>'` | `localSearchCode(mode:"structural")` | Octocode structural grep (local or clone-backed `--repo`) |
| Read / minify a file | `octocode cat <f> --mode symbols\|none --match-string` | `localGetFileContent` / `ghGetFileContent` | content |
| **Symbols** (file outline) | `octocode ls <file>` or `octocode ls <dir> --symbols` | `localGetFileContent(minify:"symbols")` or `lspGetSemantics(type:"documentSymbols")` | outline |
| **LSP** semantic nav | `octocode lsp <f> --type <t> --symbol S --line N` | `lspGetSemantics(type=…)` | LSP |
| PR list / deep-dive | `octocode pr <owner/repo[#N]> --concise`, then `--patches --file`, `--comments`, or `--deep` | `ghHistoryResearch` | history |
| Commit history | `octocode history <owner/repo/path>` | `ghHistoryResearch(type=commits)` | history |
| Discover repos | `octocode repo <keywords> --stars '>1000'` | `ghSearchRepos` | GitHub |
| Package → repo | `octocode pkg <package>` | `npmSearch` | npm |
| Smart repo research | `octocode search --query '{"target":"research",...}'` | OQL `target:"research"` | candidate reachability/dependency flow |
| Inspect archive / binary | `octocode binary <f> --list/--strings/--decompress` | `localBinaryInspect` | binary |
| Unpack archive to dir | `octocode unzip <archive>` | `localBinaryInspect(unpack)` | binary |
| Clone repo / subtree locally | `octocode clone <owner/repo[/path][@branch]>` | `ghCloneRepo` | clone (needs `ENABLE_CLONE=true`) |

Global CLI flags: `--json` (raw envelope), `--compact` (lean), `--concise` (paths/titles only for search), `--no-color`. Unknown flags are rejected with a near-miss suggestion.

---

## `grep --pattern/--rule` — structural search

AST shape search via Octocode structural grep. Structure-aware — comments and strings never false-match. It runs locally, including clone-backed `--repo` paths; for a GitHub repo, use `--repo`, `octocode clone`, or `cache fetch` first.

```
octocode grep <path> --pattern '<ast>'
octocode grep <path> --rule '<yaml>'
octocode grep <repo-relative-path> --repo <owner/repo[@ref]> --pattern '<ast>'
    --pattern <ast>      shape; metavars $X (one node), $$$ARGS (a list)
    --rule <yaml>        relational rule — not/inside/has/all/any (mutually exclusive with pattern)
    --type <ext>         ts, py, go, rs, …
    --context-lines <n>  context around each match (default 0)
    --max-matches <n>    per file
    --limit <n>          max files (default 10)
    --page <n> / --page-size <n>
    --json
```

MCP/raw-tool equivalent: `localSearchCode({ mode:"structural", pattern:"…" | rule:{…}, path, langType })`.

Pattern library (the former presets) and pitfalls: [context_ast_pattern_cookbook.md](./context_ast_pattern_cookbook.md).

---

## `lsp` — semantic navigation

```
octocode lsp <file> --type <type> --symbol <name> --line <n>
    --type   definition|references|callers|callees|callHierarchy|hover|typeDefinition|implementation
    --symbol <name>           required
    --line <n>                required — get a REAL value from grep/symbols/ast first; never guess
    --format structured|compact
    --context-lines <n>
    --depth <n>               call hierarchy depth
    --json
```

- File outline uses `octocode ls <file>` or `octocode ls <dir> --symbols`. For raw semantic document symbols, use `octocode tools lspGetSemantics --scheme` then `type:"documentSymbols"` — no `--line` needed.
- Semantic misses (`symbolNotFound`, `noReferences`, `noCalls`, …) exit `3` so shell scripts fail fast.
- `references`/`callers`/`implementation` are open-file-scoped — empty ≠ unused. Load the consumer file (MCP: batch a `documentSymbols`/`definition` query on it in the same call; CLI: `symbols`/`cat` it first), then re-query.
- `callers`/`callees`/`callHierarchy`: TS/JS/Go/Rust only. Python/C++ → use `references`.

---

## Reading less — minify by goal

`cat --mode` (CLI) / `minify` (MCP):

| Goal | mode | Notes |
|------|------|-------|
| Orient on an unknown file | `symbols` | line-numbered skeleton; never paginated |
| Read | `standard` (default) | strips comments / blank lines |
| Quote / diff exact text, get a `line` | `none` | raw; pair with `--match-string` → returns the real line number |

`grep --mode discovery` (paths only) before `paginated`/`detailed`. `--concise` for the leanest discovery list.

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

## What the native toolset does NOT do

The CLI/MCP find **shapes** (AST), **relationships** (LSP, imports), and Smart OQL can now produce candidate reachability/package-drift rows. They still do **not** compute framework-complete entrypoint graphs, dependency cycle clusters, coupling/instability metrics, complexity/Halstead/Maintainability-Index numbers, or a full multi-detector scan. For those, approximate with fan-in/fan-out counts (see [SKILL.md](../SKILL.md) §4 Architecture, metrics & graph) or use an external measurement tool from [context_external_measurement_tools.md](./context_external_measurement_tools.md) — and **flag in the artifact** when a claim rests on approximation.
