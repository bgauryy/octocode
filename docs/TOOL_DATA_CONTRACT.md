# Tool data and handoff contract

This reference explains how agents carry evidence through the research layer of the Octocode agentic toolkit. It covers handoffs among Octocode's 16 tools. Use the [tool reference](OCTOCODE_TOOLS.md) for operation fields and the [local workflow](OCTOCODE_RESEARCH_MANIFEST.md#local-workflow) for choosing the next evidence source. Inspect the live input schema when constructing an unfamiliar request; the catalog's compact fields are a summary, while `--view query` and `--view full` retain nested and conditional input constraints.

```sh
node packages/octocode/out/octocode.js scheme --compact
node packages/octocode/out/octocode.js scheme astSearch --view query
```

The CLI discovery catalog includes disabled tools: 16 tools are discoverable, and a disabled tool reports `availability.enabled:false` with the gating `envVar`. With beta tools disabled and no classification key resolved, the CLI enables 13 and MCP registers 12: MCP never registers the CLI-only `ghCloneRepo` and `astRewrite`. Check `availability` and effective configuration. Enabling a tool does not install a language server or supply provider credentials.

## Ownership and runtime boundaries

| Contract | Owner | What it establishes |
|---|---|---|
| Names, descriptions, input schemas | `@octocodeai/octocode-core/schema`, re-exported in-repo as `@octocodeai/config/schema` | Public requests and tool selection, independent of execution. |
| Shared server instructions | `@octocodeai/config/mcp` (re-exports core): `buildMcpInstructions(enabledToolNames)` | Workflow and evidence guidance for the exposed tool subset. |
| Execution, provider mapping, topology algorithms | [native runtime](../packages/octocode-native/crates/runtime/src/runtime/engine.rs) and [tool modules](../packages/octocode-native/crates/runtime/src/tools) | Validated request dispatch, provider calls, and result construction. |
| Search, syntax, minification, LSP primitives | [engine crate](../packages/octocode-native/ARCHITECTURE.md) | Native and language-server operations used by the runtime. |
| Response contracts | [generated contract](../packages/octocode-config/contract/tool-contract.json), [generated TS types](../packages/octocode-config/src/contracts/toolTypes.generated.ts), and Rust response types | Runtime-validated request and transport-neutral result structures. |
| Response shaping and pagination | [native response module](../packages/octocode-native/crates/runtime/src/response/mod.rs) | Row status, evidence, presentation, and executable continuations. |
| MCP registration | [native adapter](../packages/octocode-mcp/src/native/index.ts) | Registers available, non-CLI-only tools and forwards execution to the native runtime. |

The contract path is one-way: core authors the accepted shape, generation embeds
that shape and its preparation rules in native, and interfaces only publish or
forward it. Input preparation may add documented defaults or normalize explicit
text fields such as trimmed search terms. Numeric bounds reject invalid caller
values; no interface silently clamps them or drops unknown fields.

MCP registers each available tool (except the CLI-only `ghCloneRepo` and
`astRewrite`) with its title, description, and input schema. It deliberately
omits `outputSchema` from discovery to avoid
spending agent context on runtime-validation metadata. Core and the native
runtime retain canonical output contracts internally, validate produced results,
and include those contracts in drift detection. Responses still carry matching
`structuredContent` plus a text representation.

## Requests and result rows

Each call uses one tool and an outer `queries` array of 1–5 queries (`clasify` also accepts one matrix directly). Independent queries can batch; a query that needs a prior result must wait for that result. Each new query carries its own required `goal` and `reasoning` (at most 500 characters each); they state the decision and do not supply missing runtime fields.

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

The text channel (YAML by default) is compacted further; `structuredContent` and JSON keep the envelope:
- a single-row response drops the `results: - index: 0 data:` wrapper and renders the row's fields at the top (a batch keeps it);
- path-only search rows (`resultView:"files"`) render as `path`, and count rows as `path (count)`;
- `localFetch` and `ghGetFileContent` print file content verbatim after the metadata, under `content (source lines):` when numbered (below) or `content (copy-safe):` otherwise; a GitHub batch labels each block `=== [index] path content (…) ===`.

### Numbered source content

Hosts show agents the structured JSON, so source evidence carries its own line numbers there. When a `localFetch` or `ghGetFileContent` row returns original source lines (no transformed `contentView`; line ranges, match windows, `fullContent`, and line pages), `content` is numbered like `cat -n`, without padding:

```text
95	        self._thread_sharing_count = 0
96	
... [lines 97-254 omitted] ...
255	    def close(self):
```

