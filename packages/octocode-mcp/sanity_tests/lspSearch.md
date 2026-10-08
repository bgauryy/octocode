# Runtime check — `lspSearch`

Semantic navigation over local files: definitions, references, callers,
callees, hover, symbols, implementations, type hierarchy, and diagnostics.
Use the built CLI or the MCP tool with the same queries; check the live
operations with `octocode schema lspSearch --view variants`.

- Anchor a query on a prior search hit: `path` + `symbolName` + `lineHint`.
  `definition` and `references` resolve the intended symbol, and
  `references` includes the declaration.
- Run `documentSymbols` and `diagnostic` with `path` only, and
  `workspaceSymbol` with `symbolName` + `workspaceRoot`.
- Request references or callers on a widely used symbol with a small
  `pageSize`. Walk every `next.*` continuation; no location is dropped, and
  partial coverage (for example, a capped importer scan) is disclosed.
- Results carry exact paths and line anchors usable by `localFetch`.
- Errors distinguish a missing language server, an invalid path, and zero
  results.

```json
{"queries":[{"path":"/ABS/repo/src/index.ts","operation":"documentSymbols"}]}
```
