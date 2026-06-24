# Sanity check - `lspGetSemantics`

> Semantic navigation over local files: definitions, references, callers,
> callees, call hierarchy, hover, symbols, implementations, and type hierarchy.
>
> **Manual runtime checklist** - run each step against the live MCP tool (or `benchmark/github/scripts/call-tool.mjs lspGetSemantics '<queries-json>'`) and tick the box.
> Automated schema-contract checks live in `tests/tools/all-tools.pagination-contract.test.ts`; cursor-uniformity in `tests/tools/all-tools.pagination.test.ts`.

## 1. Scheme
- [ ] Tool is registered and accepts the bulk envelope: `queries[]`, `responseCharOffset`, `responseCharLength`.
- [ ] `type` exposes the current enum: `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic`.
- [ ] `uri`, `workspaceRoot`, `symbolName`, and `lineHint` are documented as operation-specific anchors.
- [ ] A minimal valid query parses (see Example).

## 2. Pagination
- [ ] Reference/call hierarchy operations expose page/cursor metadata when result sets exceed the requested size.
- [ ] Follow the emitted continuation to the next page or narrowed operation.
- [ ] No semantic locations are silently dropped without diagnostics or continuation.

## 3. Quality
- [ ] Run `documentSymbols` on a known file and verify symbol names, kinds, ranges, and URI are correct.
- [ ] Use a prior search hit line as `lineHint` for `definition` or `references`; verify the result resolves the intended symbol.
- [ ] `workspaceSymbol` works from `workspaceRoot` without requiring `uri`.

## 4. Agent Effectiveness
- [ ] The output gives exact `uri` + line anchors usable by `localGetFileContent`.
- [ ] Errors distinguish missing language server, invalid URI, and zero results.
- [ ] OQL semantic rows emit executable `next.fetch` continuations.

## Example call
```json
{ "queries": [ { "uri": "/abs/path.ts", "type": "documentSymbols" } ] }
```
