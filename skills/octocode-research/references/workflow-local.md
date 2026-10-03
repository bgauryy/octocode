# Local research

Load when a checkout, artifact, or resolved dependency is the evidence source.

A known file, path, or anchor skips discovery: search or read it directly.

| Question | Tool |
|---|---|
| names, strings, errors, config | `localSearch` (`path` + `searchText`; alternation `a\|b`; `regex` literal/rust/pcre2) |
| paths/layout | `structureSearch` `operation:"files"` (`names` globs; with `/` they match below `path`) or `"tree"` (`maxDepth`) |
| declarations/shape (syntax, not identity) | `astSearch` `symbols` (`name`: substring or list) or `match` (`pattern`, or `rule` as YAML string or object) |
| exact behavior/quote | `localFetch` (`matchString`, `ranges`, lines, `block:true` for the enclosing declaration) |
| symbol identity/uses | `lspSearch` |
| file relationships without graph or topology | LSP references + text |

Use files/count views when bodies are unnecessary. `localSearch` shows every hit when the total is small, else pages per file; read all hits in one `localFetch` with `matchString`. Run an invalid-regex `hints.repair` only after checking it still searches what you meant. `structureSearch files` walks in path order and stops at `limit` (`next.expandLimit`); its rows group by `dir` (path = `base`/`dir`/name, ` (n)` = bytes); `noIgnore`/`hidden` list ignored and dot entries before an absence claim. Quote and edit only `minify:"none"` text. Never pass view offsets as LSP positions.

## AST and LSP
- AST: `langType` is inferred per extension for directories. Inspect hints and diagnostics before relaxing a zero-match pattern; incomplete or partial execution cannot prove absence. `terminalLimit` means narrow the query.
- A `symbols` row's `name` + `line` are `lspSearch` `symbolName` + `lineHint` as-is.
- Anchored LSP needs `uri` plus `symbolName` and a real `lineHint` (or a 0-based `position`); `workspaceSymbol` needs `symbolName` + `uri`/`workspaceRoot`. An unresolved anchor returns `hints.readFile`.
- `definition` identity · `references` uses (per-file `byFile` rows `line:col text`; `groupByFile:true` → counts) · `callers` (direct: `byFile` rows `line:col in kind name start-end`)/`callees`/`callHierarchy` flow · `hover`/`implementation` types.
- Servers cover ts/js, py, rust, c/c++. Check `lsp.serverAvailable`, capabilities (`debug:true` receipt), `coverage`, and truncation; native fallback is syntactic. First calls pay a cold start; `OCTOCODE_LSP_PREWARM=1` starts the server from earlier local reads in a long-lived MCP session. Usage checks set `includeDeclaration:false`; zero references still needs entrypoint, export, and runtime checks.

## Graph
- `octocode graph query`: `callers`, `impact --since <rev>`, `cycles`, `issues`, `stale`; honor `coverage` and `tier`.
- `astTopology`: `dependencies`/`dependents`, `path`, `cycles`, `deadCode`, `drift`, and reachability with optional `entrypoints`. Same-package Java and re-exports count as edges.
- Check `entrypointsResolved`: unclassified files without resolved roots are not unreachable. Imports left unresolved and dynamic CommonJS loaders weaken coverage; coverage reports `diagnosticCounts`, and `next.nextDiagnostics` (`diagnosticPage`) lists the rows. `rustWorkspace` `syntax` vs `cargo` modes differ; Cargo metadata does not prove feature-gated reachability.
- Confirm decisive graph edges with `localFetch`/`lspSearch`; never delete from graph evidence alone.

Honor vendor/generated restrictions and the installed version.

Next: deletion proof → `code-research.md`; upstream intent → `workflow-external.md`.
