# Tool data and handoff contract

This reference explains how agents carry evidence through the research layer of the Octocode agentic toolkit. It covers handoffs among Octocode's eleven tools. Use the [tool reference](OCTOCODE_TOOLS.md) for operation fields and the [local workflow](LOCAL_RESEARCH_WORKFLOW.md) for choosing the next evidence source. Inspect the live schema when constructing an unfamiliar request; compact fields are a summary, while the full schema retains nested and conditional constraints.

```sh
node packages/octocode/out/octocode.js tools --json
node packages/octocode/out/octocode.js tools astSearch --scheme --json
```

The CLI discovery catalog includes disabled tools: eleven tools are discoverable and ten are enabled by default. MCP registers the enabled subset. Check `availability` and effective configuration. Enabling a tool does not install a language server or supply provider credentials.

## Ownership and runtime boundaries

| Contract | Owner | What it establishes |
|---|---|---|
| Names, descriptions, input schemas, relations | `@octocodeai/octocode-core/schema` in the sibling `octocode-mcp-host` repository | Public requests and tool selection, independent of execution. |
| Shared server instructions | `@octocodeai/octocode-core/mcp`: `buildMcpInstructions(enabledToolNames)` | Workflow and evidence guidance for the exposed tool subset. |
| Execution, provider mapping, topology algorithms | [tools-core registry and runners](../packages/octocode-tools-core/src/tools/toolConfig.ts) | Validated request dispatch, provider calls, and result construction. |
| Search, syntax, minification, LSP primitives | [engine](../packages/octocode-engine/ARCHITECTURE.md) | Native and language-server operations used by tools-core. |
| Output TypeScript types | [bulk envelope](../packages/octocode-tools-core/src/types/toolOutput.ts) and per-tool types | Compile-time descriptions; these types do not validate external data at runtime. |
| Response shaping and pagination | [bulk response](../packages/octocode-tools-core/src/utils/response/bulk/response.ts) | Row status, evidence, presentation, and executable continuations. |
| MCP registration | [registration adapter](../packages/octocode-mcp/src/tools/registerTool.ts) | Publishes input schemas and forwards requests to the shared runners. |

