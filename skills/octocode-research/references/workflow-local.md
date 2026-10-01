# Local research

Load when a checkout, artifact, or resolved dependency is the evidence source.

A known file, path, or anchor skips discovery: search or read it directly.

| Question | Tool |
|---|---|
| names, strings, errors, config | `localSearch` (`path` + `searchText`; `regex` literal/rust/pcre2) |
| paths/layout | `structureSearch` `operation:"files"` (`names`) or `"tree"` (`maxDepth`) |
| declarations/shape (syntax, not identity) | `astSearch` `symbols` (`name`) or `match` (`pattern`/`rule`) |
| exact behavior/quote | `localFetch` (`matchString`, `ranges`, lines, `block:true` for the enclosing declaration) |
| symbol identity/uses | `lspSearch` |
| file relationships | `octocode graph` or beta `astTopology`; else LSP references + text |

Use files/count views when bodies are unnecessary; read all hits in one `localFetch` with `matchString`. Quote and edit only `minify:"none"`. Copy `next.continue` whole; never pass view offsets as LSP positions.

## AST and LSP
- AST: inspect diagnostics before relaxing a zero-match pattern; incomplete or partial execution cannot prove absence. `terminalLimit` means narrow the query. Reuse `structural.query.rewritten` when present.
- A `symbols` row's `name` + `line` are `lspSearch` `symbolName` + `lineHint` as-is.
- Anchored LSP needs `uri` plus `symbolName` and a real `lineHint` (or a 0-based `position`); `workspaceSymbol` needs `symbolName` + `uri`/`workspaceRoot`.
- `definition` identity · `references` uses · `callers`/`callees`/`callHierarchy` flow · `hover`/`implementation` types.
- Check server capabilities, `lsp.source`, `warmup`, truncation; native fallback is syntactic. Usage checks set `includeDeclaration:false`; zero references still needs entrypoint, export, and runtime checks.

## Graph
- `octocode graph query`: `callers`, `impact --since <rev>`, `cycles`, `issues`, `stale`; honor `coverage` and `tier`.
- `astTopology`: `dependencies`/`dependents`, `path`, `cycles`, `deadCode`, and reachability with optional `entrypoints`.
- Check `entrypointsResolved`: unclassified files without resolved roots are not unreachable. Imports left unresolved and dynamic CommonJS loaders weaken coverage; follow `diagnosticPage`. `rustWorkspace` `syntax` vs `cargo` modes differ; Cargo metadata does not prove feature-gated reachability.
- Graph evidence is syntactic: never delete from it alone.

Honor vendor/generated restrictions and the installed version.

Next: deletion proof → `code-research.md`; upstream intent → `workflow-external.md`.
