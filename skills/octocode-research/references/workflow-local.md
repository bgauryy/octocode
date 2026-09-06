# Local research

Load when a checkout, local artifact, or resolved dependency is the evidence source. Use the router in `references/algorithm.md`; this reference owns local tool selection and completeness.

## Choose the missing evidence
A known file or path can skip discovery and orientation: read the relevant lines directly. Otherwise choose one starting point:
| Question | Tool and selection | Evidence |
|---|---|---|
| Names, strings, errors, configuration | `localSearch operation:"text"`, `searchText` | lexical candidates |
| Paths or directory layout | `localSearch operation:"files"` or `"tree"` | files inside the stated scope |
| Call/declaration/import shape | `localSearch operation:"structural"`, exactly one `pattern` or `rule` | AST syntax, not resolved identity |
| Exact behavior or quote | `localGetFileContent`, `minify:"none"`, bounded lines or match | source text |
| Symbol identity and use | `lspGetSemantics` | server-resolved definitions/references/callers |
| Relationships between files | `localAnalyzeGraph` | syntactic topology |

Use an absolute `path` and an explicit `operation` for local search. Text uses `searchText`, structural uses `pattern`/`rule`; do not mix fields. Use files/count views when bodies are unnecessary. Outline with `minify:"symbols"` only when useful; `standard` may rewrite text.

## AST and LSP
- AST: inspect diagnostics before relaxing a zero-match pattern. Incomplete or partial execution cannot prove absence. A `terminalLimit` requires narrowing/simplifying, while a returned continuation can recover a scan or display bound.
- `structural.query.rewritten` identifies a different executed pattern; report it and use that explicit pattern when repeating the search. Keep captures off unless needed and inspect compacted-match recovery.
- Anchored LSP queries need `uri` + `symbolName` + a real `lineHint` from search/read. `workspaceSymbol` needs a name; `documentSymbols` and `diagnostic` need a URI.
- Use `definition` for identity, `references` for uses, `callers`/`callees` for call flow, and `hover`/`implementation`/type queries for their specific questions.
- Inspect server capabilities, `warmup`, partial state, and pagination. Unsupported diagnostics are not a clean bill of health. For usage checks set `includeDeclaration:false`; zero references still needs entrypoint/export/runtime checks before deletion.

## Graph
| Question | Operation and inputs | Corroboration |
|---|---|---|
| Imports or importing files | `dependencies` / `dependents` + `file`; optional `depth` | exact edges; LSP for affected symbols |
| Connection between files | `path` + `file` + `target` | inspect returned edges |
| Cycles | `cycles` | distinguish type-only edges from runtime effects |
| Reachable files | `reachability`; optional `entrypoints`/`includeTests` | prefer explicit roots; verify inferred roots and exclusions |
| Dead-code candidates | `deadCode`; optional `entrypoints`/`includeTests` | exact imports, LSP, registrations, tests/build |

Set `path` to the relevant package/repository. Inspect `entrypointsResolved` and confidence when roots are inferred; unclassified files with no resolved roots are not proven unreachable. Inspect coverage: unresolved imports and unsupported dynamic or ambiguous CommonJS loaders weaken completeness. Results and diagnostics paginate independently; follow `diagnosticPage` continuations with their snapshot. `rustWorkspace:"syntax"` and `"cargo"` use different workspace evidence; Cargo metadata does not prove feature-dependent runtime reachability. Never delete from graph output alone.

## Validate
Inspect the installed/resolved version when access permits; honor repository restrictions on vendor/generated files. Package metadata and upstream source are alternatives, but record version gaps. After an edit, run the matching change/refactor verification; do not force AST, LSP, and graph into every lookup.

Next: for symbol/deletion proof use `references/code-research.md`; for upstream intent use `references/workflow-external.md`; for implementation use `references/workflow-change.md`.