- Each returned line is `<line>` + TAB + the source text. The prefix is not part of the source: strip everything up to the first TAB before copying text into an edit or a `matchString`.
- Line-omission markers between non-adjacent windows (`... [lines A-B omitted] ...`) stay unnumbered.
- The numbers state the returned source lines, so `sourceLineRanges` is omitted from a numbered row; `matchedLines` is omitted when every returned line matched (a grep-style map).
- Size counts (`returnedChars`, and the debug-only `sourceBytes`/`returnedBytes`) measure the source text, not the line-number prefixes.
- Views whose lines are not source lines stay verbatim and keep `sourceLineRanges` when they have one: `minify:"standard"`/`"symbols"` views (`contentView`), byte windows (`contextBytes`, long minified lines) and `chunkType:"bytes"` pages, whose offsets count the returned text, and content whose lines no longer map one-to-one onto the source range.
- Both text encodings render from the same numbered content. The runtime helper is `packages/octocode-native/crates/runtime/src/runtime/numbered.rs`; other tools that return multi-line source text reuse it rather than inventing a format.
- Search rows use the same form. A `localSearch` row with `contextLines > 0` numbers its window (`matchLines` still lists which lines matched; a truncated window stays verbatim). A repo-scoped `ghSearchCode` `match:"file"` row lists its keyword lines as `lines: ["<line>\t<text>", …]` (see below).

### Search result shapes

- `localSearch` shows every hit on one page when a search has at most 50 hits and no `maxMatchesPerFile`, so no per-file `pagination` or `next.nextMatchPage` appears; larger results keep 10 rows per file and the paging continuations. A complete result over at most three files carries `next.read`: a `localFetch` `matchString` read (±6 lines) of the top file's hits. An invalid regex alternation (`a(|b`) gets a `next.repair` that escapes each broken alternative and keeps regex mode; a single invalid anchor still gets the literal repair.
- `ghSearchCode` reads the top 5 files of a repo-scoped `match:"file"` page through the contents cache (core API quota, no extra code-search calls). Each resolved row has `lines` (every keyword line, up to 20, with `hitCount` when there are more) in place of index fragments; `data.commitSha` names the commit read. `owner`/`repo` are named once on `data` for a repo-scoped page. A row whose blob had no keyword line or could not be read keeps its fragments and sets `lineResolved:false`. With `branch`, lines are read at that ref (`data.ref`), `data.indexRef:"defaultBranch"` labels the candidates and unresolved fragments as default-branch index output, and a path absent at the ref is `atRef:false` without default-branch text. Fragment `matchIndices` appear only with `debug:true`. `next.readTopMatch` reads the first resolved file by line range at `data.commitSha`; an unresolved top file's fragment read is pinned to that commit, and one absent at the requested ref offers no read.
- `structureSearch` `files` sorts by path by default (walk order, like `git ls-files`) and stops walking once `limit` is filled; such a cut reports `truncated`, `partialReasons:["limit"]`, `atLeast` (a lower bound, not a total) and `next.expandLimit`. Other sorts walk the whole scope and report `totalAvailable`.

| Field | Interpretation |
|---|---|
| `results[].index` | Zero-based input position. Preserve it when a batch has mixed outcomes. |
| `results[].status` | Successful nonempty rows normally omit it. `empty` and `error` are distinct outcomes; inspect the reason and evidence before interpreting either. |
| `results[].cache` | Debug only. `1` indicates a cached primary response. It does not establish current source freshness. |
| `results[].meta.evidence` | `kind` and `confidence` describe evidence provenance and strength. They do not promise complete coverage. |
| `results[].meta.diagnostics` | Optional diagnostic codes, hints, and partial state. |
| `results[].data` | Operation-specific payload, pagination, coverage, errors, hints, and `next` calls. Empty and error rows carry one concise recovery hint (at most 120 characters). |
| `base`, `shared` | Presentation compression metadata described below. |
| `responsePagination` | Optional pagination of the whole response, independent of row-level result pages. |

### Minimal by default

Rows carry the answer and what the next call needs. With `debug: true` a query also receives:
- `meta`;
- `cache`;
- each tool's scan and provider fields (for example `searchEngine`, `filesScanned`, `modified`, byte counts, `effectiveQuery`, the lspSearch `lsp` receipt and `workspaceRoot`, topology `coverage.diagnostics`);
- info-level diagnostics;
- the top-level `snapshot`;
- some request echoes (for example `operation`); identity fields stay in minimal rows: `owner`, `repo`, `path`, `type`, and `ref` on GitHub rows, `type` on `workspaceSymbol` rows;
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

Use `minify:"none"` when exact text matters. Local file reads and path-only GitHub file reads default to exact content (`minify:"none"`). `standard` and `symbols` are explicit transformations with different purposes. A small response does not establish fidelity or absence.

For LSP, a tool name is insufficient evidence of semantic resolution: native document-symbol output is syntactic. Inspect `data.lsp.source` (present only with `debug: true`), evidence metadata, and the operation's completeness information. An unavailable provider, unsupported operation, failed anchor, and valid empty result require different recovery actions.

