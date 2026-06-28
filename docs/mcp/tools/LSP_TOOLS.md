# LSP Tools Reference

This is the canonical reference for Octocode's semantic code-intelligence operations. LSP is the protocol layer behind these operations; structural AST search remains part of `localSearchCode`.

Octocode exposes **one** public semantic tool:

| Tool | Use it for |
|------|------------|
| `lspGetSemantics` | Definitions, references, callers, callees, bidirectional call hierarchy, hover, document symbols, type definitions, and implementations. |

Semantic operations are local-only. Local tools are enabled by default; `ENABLE_LOCAL=false` disables them. LSP needs a file that exists on disk. Use `localSearchCode` first when you need a symbol `lineHint`; `mode:"structural"` matches can provide AST-derived anchors before LSP proves symbol identity.

For external repos: clone first with `ghCloneRepo` (or fetch a subtree with `ghGetFileContent(type:"directory")`), then use the returned `localPath` as the `uri` prefix for `lspGetSemantics`. The path is always absolute and immediately valid.

## Workflow

1. Search textually or structurally with `localSearchCode` and capture the exact `lineHint`.
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
| `definition` | Jumping from usage/import to declaration. For local TypeScript/JavaScript import aliases, definitions follow the import to the exported declaration when the language server first returns the import binding. | `payload.kind="definition"`, `locations[]`. |
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

Empty semantic payloads use `payload.kind="empty"` with a machine-readable `category`, such as `symbolNotFound`, `noLocations`, `noReferences`, `noHover`, or `noCalls`. The CLI maps these semantic misses to exit code `3` (`not found`) so scripts can fail without parsing the JSON envelope.

Call-flow payloads are compact by default. Each call includes the target item, sampled call ranges, `rangeCount`, and `rangeSampleCount`. Use `contextLines>0` only when source previews are useful.

## Root Selection

If `workspaceRoot` is omitted:

1. Files inside `WORKSPACE_ROOT` use that configured root.
2. Files outside `WORKSPACE_ROOT` walk upward to the nearest project marker, such as `package.json`, `tsconfig.json`, `.git`, `Cargo.toml`, `go.mod`, or `pyproject.toml`.
3. If no marker exists, the file's directory is used.

## Native vs. server fidelity, and the no-fallback contract

`documentSymbols` has a **native fast path** (oxc for JS/TS, Markdown heading outline) that runs with no language server and is preferred even when a server is present:

| Source (`lsp.source`) | When | Fidelity |
|-----------------------|------|----------|
| `lsp` | A language server is available | Type-aware, cross-file. |
| `native` / `markdown` | `documentSymbols` only | Syntax-only outline; no type inference. |

Every **other** semantic operation — `references`, `definition`, `hover`, `callers`/`callees`/`callHierarchy`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`/`subtypes`, `diagnostic` — requires a real server. When no server is available octocode **does not fall back to a syntactic guess**: it returns `status:"error"` with `errorCode:"lspServerUnavailable"` and a message directing you to `localSearchCode` (text/structural search) + `localGetFileContent`. (There is no longer a same-file-only `references` native path — a partial answer that silently omits cross-file usages is a trap, so it now errors instead.) See `docs/LSP_SERVER_LIFECYCLE.md`.

## TypeScript backends

The TS/JS server resolves in this order:

1. `OCTOCODE_TS_SERVER_PATH` — explicit override (args auto-selected: `--lsp -stdio` if the path is `tsgo`, else `--stdio`).
2. **`tsgo` on `PATH`** — Microsoft's Go-native server (`tsgo --lsp -stdio`, Node-free, ~10× faster). Opt-in: present-on-PATH only, no flag. References/rename are still maturing upstream.
3. **`typescript-language-server`** — the bundled zero-config default.

For the bundled default, Octocode first honors an executable
`typescript-language-server` already available on `PATH`. If the command is not
available, the resolver looks for `node_modules/typescript-language-server/lib/cli.mjs`
from the detected `workspaceRoot` and then from Octocode's package root. That
fallback keeps cloned or external workspaces working without installing a
language server inside every analyzed repository; the CLI path is run through the
current Node executable.

## Language Servers

TypeScript and JavaScript are bundled through `typescript-language-server` and `typescript`; JS/TS also has the server-free native path above. Other languages require their language server to be installed or configured.

Common environment overrides:

| Variable | Language |
|----------|----------|
| `OCTOCODE_TS_SERVER_PATH` | TypeScript/JavaScript (bundled — override only if needed) |
| `OCTOCODE_PYTHON_SERVER_PATH` | Python |
| `OCTOCODE_GO_SERVER_PATH` | Go |
| `OCTOCODE_RUST_SERVER_PATH` | Rust |
| `OCTOCODE_JAVA_SERVER_PATH` | Java |
| `OCTOCODE_CLANGD_SERVER_PATH` | C/C++ |
| `OCTOCODE_CSHARP_SERVER_PATH` | C# |
| `OCTOCODE_PHP_SERVER_PATH` | PHP |
| `OCTOCODE_SQL_SERVER_PATH` | SQL |
| `OCTOCODE_SWIFT_SERVER_PATH` | Swift |
| `OCTOCODE_JSON_SERVER_PATH` | JSON |
| `OCTOCODE_YAML_SERVER_PATH` | YAML |
| `OCTOCODE_HTML_SERVER_PATH` | HTML |
| `OCTOCODE_CSS_SERVER_PATH` | CSS/SCSS/LESS |

### Custom / bring-your-own servers

To add a language with **no built-in server** (e.g. Scala, Kotlin, Ruby) — or to replace a
built-in one — register it in a JSON config. Loaded in precedence order:

1. `$OCTOCODE_LSP_CONFIG` (explicit file path)
2. `<workspace>/.octocode/lsp-servers.json` (per-project)
3. `~/.octocode/lsp-servers.json` (per-user)

The file maps a file **extension** to a launch spec; a custom entry overrides the built-in spec
for that extension:

```jsonc
{
  "languageServers": {
    ".scala": { "command": "metals", "args": ["stdio"], "languageId": "scala" }
  }
}
```

`command` and `languageId` are required; `args` (default `[]`) and `initializationOptions`
(passed verbatim in `initialize`) are optional. With the config present, every semantic op works
for that language; without it the extension is unsupported and semantic ops throw
`lspServerUnavailable` (→ fall back to `localSearchCode`). See
[`LSP_SERVER_LIFECYCLE.md`](https://github.com/bgauryy/octocode/blob/main/docs/LSP_SERVER_LIFECYCLE.md#custom--bring-your-own-lsp-any-language).

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

- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
