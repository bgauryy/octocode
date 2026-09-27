# Octocode interfaces

Load when tool selection, transport, availability, or recovery is unclear. The live catalog and schemas are authoritative; this file is an invocation guide, not a second schema.

## Discover and invoke
Prefer exposed Octocode MCP tools with current public contracts. If unavailable, use the built checkout CLI; an installed skill can use `npx -y octocode`. These share core-owned contracts and the native Rust runtime. Do not substitute a legacy tool with different fields.

```bash
node packages/octocode/out/octocode.js scheme --compact
node packages/octocode/out/octocode.js scheme localSearch --view query --compact
node packages/octocode/out/octocode.js localSearch '{"reasoning":"<why>","path":"/ABS/repo/src","searchText":"needle","resultView":"matchOnly","pageSize":10}'
```

Run `scheme` once per session or tool-version change to discover enabled tools; `scheme <tool>` prints that tool's public input contract. Every tool requires nonblank `reasoning`. The grammar inventory lives in the live tool schemas (the `langType` enum from `scheme astSearch`): a language name, grammar ID, or alias selects its family; a dot-prefixed extension selects exactly; parser availability does not imply LSP availability. Never copy a static grammar list into a skill. Inspect an unfamiliar schema once, including relations and operation variants; reuse it until the tool/version changes. Use the default public schema view when compact fields do not resolve an input condition. CLI and MCP discovery do not expose internal output-validation schemas. Explicit commands above work in Bash and zsh without splitting a command stored in a scalar.

Pass arguments as an object. Direct MCP uses `{ "queries": [query] }`; CLI also accepts a single query or array. A host gateway may add its own outer envelope; follow its schema. Omit optional fields until the task needs them. On validation failure, correct the named field or selector using the live schema before retrying.

## 14 public tools

| Evidence question | Tool |
|---|---|
| GitHub code / tree / repositories | `ghSearch` |
| Known GitHub file | `ghGetFileContent` |
| GitHub history discovery / known item | `ghSearchHistory` / `ghGetHistoryItem` |
| Repeated cross-file GitHub analysis | `ghCloneRepo` when enabled |
| Local text / file layout / syntax / file-graph topology / structural rewrite / exact content / symbol identity | `localSearch` / `structureSearch` / `astSearch` / `astTopology` / `astRewrite` / `localFetch` / `lspSearch` |
| Package metadata or capability discovery | `artifactSearch` |
| Explicit classification capability | `octocode-clasify` owns admission; `clasify` also needs a nonblank `OCTOCODE_CLASSIFICATION_API` |

The default catalog contains 10 tools. The full CLI discovery catalog also includes opt-in `ghCloneRepo`, beta-gated `astTopology` and `astRewrite` (`OCTOCODE_BETA`), and credential-gated `clasify`. `ghCloneRepo` and `astRewrite` are CLI-only; MCP never lists them. MCP omits `clasify` when the resolved `OCTOCODE_CLASSIFICATION_API` is missing or blank; a direct CLI assessment names the missing key. Local access, clone, storage, credentials, and tool filters determine availability. Check the live catalog before using a follow-up. Check auth only when needed. If the current interface is unavailable, state the fallback and its coverage; do not present an unsupported call as an empty result.

## Output and recovery
- CLI output is single-line structured JSON by default (`--pretty` indents it for humans). MCP returns text plus structured data. Inspect per-row status: error is failure, empty is scoped absence, and exit 0 alone does not establish success for every row.
- Compact output can hoist repeated values into top-level `shared`. Inspect those values before declaring a row field missing, and retain shared identity when interpreting its files or directories.
- Follow executable `next.*` calls relevant to the claim in row data and nested payloads. Each supplies a tool and query; a label is not a tool name. Optional follow-up suggestions are not mandatory workflow steps.
- Result, match, metadata, content, and diagnostic pagination can be independent. Inspect partial/limit state even when `hasMore:false`; a limit may have another continuation or be terminal.
- `responsePagination` windows `content[].text`; structured content can remain complete. Avoid fetching the same evidence again solely to recover an envelope text window.
- Copy a continuation query unchanged before adapting a new search. For coverage claims, execute all relevant pages and check their union. For a lookup, stop at sufficient evidence and state material limits.
- An incomplete response never proves absence. A terminal limit calls for a narrower scope/query or an explicit gap, not an invented cursor. Preserve redaction and never reconstruct secrets.
- Local contracts: `localSearch` is lexical and has no `operation`; `structureSearch` uses `tree` (directory outline) or `files` (name/metadata discovery); `astSearch` uses `match`, `syntaxTree`, or `symbols` (1-based lines, 0-based UTF-16 columns; a `symbols` row's `name`+`line` or an identifier capture's `text`+`line` is `lspSearch` `symbolName`+`lineHint` as-is); file topology is the separate beta `astTopology` tool (`operation:"topology"` plus `analysis`). `localFetch` is exact by default and selectors are optional. LSP anchors are either 1-based `lineHint` with `symbolName` or 0-based UTF-16 `position`; document operations have no symbol anchor, and workspace symbols require a name plus `uri` or `workspaceRoot`.
- Batch independent probes within the interface's current limit; sequence dependent probes. Respect provider rate-limit/retry guidance rather than repeatedly issuing the same failing request.

Exit codes: `0` command completed · `1` empty · `2` input · `3` not-found · `4` auth · `5` tool · `6` partial (a re-runnable `next.*` continuation is in the response) · `7` rate-limit. Inspect row errors as well.

Next: route local evidence with `references/workflow-local.md`, remote evidence with `references/workflow-external.md`, or materialization with `references/workflow-combination.md`; examples live in `references/tool-examples.md`.