MCP publishes no `outputSchema`. It returns `structuredContent` and text content, but clients cannot discover a per-tool output JSON Schema from `tools/list`. This is an output-discovery limitation, not proof that responses are untyped internally. MCP makes output schemas optional; when a server advertises one, its structured results must conform. See the [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools#output-schema).

## Requests and result rows

Each call uses one tool and an outer `queries` array of 1–5 queries. Independent queries can batch; a query that needs a prior result must wait for that result. Optional `goal` and `reasoning` provide task context and do not supply missing runtime fields.

For example, this is a `localFetch` request. Substitute an observed path and line range:

<!-- tool: localFetch -->
```json
{
  "queries": [
    {
      "path": "/ABS/repo/src/parser.ts",
      "startLine": 20,
      "endLine": 40,
      "minify": "none"
    }
  ]
}
```

MCP returns the envelope under `structuredContent`; CLI JSON/compact output exposes the result envelope directly. Tool payloads and ordinary follow-ups are row-local under `results[index].data`.

| Field | Interpretation |
|---|---|
| `results[].index` | Zero-based input position. Preserve it when a batch has mixed outcomes. |
| `results[].status` | Successful nonempty rows normally omit it. `empty` and `error` are distinct outcomes; inspect the reason and evidence before interpreting either. |
| `results[].cache` | `1` indicates a cached primary response. It does not establish current source freshness. |
| `results[].meta.evidence` | `kind` and `confidence` describe evidence provenance and strength. They do not promise complete coverage. |
| `results[].meta.diagnostics` | Optional diagnostic codes, hints, and partial state. |
| `results[].data` | Operation-specific payload, pagination, coverage, errors, and `next` calls. |
| `base`, `shared` | Presentation compression metadata described below. |
| `responsePagination` | Optional pagination of the rendered aggregate text, independent of row-level result pages. |

An outer `isError:false` does not establish that every row succeeded. Never infer success, absence, or completeness from a missing field. `answerReady` and `complete` are not universal members of `meta.evidence`; inspect the actual operation's pagination, coverage, truncation, and terminal-limit fields.

## Evidence boundaries

| Evidence kind | Supports | Still requires |
|---|---|---|
| `lexical` | Text/regex matches in the scanned scope. | Exact source and semantic checks for identity or usage claims. |
| `structural` | Syntax matches and captures. | Symbol resolution and runtime checks when those are the claim. |
| `syntactic` | Parsed declarations, syntax trees, and file topology. | Project-aware semantics; graph roots and exclusions limit reachability claims. |
| `exact` | Returned source or file metadata in the selected scope. | Coverage checks; security redaction and explicit content transformations still matter. |
| `semantic` | Results from a language server for its configured project and capabilities. | Provider/completeness inspection and runtime verification for runtime claims. |
| `provider` | Registry or repository-provider data. | Revision, index, result-cap, and materialization checks appropriate to the claim. |

Use `minify:"none"` when exact text matters. Local file reads default to exact content; a path-only GitHub file read defaults to standard minification. `standard` and `symbols` are explicit transformations with different purposes. A small response does not establish fidelity or absence.

For LSP, a tool name is insufficient evidence of semantic resolution: native document-symbol output is syntactic. Inspect `data.lsp.source`, evidence metadata, and the operation's completeness information. An unavailable provider, unsupported operation, failed anchor, and valid empty result require different recovery actions.

## Executable continuations

Copy the returned target and query. Follow every independent partial surface relevant to the claim, including nested captures, diagnostics, history collections, and content windows.

| Returned location | Query shape | How to call it |
|---|---|---|
| `results[].data.next.<name>` | Normally one tool query. | Call the named tool with `{ "queries": [next.query] }`. Check the returned shape rather than guessing from the next-call name. |
| `responsePagination.next` | A complete outer request, including its own `queries`. | Pass `next.query` as the tool arguments. Do not wrap that envelope inside another `queries` array. |

The CLI accepts the returned query or envelope through `tools <next.tool> --queries '<next.query JSON>' --compact`. A numeric cursor alone is not a complete continuation. Preserve the returned operation, scope, revision, filters, bounds, and unrelated pagination axes.

| Pagination layer | Typical controls | Identity and stopping rule |
|---|---|---|
| Collection | `page`, `pageSize`, `matchPage`, or operation-specific cursors | Follow the emitted next call until that collection is complete. Mutable provider searches do not all offer snapshot isolation. |
| Selected content | File readers: `chunkType`/`offset`/`limit`; history text: `charOffset`/`charLength` | Use returned offsets and selectors. Do not recompute them from displayed text or byte lengths. |
| Snapshot-aware operation | An operation's `snapshot` token, where supported | Preserve it in that operation's continuation. On a changed-result restart, discard its prior pages and rerun the returned restart query. |
| Whole-response text | Outer `responseCharOffset`, `responseCharLength`, `responseSnapshot` | Preserve the response token. `responsePagination.restart:true` requires discarding the prior text pages and executing its offset-zero continuation. |

Whole-response pagination limits `content[].text`, not the structured result collections. Its token is a digest of the complete rendered response; it does not create a frozen provider snapshot. Page headers are presentation, not source text. Use the returned offsets, actual lengths, and restart metadata when reconstructing text.

A typed terminal limit reports a boundary that cannot be paged further. Narrow the scope, choose another evidence surface, or report the limitation. Do not repeatedly increase an unsupported bound, invent a continuation, or convert a terminal result into an absence claim.

## Paths, shared fields, and anchors

Some local result metadata replaces absolute `path`/`uri` values with a relative `path` plus top-level `base`. Reconstruct a local absolute path from those fields before a manually constructed follow-up. Do not apply `base` to GitHub repository-relative paths or URLs. Returned `next` and clone `location` objects retain callable paths; prefer those continuations over manual reconstruction.

`shared` contains identical scalar fields removed from object entries in arrays directly inside row `data` payloads. Apply shared defaults to those entries when consuming the compressed representation. Do not merge them indiscriminately into every nested object. Identity, path, anchor, kind, and reason fields remain per-entry. Source text, snippets, and capture strings are evidence, not path metadata; do not rewrite them using `base`.

| Anchor | Units and scope |
|---|---|
| File-read `startLine`/`endLine`, LSP `lineHint` | One-based source lines. `lineHint` must identify the observed symbol line. |
| File-read `matchedLines` | Actual matched source lines. Context `matchRanges` can start earlier and are not interchangeable with these anchors. |
| LSP `position.line` / `position.character` | Zero-based line and UTF-16 character offset. Use instead of `symbolName` + `lineHint`. |
| LSP `orderHint` | Disambiguates repeated names on the observed line. |

Document LSP operations use `uri` without symbol anchors. `workspaceSymbol` requires `symbolName` and either `uri` or `workspaceRoot`. Anchored semantic operations require `uri` and one anchor form. Read the exact source first; minified output or a search snippet does not establish a precise character position.

## Connections between tools

| From | To | Carry forward and verify |
|---|---|---|
| `ghSearch` | `ghGetFileContent` | Owner, repository, observed path, and applicable ref. Indexed code search has no reliable source-line identity; fetch the source to establish it. |
| `ghSearchHistory` | `ghGetHistoryItem` | PR/issue number or commit ref, owner/repository, and the singular detail operation. Prefer the emitted detail call. |
| `ghGetHistoryItem` | `ghGetFileContent` or another history read | Changed-file path and the correct revision or diff side; continue each selected history surface independently. |
| `artifactSearch` | Repository search or clone | Verify repository host, owner/name, and any package subdirectory before constructing a repository query. A repository link is metadata, not source content. |
| `ghCloneRepo` | Local tools | `data.location.localPath` and checkout metadata. Completeness is relative to the selected sparse scope. Cached working-tree contents are not reverified merely because HEAD has a SHA. |
| `localSearch` or `astSearch` | `localFetch` | Observed path and source range, preferably through an executable `next` call. |
| `localFetch` | `lspSearch` | Exact path, symbol and actual source line, or an observed UTF-16 position. |
| `lspSearch` | Exact read or lexical/structural recovery | Returned source locations, `readSite`, or an explicit recovery call; retain provider and completeness qualifications. |

Check these handoffs through the public interface, not only by asserting that a `next` object exists. The [quality and acceptance guide](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md) separates schema checks, executed continuations, fixture coverage, and live-provider evidence. Distinguishable tools and task-based evaluations are also central to [Anthropic's tool-design guidance](https://www.anthropic.com/engineering/writing-tools-for-agents).