## Executable continuations

Copy the returned target and query. Follow every independent partial surface relevant to the claim, including nested captures, diagnostics, history collections, and content windows.

| Returned location | Query shape | How to call it |
|---|---|---|
| `results[].data.next.<name>` | Normally one tool query. | Call the named tool with `{ "queries": [next.query] }`. Check the returned shape rather than guessing from the next-call name. |
| `responsePagination.next` | A complete outer request, including its own `queries`. | Pass `next.query` as the tool arguments. Do not wrap that envelope inside another `queries` array. |

PR menus are exact reads: `reviewPatches` names its files (a ranking guess, `confidence: "high"`), and a literal search of every patch needs the caller's literal, so it is never offered as a placeholder. Issue reads add `closedBy` (`{number, state, mergedAt?}`, merged first, at most 25; more set `isPartial`, `terminalLimit`, and `partialReasons:["closingReferenceLimit"]`) and `next.readFixPr`; `ghSearchHistory`'s `next.readPr` carries `candidates` (up to three numbers, the target first).

Every `next.*` query is complete under the contract: it carries the `goal` and `reasoning` of the query that produced it, plus any page, snapshot, or offset fields the published schemas leave out. Run it unchanged. A query you write yourself needs its own `goal` and `reasoning`; the contract rejects a query without them, whoever wrote it.

The CLI accepts the returned query or envelope through `<next.tool> '<next.query JSON>'`. A numeric cursor alone is not a complete continuation. Preserve the returned operation, scope, revision, filters, bounds, and unrelated pagination axes.

| Pagination layer | Typical controls | Identity and stopping rule |
|---|---|---|
| Collection | `page`, `pageSize`, `matchPage`, or operation-specific cursors | Follow the emitted next call until that collection is complete. Mutable provider searches do not all offer snapshot isolation. |
| Selected content | File readers: `chunkType`/`offset`/`chunkSize`; history text: `charOffset`/`charLength` | Use returned offsets and selectors. Do not recompute them from displayed text or byte lengths. |
| Snapshot-aware operation | An operation's `snapshot` token, where supported | Preserve it in that operation's continuation. On a changed-result restart, discard its prior pages and rerun the returned restart query. |
| Whole-response text | Outer `responseCharOffset`, `responseCharLength`, `responseSnapshot`, `responseScope` | Preserve the response token. `responsePagination.restart:true` requires discarding the prior text pages and executing its offset-zero continuation. |

`responseScope` selects what an explicit window pages: `text` (default), `structured` (the serialized envelope, returned as `responseWindow` fragments), or `rows` (complete JSON envelopes of whole rows). A response larger than `output.pagination.defaultCharLength` (default 50,000) with no explicit window is paged automatically by rows. Explicit text pagination limits `content[].text`; partial text pages leave `structuredContent.results` empty to avoid repeating the full payload. Empty structured rows on such a page do not mean no results: read the numbered text and follow `responsePagination.next`. A single complete text page retains its structured rows. Automatic row pagination instead returns complete rows or collection elements in both surfaces; structured-window pagination uses `responseWindow` fragments that concatenate into the envelope JSON. Follow the returned `scope` and continuation rather than assuming every page has source rows.

The response token identifies one captured rendered response. While that response remains in the runtime cache, its continuations replay the same output even if a file or provider changes; they do not rerun the source query. This prevents mixing pages from different captures. To observe current source, start a fresh query without response continuation fields. If the cached capture is unavailable, the runtime may re-execute the query; a different response produces restart metadata instead of mixed pages. This response token does not freeze an upstream provider or replace a tool's source-version snapshot. Page headers are presentation, not source text. Use returned offsets and actual lengths when reconstructing text.

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
| `localSearch`, `structureSearch`, `astSearch`, or `astTopology` | `localFetch` | Observed path (joined with `base`) and source line. |
| `localSearch` / `ghSearchCode` (wide page) | `clasify` | The emitted `next.clasify`, run unchanged; present only while `clasify` is available. See [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md). |
| `localFetch` | `lspSearch` | Exact path, symbol and actual source line, or an observed UTF-16 position. |
| `lspSearch` | Exact read or lexical/structural recovery | Returned source locations (`uri` + one-based `displayRange`) or an emitted recovery call (`next.readFile`, `next.textSearch`, `next.retry`); retain provider and completeness qualifications. |

Check these handoffs through the public interface, not only by asserting that a `next` object exists. The [quality and acceptance guide](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md) separates schema checks, executed continuations, fixture coverage, and live-provider evidence. Distinguishable tools and task-based evaluations are also central to [Anthropic's tool-design guidance](https://www.anthropic.com/engineering/writing-tools-for-agents).
