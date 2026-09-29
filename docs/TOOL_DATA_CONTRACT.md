# Tool data and handoff contract

This reference explains how agents carry evidence through the research layer of the Octocode agentic toolkit. It covers handoffs among Octocode's 16 tools. Use the [tool reference](OCTOCODE_TOOLS.md) for operation fields and the [local workflow](LOCAL_RESEARCH_WORKFLOW.md) for choosing the next evidence source. Inspect the live public input schema when constructing an unfamiliar request; compact fields are a summary, while the default public view retains nested and conditional input constraints.

```sh
node packages/octocode/out/octocode.js scheme --compact
node packages/octocode/out/octocode.js scheme astSearch --view query
```

The CLI discovery catalog includes disabled tools: 16 tools are discoverable; with beta tools disabled and no Jev provider key resolved, MCP registers 12 and the CLI enables 13 (it adds `ghCloneRepo` when persistent storage is available). Check `availability` and effective configuration. Enabling a tool does not install a language server or supply provider credentials.

## Ownership and runtime boundaries

| Contract | Owner | What it establishes |
|---|---|---|
| Names, descriptions, input schemas, relations | `@octocodeai/octocode-core/schema` in the sibling `octocode-mcp-host` repository | Public requests and tool selection, independent of execution. |
| Shared server instructions | `@octocodeai/octocode-core/mcp`: `buildMcpInstructions(enabledToolNames)` | Workflow and evidence guidance for the exposed tool subset. |
| Execution, provider mapping, topology algorithms | [native runtime](../packages/octocode-native/crates/runtime/src/runtime/engine.rs) and [tool modules](../packages/octocode-native/crates/runtime/src/tools) | Validated request dispatch, provider calls, and result construction. |
| Search, syntax, minification, LSP primitives | [engine crate](../packages/octocode-native/ARCHITECTURE.md) | Native and language-server operations used by the runtime. |
| Response contracts | [generated contract](../packages/octocode-config/contract/tool-contract.json) and Rust response types | Runtime-validated request and transport-neutral result structures. |
| Response shaping and pagination | [native response module](../packages/octocode-native/crates/runtime/src/response/mod.rs) | Row status, evidence, presentation, and executable continuations. |
| MCP registration | [public adapter](../packages/octocode-mcp/src/public.ts) | Publishes Standard Schema definitions and forwards execution to the native runtime. |

The contract path is one-way: core authors the accepted shape, generation embeds
that shape and its preparation rules in native, and interfaces only publish or
forward it. Input preparation may add documented defaults or normalize explicit
text fields such as trimmed search terms. Numeric bounds reject invalid caller
values; no interface silently clamps them or drops unknown fields.

MCP publishes each enabled tool's input schema, description, annotations, and
availability. It deliberately omits `outputSchema` from discovery to avoid
spending agent context on runtime-validation metadata. Core and the native
runtime retain canonical output contracts internally, validate produced results,
and include those contracts in drift detection. Responses still carry matching
`structuredContent` plus a text representation.

## Requests and result rows

Each call uses one tool and an outer `queries` array of 1–5 queries. Independent queries can batch; a query that needs a prior result must wait for that result. Each query carries its own required `goal` and `reasoning`; they state the decision and do not supply missing runtime fields.

For example, this is a `localFetch` request. Substitute an observed path and line range:

