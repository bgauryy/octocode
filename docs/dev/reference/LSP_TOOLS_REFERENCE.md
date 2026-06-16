# LSP Tools Reference

This is the canonical reference for Octocode's public Language Server Protocol tools.

Octocode exposes **one** public LSP tool:

| Tool | Use it for |
|------|------------|
| `lspGetSemantics` | Definitions, references, callers, callees, bidirectional call hierarchy, hover, document symbols, type definitions, and implementations. |

LSP tools are local-only. They require `ENABLE_LOCAL=true` and a file that exists on disk. Use `localSearchCode` first when you need a symbol `lineHint`.

## Workflow

1. Search textually with `localSearchCode` and capture the exact `lineHint`.
2. Query `lspGetSemantics` with `uri`, `type`, `symbolName`, and `lineHint`.
3. Page large symbol or call-flow results with `page` and `itemsPerPage`.
4. Run project lint, typecheck, and tests before claiming risky changes are fully verified.

## `lspGetSemantics`

Required fields:

| Field | Required | Notes |
|-------|----------|-------|
| `uri` or `filePath` | Yes | Absolute local file path. `filePath` is an alias for `uri`. |
| `type` | No | Defaults to `definition`. |
| `symbolName` | Yes except `documentSymbols` | Exact symbol text at the target line. |
| `lineHint` | Yes except `documentSymbols` | 1-based line number from search results. |

Optional fields:

| Field | Notes |
|-------|-------|
| `orderHint` | Disambiguates repeated symbol text on the same line. |
| `workspaceRoot` | Overrides automatic project-root detection. |
| `contextLines` | Adds source previews to call-flow results. Keep `0` unless previews are needed. |
| `page` | Result page for `documentSymbols` and call-flow results. |
| `itemsPerPage` | Semantic items per page. Defaults to `40` for `documentSymbols`, `10` for call-flow. Max `100`. |
| `depth` | Call-flow recursion depth. Keep `1` unless you need nested calls. |
| `includeDeclaration` | For `references`; defaults to `true`. |
| `groupByFile` | For `references`; adds per-file rollups. |

Semantic types:

| `type` | Best for | Output |
|--------|----------|--------|
| `definition` | Jumping from usage/import to declaration. | `payload.kind="definition"`, `locations[]`. |
| `references` | Blast radius for functions, types, variables, constants, classes. | `locations[]`, `totalReferences`, `totalFiles`, optional `byFile`. |
| `callers` | Static incoming calls to a callable symbol. | Compact `calls[]`, `summary.incomingCalls`, pagination. |
| `callees` | Static outgoing calls made by a callable symbol. | Compact `calls[]`, `summary.outgoingCalls`, pagination. |
| `callHierarchy` | Bidirectional call-flow snapshot. | Incoming and outgoing calls in one compact page. |
| `hover` | Quick type/signature/docs from the language server. | `markdown` or `text`. |
| `documentSymbols` | File outline and symbol inventory. | Compact `symbols[]`, `summary.kinds`, pagination. |
| `typeDefinition` | Declared type behind a symbol. | `locations[]`. |
| `implementation` | Concrete implementation behind an interface/abstract symbol when the server supports it. | `locations[]`. |

All semantic responses use this envelope:

| Field | Meaning |
|-------|---------|
| `type` | Requested semantic type. |
| `uri` | Resolved local file path. |
| `resolvedSymbol` | Symbol anchor for symbol-based requests. |
| `lsp` | Server availability and provider/source metadata. |
| `evidence` | Confidence, completeness, and reason when incomplete. |
| `summary` | Agent-readable totals for symbol and call-flow requests. |
| `payload` | Typed semantic payload. |
| `pagination` | Native semantic pagination for symbol and call-flow requests. |
| `warnings` | Incomplete or unavailable evidence reasons. |
| `hints` | Suggested next steps. |

Call-flow payloads are compact by default. Each call includes the target item, sampled call ranges, `rangeCount`, and `rangeSampleCount`. Use `contextLines>0` only when source previews are useful.

## Root Selection

If `workspaceRoot` is omitted:

1. Files inside `WORKSPACE_ROOT` use that configured root.
2. Files outside `WORKSPACE_ROOT` walk upward to the nearest project marker, such as `package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, or `pyproject.toml`.
3. If no marker exists, the file's directory is used.

## Language Servers

TypeScript and JavaScript are bundled through `typescript-language-server` and `typescript`. Other languages require their language server to be installed or configured.

Common environment overrides:

| Variable | Language |
|----------|----------|
| `OCTOCODE_TS_SERVER_PATH` | TypeScript/JavaScript |
| `OCTOCODE_PYTHON_SERVER_PATH` | Python |
| `OCTOCODE_GO_SERVER_PATH` | Go |
| `OCTOCODE_RUST_SERVER_PATH` | Rust |
| `OCTOCODE_JAVA_SERVER_PATH` | Java |
| `OCTOCODE_CLANGD_SERVER_PATH` | C/C++ |

Custom language-server config is loaded from:

1. `OCTOCODE_LSP_CONFIG`
2. `.octocode/lsp-servers.json`
3. `${OCTOCODE_HOME:-~/.octocode}/lsp-servers.json`

## Examples

Definition:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "definition",
  "symbolName": "printSchema",
  "lineHint": 133
}
```

References grouped by file:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "references",
  "symbolName": "isOctokitDeprecation",
  "lineHint": 27,
  "includeDeclaration": true,
  "groupByFile": true
}
```

Paginated call flow:

```json
{
  "uri": "/workspace/src/run.ts",
  "type": "callHierarchy",
  "symbolName": "printSchema",
  "lineHint": 133,
  "itemsPerPage": 5,
  "page": 1
}
```

Diagnostics:

```json
{
  "uri": "/workspace/src/run.ts",
  "severity": "all"
}
```

## Related Docs

- [Local Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md)
