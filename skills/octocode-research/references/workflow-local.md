# Local research

Load when a checkout, local artifact, or resolved dependency is the evidence source. Use the router in `references/algorithm.md`; this reference owns local tool selection and completeness.

## Choose the missing evidence
For Markdown headings, code signatures, complete declaration bodies, and full-file requests, use [reading flows](reading-flows.md). Outline first only when orientation is unresolved; a known phrase or range skips that step.

A known file or path can skip discovery and orientation: read the relevant lines directly. Otherwise choose one starting point:
| Question | Tool and selection | Evidence |
|---|---|---|
| Names, strings, errors, configuration | `localSearch`, `searchText` | lexical candidates |
| Paths or directory layout | `astSearch operation:"files"` (`maxDepth` for layout) | files inside the stated scope |
| Call/declaration/import shape | `astSearch operation:"match"`, exactly one `pattern` or `rule` | AST syntax, not resolved identity |
| Exact behavior or quote | `localFetch`, `minify:"none"`, bounded lines or match | source text |
| Symbol identity and use | `lspSearch` | server-resolved definitions/references/callers |
| Relationships between files | `astTopology` (`astTopology` is beta: enabled only with `OCTOCODE_BETA=1`; otherwise use `lspSearch` references plus `localSearch`) | syntactic topology |

Use an absolute `path` for `localSearch`; it has no `operation` field. Text uses `searchText` with `regex:"literal"`, `"rust"`, or `"pcre2"`. Structural AST uses `astSearch operation:"match"` with exactly one `pattern` or `rule`; do not mix lexical and structural fields. Use files/count views when bodies are unnecessary. `localFetch` is exact by default and accepts path-only reads; choose a line range, `matchString`, or `fullContent` when needed. `matchRanges` describe the selected source context windows; `matchedLines` contains source anchors intersecting this page, and `selectedMatchCount` counts matching source lines across the selected view. `standard` and `symbols` are transformed views.

`localFetch` selects source first, applies the explicit `minify` view and redaction, then paginates. Use `chunkType:"lines"` (default), `offset:0`, and `chunkSize:100` for line chunks; use `chunkType:"bytes"` for UTF-8 byte chunks (default 16384 bytes). A line page also has a 16384-byte budget; oversized lines switch to byte chunks. Copy `next.continue.tool` and its entire `query`, including selectors and units. Do not infer offsets from source lines, string length, or requested limits. Pages stop at the end of the selected range/matched view. `totalLines` and `sourceBytes` describe the original file; `pagination.totalLines/totalBytes` describe the transformed, redacted view. `returnedBytes` is the actual chunk size. These view offsets are never LSP positions.

`matchString` uses source text; byte-context matching follows full-source redaction. Context defaults to five lines for line chunks or 256 UTF-8 bytes for byte chunks. Explicit `contextLines` or `contextBytes` overrides the context unit; never combine them. Copy continuations unchanged to preserve the selected context. Matches force `minify:"none"`; `minifyFallback` reports an ignored compact request or unavailable outline. `fullContent:true` requests an unpaged view within resource/security limits and cannot accompany chunk controls. A limit response provides a bounded recovery or an explicit terminal limit.

## AST and LSP
- AST: inspect diagnostics before relaxing a zero-match pattern. Incomplete or partial execution cannot prove absence. A `terminalLimit` requires narrowing/simplifying, while a returned continuation can recover a scan or display bound.
- Structural results should be interpreted from the returned matches and diagnostics; do not assume an automatic rewrite. If a current result explicitly exposes `structural.query.rewritten`, report that observed field and use it when repeating the search. Keep captures off unless needed and inspect compacted-match recovery.
- Anchored LSP queries need `uri` plus either `symbolName` and a real 1-based `lineHint`, or a zero-based UTF-16 `position`. `workspaceSymbol` needs `symbolName` and either `uri` or `workspaceRoot`; `documentSymbols` and `diagnostic` need a URI and no symbol anchor.
- Use `definition` for identity, `references` for uses, `callers`/`callees` for call flow, and `hover`/`implementation`/type queries for their specific questions.
- Inspect server capabilities, `lsp.source`, `warmup`, partial state, `truncated`/`terminalLimit`, and pagination. Native or graph-facts fallback is syntactic evidence, not semantic identity. Unsupported diagnostics are not a clean bill of health. For usage checks set `includeDeclaration:false`; zero references still needs entrypoint/export/runtime checks before deletion.

## Graph
| Question | Operation and inputs | Corroboration |
|---|---|---|
| Imports or importing files | `dependencies` / `dependents` + `file`; optional `depth` | exact edges; LSP for affected symbols |
| Connection between files | `path` + `file` + `target` | inspect returned edges |
| Cycles | `cycles` | distinguish type-only edges from runtime effects |
| Reachable files | `reachability`; optional `entrypoints`/`includeTests` | prefer explicit roots; verify inferred roots and exclusions |
| Dead-code candidates | `deadCode`; optional `entrypoints`/`includeTests` | exact imports, LSP, registrations, tests/build |

Set `path` to the relevant package/repository. Inspect `entrypointsResolved` (page 1 only; `entrypointsResolvedCount` on every page) and confidence when roots are inferred; unclassified files with no resolved roots are not proven unreachable. Inspect coverage: unresolved imports and unsupported dynamic or ambiguous CommonJS loaders weaken completeness. Results and diagnostics paginate independently; follow `diagnosticPage` continuations with their snapshot. `rustWorkspace:"syntax"` and `"cargo"` use different workspace evidence; Cargo metadata does not prove feature-dependent runtime reachability. Never delete from graph output alone.

## Validate
Inspect the installed/resolved version when access permits; honor repository restrictions on vendor/generated files. Package metadata and upstream source are alternatives, but record version gaps. After an edit, run the matching change/refactor verification; do not force AST, LSP, and graph into every lookup.

Next: for symbol/deletion proof use `references/code-research.md`; for upstream intent use `references/workflow-external.md`; for implementation use `references/workflow-change.md`.

`ghGetFileContent` uses the same selection and pagination fields as `localFetch`: `chunkType`, `offset`, `chunkSize`, and `next.continue`. Both default to exact content and paginate after selection, transformation, and redaction. Only resource identity differs: GitHub needs `owner`/`repo`/repo-relative `path` and optionally `branch`; local reads need an absolute `path`.