<!-- tool: localFetch -->
```json
{
  "queries": [
    {
      "goal": "Read the parser range needed for the claim",
      "reasoning": "Read the exact parser range needed for the current claim.",
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
| `results[].cache` | Debug only. `1` indicates a cached primary response. It does not establish current source freshness. |
| `results[].meta.evidence` | `kind` and `confidence` describe evidence provenance and strength. They do not promise complete coverage. |
| `results[].meta.diagnostics` | Optional diagnostic codes, hints, and partial state. |
| `results[].data` | Operation-specific payload, pagination, coverage, errors, and `next` calls. |
| `base`, `shared` | Presentation compression metadata described below. |
| `responsePagination` | Optional pagination of the rendered aggregate text, independent of row-level result pages. |

### Minimal by default

Rows carry the answer and what the next call needs. With `debug: true` a query also receives:
- `meta`;
- `cache`;
- each tool's scan and provider fields (for example `searchEngine`, `filesScanned`, `modified`, byte counts, `effectiveQuery`, the lspSearch `lsp` receipt and `workspaceRoot`, topology `coverage.diagnostics`);
- info-level diagnostics;
- the top-level `snapshot`;
- request echoes (`operation`, `type`, `owner`, `repo` equal to the query);
- `false`/`0` defaults;
- pagination of a finished single page.

These are never dropped: open pagination (`hasMore`, or a page after the first), every `next` continuation (continuations carry their own `snapshot`), warnings and errors, scan scope on an empty search, and confidence signals such as topology `completeness` and `confidence`. Error rows are always complete. The runtime owns this projection (`minimize_row`); each tool declares its debug-only fields.

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

Every `next.*` query except `next.clasify` carries `followUp: true` instead of `goal` and `reasoning`: a continuation serves the decision of the query that produced it, so the brief is not repeated on each page or retyped on replay. Run it unchanged. A query you write yourself still needs its own `goal` and `reasoning`; the runtime rejects a new query without them. Clasify continuations keep their briefs because clasify sends them to its provider.

The CLI accepts the returned query or envelope through `<next.tool> '<next.query JSON>'`. A numeric cursor alone is not a complete continuation. Preserve the returned operation, scope, revision, filters, bounds, and unrelated pagination axes.

| Pagination layer | Typical controls | Identity and stopping rule |
|---|---|---|
| Collection | `page`, `pageSize`, `matchPage`, or operation-specific cursors | Follow the emitted next call until that collection is complete. Mutable provider searches do not all offer snapshot isolation. |
| Selected content | File readers: `chunkType`/`offset`/`chunkSize`; history text: `charOffset`/`charLength` | Use returned offsets and selectors. Do not recompute them from displayed text or byte lengths. |
| Snapshot-aware operation | An operation's `snapshot` token, where supported | Preserve it in that operation's continuation. On a changed-result restart, discard its prior pages and rerun the returned restart query. |
| Whole-response text | Outer `responseCharOffset`, `responseCharLength`, `responseSnapshot` | Preserve the response token. `responsePagination.restart:true` requires discarding the prior text pages and executing its offset-zero continuation. |

Explicit text pagination limits `content[].text`; partial text pages leave `structuredContent.results` empty to avoid repeating the full payload. Empty structured rows on such a page do not mean no results: read the numbered text and follow `responsePagination.next`. A single complete text page retains its structured rows. Automatic row pagination instead returns complete rows or collection elements in both surfaces; structured-window pagination uses `responseWindow` fragments that concatenate into the envelope JSON. Follow the returned `scope` and continuation rather than assuming every page has source rows.

The response token identifies the full response being paged; it does not freeze the upstream provider. Page headers are presentation, not source text. Use returned offsets, actual lengths, and restart metadata when reconstructing text.

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

## Graph evidence

`astTopology` returns candidate import/re-export relationships within the scanned scope. `transitiveEdge:true` marks a direct condensation edge that also has an alternate path; it does not mean an indirect import. `topologicalLayer` is computed in the query's traversal direction, so dependents and dependencies can assign different layers. It is not an architectural-layer label.

`includeTests:false` excludes tests as retention roots; it does not evaluate conditional compilation or remove every syntactic edge into test code. Preserve edge kinds, coverage diagnostics, configuration and snapshot information when combining results. AST declaration identifiers describe source occurrences, not canonical bindings stable across edits. Use exact source and LSP evidence for symbol identity and consequential impact claims.

## Connections between tools

| From | To | Carry forward and verify |
|---|---|---|
| `ghSearchCode` / `ghStructure` | `ghGetFileContent` | Owner, repository, observed path, and applicable ref. Indexed code search has no reliable source-line identity; fetch the source to establish it. |
| `ghSearchHistory` | `ghGetHistoryItem` | PR/issue number or commit ref, owner/repository, and the singular detail operation. Prefer the emitted detail call. |
| `ghGetHistoryItem` | `ghGetFileContent` or another history read | Changed-file path and the correct revision or diff side; continue each selected history surface independently. |
| `artifactSearch` | Repository search or clone | Verify repository host, owner/name, and any package subdirectory before constructing a repository query. A repository link is metadata, not source content. |
| `ghCloneRepo` | Local tools | `data.location.localPath` and checkout metadata. Completeness is relative to the selected sparse scope. Cached working-tree contents are not reverified merely because HEAD has a SHA. |
| `localSearch`, `structureSearch`, `astSearch`, or `astTopology` | `localFetch` | Observed path and source range, preferably through an executable `next` call. |
| `localFetch` | `lspSearch` | Exact path, symbol and actual source line, or an observed UTF-16 position. |
| `lspSearch` | Exact read or lexical/structural recovery | Returned source locations, `readSite`, or an explicit recovery call; retain provider and completeness qualifications. |

Check these handoffs through the public interface, not only by asserting that a `next` object exists. The [quality and acceptance guide](MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md) separates schema checks, executed continuations, fixture coverage, and live-provider evidence. Distinguishable tools and task-based evaluations are also central to [Anthropic's tool-design guidance](https://www.anthropic.com/engineering/writing-tools-for-agents).
