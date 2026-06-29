# Octocode Search Parity Checklist

This document is the agent-facing audit guide for making `npx octocode search`
replace Octocode's raw tools and quick CLI commands without duplicating
implementation logic.

Authoritative contract:
https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md

## Replacement Rule

`npx octocode search` may replace another Octocode surface only when all of these
are true:

1. The query lowers into a canonical OQL object and `--explain` shows the exact
   normalized query.
2. The plan calls the same backing runner in `packages/octocode-tools-core`.
3. Returned rows preserve the information an agent needs to decide, cite, and
   continue research.
4. Diagnostics make uncertainty explicit: partial result, approximate provider,
   unsupported target, truncation, auth/rate failures, stale cache, sanitizer.
5. Follow-ups are executable OQL continuations or explicitly marked legacy
   handoff data.
6. The CLI/MCP layer does not reimplement tool behavior, schema rules, routing,
   pagination, or output semantics.

Non-goal: `search` should not replace management/meta commands such as
`npx octocode install`, `npx octocode auth login`, `npx octocode auth logout`,
`npx octocode auth status`, `npx octocode cache status`,
`npx octocode cache clear`, `npx octocode tools`, `npx octocode context`,
`npx octocode --help`, or `npx octocode --version`.

## Ownership Boundary

| Layer | Owns | Must not own |
|---|---|---|
| `@octocodeai/octocode-core` | Public descriptions, schema text, command/tool guidance | Execution or interface-specific parsing |
| `packages/octocode-tools-core/src/oql` | OQL schema, normalization, shorthand lowering, planning, adapters, result envelope | CLI rendering or terminal argv concerns |
| `packages/octocode-tools-core/src/tools/*` | The direct tool runners and security wrappers, including the `oqlSearch` wrapper around `packages/octocode-tools-core/src/oql` | OQL-specific presentation |
| `packages/octocode/src/cli` | argv parsing, local-vs-GitHub target classification, rendering | Search semantics, routing, backend field mapping |
| `packages/octocode-mcp` | MCP registration and transport | Tool behavior or OQL planning |

If a quick command needs a convenience form, put the reusable lowering helper in
`tools-core/oql` and call it from the CLI. The CLI may classify a string as a
local path or GitHub ref because that depends on filesystem/runtime context.

Evidence anchors:

- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/run.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/shorthand.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode/src/cli/commands/search.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/adapters/v2.ts

## Live Inventory Checked

Command and tool inventory last checked from the built CLI on 2026-06-29. The
parity scorecard and gap log below keep their original verification dates unless
an entry says otherwise.

Raw tools checked with `npx octocode tools <name> --scheme`:

| Group | Tools |
|---|---|
| GitHub | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo` |
| Local | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect` |
| LSP | `lspGetSemantics` |
| Package | `npmSearch` |
| Other | `oqlSearch` |

CLI commands checked with `npx octocode <command> --help`:

| Surface | Commands |
|---|---|
| Search/research | `search`, `unzip`, `clone`, `cache fetch` |
| Raw/meta | `tools`, `context`, top-level `--help` |
| Management | `skill`, `install`, `auth login`, `auth logout`, `auth refresh`, `auth status`, top-level `login`, top-level `logout`, `status`, `lsp-server` |

Current `search --scheme` target inventory:

| Type | Targets |
|---|---|
| Active | `code`, `content`, `structure`, `files`, `semantics`, `repositories`, `packages`, `pullRequests`, `commits`, `artifacts`, `diff`, `research`, `graph`, `materialize` |

Parity rule: if a target or command is not in this inventory, do not claim
`search` replaces it. If the CLI adds a command, this checklist must be updated
from its live help and the backing raw tool schema in the same change.

## Scheme Hooks To Preserve

This is not a replacement for `tools <name> --scheme`; it is the audit shortcut
for the fields most likely to break agent research.

| Raw tool | Scheme hooks that must survive through `search` |
|---|---|
| `ghSearchCode` | `keywords`, owner/repo scope, `extension`, `filename`, `path`, `language`, `match:file|path`, `limit`, `page`, `concise`. |
| `ghGetFileContent` | `owner`, `repo`, `path`, `branch`, `type:file|directory`, line range, `matchString`, regex/case flags, `contextLines`, char window, `minify:none|standard|symbols`, `forceRefresh`. |
| `ghViewRepoStructure` | repo/ref/path, `maxDepth`, `page`, `itemsPerPage`, `includeSizes`. |
| `ghSearchRepos` | keywords/topics/language/owner/license/visibility, GitHub range filters, `match`, `sort`, `archived`, pagination, `concise`. |
| `ghHistoryResearch` | `type:prs|commits`, PR list/detail selectors, merged/open/closed state, patch/comment/review/commit selectors, path/branch/date filters, `matchString`, file/comment/commit pages, char window, PR `minify:none|standard`. |
| `ghCloneRepo` | repo/ref, `sparsePath`, `forceRefresh`, clone-enabled diagnostics, returned local path. |
| `localSearchCode` | text/regex/structural modes, AST `pattern`/YAML `rule`, include/exclude/hidden/noIgnore, context and match content length, per-file match paging, count/onlyMatching/unique, multiline/dotall, ranking and sort controls. |
| `localGetFileContent` | path, exact/match/line/char reads, regex/case flags, `contextLines`, `minify:none|standard|symbols`, full content. |
| `localViewStructure` | path, recursive/depth, details/hidden, sort/reverse, pattern/extensions, files/directories-only, pagination. |
| `localFindFiles` | name/path/regex filters, depth, entry type, metadata filters, permissions, size/time filters, sort/details, pagination. |
| `localBinaryInspect` | modes `inspect|list|extract|decompress|strings|unpack`, archive entry paging, string scan offset, char window, format override, offsets, `localPath` for derived output. |
| `lspGetSemantics` | all semantic types, `uri`, `symbolName`, `lineHint`, `orderHint`, `workspaceRoot`, `depth`, `includeDeclaration`, `groupByFile`, `format`, pagination, context lines. |
| `npmSearch` | exact package vs keyword search, `mode:lean|full`, `page`, repository handoff. |
| `oqlSearch` | canonical `target/from/where/fetch/params/materialize`, batch envelopes, `explain`, diagnostics, evidence, and `next.*` continuations. |

## Raw Tool Parity Matrix

| Raw tool | OQL target | Required OQL shape | Replacement status | Agent checks |
|---|---|---|---|---|
| `localSearchCode` | `code` | `from:{kind:"local"}`, `where.kind:"text"|"regex"|"structural"` | Strong | Compare `path`, `line`, `snippet`, `metavars`, count/onlyMatching/unique behavior, per-file match pagination, truncation diagnostics. |
| `localGetFileContent` | `content` | `from:{kind:"local"}`, `fetch.content` | Strong | Verify `contentView`, minification, line/char/match ranges, `contentTruncated`, exact-mode proof. |
| `localViewStructure` | `structure` | `from:{kind:"local"}`, `fetch.tree` | Strong | Verify file/dir entries, depth, pattern filters, sizes, sorting, pagination. |
| `localFindFiles` | `files` | `from:{kind:"local"}`, optional field/content predicates | Strong | Verify metadata filters, content-contained queries, negative queries, local complete-universe semantics. |
| `localBinaryInspect` | `artifacts` | `from:{kind:"local",path}`, `params` passed to binary runner | Partial | Generic record rows preserve inspect/list/extract/decompress/strings/unpack payloads + derived `localPath`; extracted paths now emit `next.structure`/`next.files`. Typed row sub-shapes still future. |
| `lspGetSemantics` | `semantics` | local/materialized/GitHub `from`, `params.type`, symbol/line fields | Partial | All 9 LSP types reachable; semantics rows now emit `next.fetch`, code rows emit `next.semantic`. Server diagnostics + remote materialization provenance preserved; typed per-op rows still future. |
| `ghSearchCode` | `code` | `from:{kind:"github"}`, provider-safe predicate | Strong for provider search | Regex/provider semantics may be approximate; require materialization for AST/PCRE2/exact proof. |
| `ghGetFileContent` | `content` | `from:{kind:"github"}`, `scope.path`, `fetch.content` | Strong | Verify branch/ref, matchString/ranges, minification mode, char pagination. |
| `ghViewRepoStructure` | `structure` | `from:{kind:"github"}`, `scope.path`, `fetch.tree` | Strong | Verify repo/ref/path, depth, empty dirs, large tree truncation. |
| `ghCloneRepo` | materialization lane + `target:"materialize"` | `from:{kind:"github"}`, `materialize.mode`, bounded `scope.path` | Strong | Search uses clone internally **and** `target:"materialize"` is a first-class checkpoint row (`localPath`/`repoRoot`/`cache`/`complete`) with `next.structure`/`next.files`; unbounded clone refused. |
| `ghSearchRepos` | `repositories` | `target:"repositories"`, optional GitHub `from`, `params` | Partial | `params` is opaque; row payload is generic. Need typed schema docs and OQL continuations. |
| `ghHistoryResearch` | `pullRequests`, `commits`, `diff` | GitHub `from`, `params` for PR/commit/diff selectors | Partial | PR list/detail, merged state, patch selectors, comments/reviews, commit diffs, history pagination must match raw tool. `diff` now has two typed lanes (PR-patch vs direct-file `{baseRef,headRef,path}` + repair). `symbols` view on PR/commit/diff now flagged `signatureUnsupported`. |
| `npmSearch` | `packages` | `target:"packages"`, `from:{kind:"npm"}` default, `params` | Partial | Package rows exist, but raw `data.next` is legacy handoff, not first-class OQL `next`. |
| `oqlSearch` | all OQL targets | canonical OQL query or batch envelope | Strong | Direct MCP `oqlSearch` and CLI `search --query` must execute the same OQL runner, produce the same diagnostics/evidence, and preserve `next.*` continuations. |

Status meanings:

- Strong: core runner path exists and row type is OQL-native.
- Partial: core runner path exists, but parity depends on generic `params`,
  generic `record` rows, renderer support, or missing OQL continuations.
- Not covered: target is intentionally unsupported or belongs outside search.

## OQL Rating Scorecard

Rating is for the current `npx octocode search` implementation as verified from the
built CLI and raw schemas on 2026-06-22, not the desired final design.

> **2026-06-22 update.** Open-gap closures (continuation registry, capability
> diagnostics, diff lane split, `target:"materialize"`) lifted the affected
> cells below. Remaining drag on the higher-level ratings is now typed V2 row
> contracts, the human renderer for record rows, the engine-captures piece
> (12a), and the full golden parity matrix — not missing dispatch.

| Scope | Rating | Meaning |
|---|---:|---|
| Overall OQL readiness | 8/10 | Strong V1 basics + all coverage gaps closed (continuations, capability diagnostics, diff lanes, materialize checkpoint, typed V2 params/rows, structural captures). Remaining drag is human rendering + live-network golden breadth. |
| Backend/tool reuse | 9/10 | Search delegates through `octocode-tools-core`; no second implementation of the direct tool runners was found. |
| Agent JSON workflow | 8.5/10 | `--json`/`--explain`/raw-schema fallback plus typed params, typed record-row contracts, and universal `next.*` continuations. |
| Human CLI replacement | 5.5/10 | Plain rendering is not yet enough for V2 record targets; agents should prefer `--json` for those. |
| Replace-all confidence | 8/10 | Strong for `code`/`content`/`structure`/`files`/`diff`/`materialize` and now typed repo/pkg/PR/commit/artifact rows; LSP remote + live human rendering remain the soft spots. |

Target ratings:

| OQL target | Rating | Why |
|---|---:|---|
| `code` | 8.5/10 | Local text/regex/AST and GitHub provider search route well; remote AST/PCRE2 proof still requires explicit materialization. |
| `content` | 8/10 | Local/GitHub content reads, ranges, match windows, and minification are strong; exact proof must remain explicit. |
| `structure` | 8/10 | Local/GitHub tree browsing maps cleanly; large-tree pagination and empty-directory semantics still need golden parity tests. |
| `files` | 8/10 | Local file predicates are strong; negative/provider-universe cases must force local/materialized proof. |
| `semantics` | 6.5/10 | Raw LSP is powerful; code/semantics rows now emit `next.semantic`/`next.fetch` and the `documentSymbols` parity narrative is recorded. Remote workspace roots, server diagnostics, and typed per-op rows still need work. |
| `repositories` | 7/10 | Typed params schema + documented `OqlRepositoryData` row contract; live-network golden + richer rendering still future. |
| `packages` | 6.5/10 | Typed params + `OqlPackageData` contract; envelope `next.page` promoted. Repository-handoff continuation still future. |
| `pullRequests` | 6.5/10 | Typed params + `OqlPullRequestData` contract; symbols-view flagged. PR-detail sub-page char cursors still ride params. |
| `commits` | 6.5/10 | Typed params + `OqlCommitData` contract; envelope pagination promoted. PR handoff still future. |
| `artifacts` | 6.5/10 | Binary/archive adapter exists; extracted `localPath` now emits `next.structure`/`next.files`. Typed row contracts per mode still future. |
| `diff` | 7/10 | Direct file diff and PR patch diff are now separate typed lanes (`{baseRef,headRef,path}` vs `{prNumber}`); neither → repair. Hunk-level fidelity of the local line diff can still improve. |
| Materialization lane | 7.5/10 | First-class `target:"materialize"` checkpoint row (`localPath`/`repoRoot`/`cache`/`complete`) with `next.structure`/`next.files`, bounded-scope enforced. Replaces `clone`/`cache fetch` for proof flows. |

Feature ratings:

| Feature | Rating | Keep / improve |
|---|---:|---|
| Canonical language shape | 8/10 | `target/from/scope/where/fetch/materialize` is coherent; `search --scheme` target text must stop lagging active targets. |
| Normalization and planning | 8/10 | `--explain --dry-run` is the right agent affordance; diagnostics and repair text need current active-target awareness. |
| Raw-tool coverage | 8.5/10 | All tools routed; V2 targets now have typed `params` schemas + documented record-row contracts. |
| Returned data | 7.5/10 | Core envelope + documented per-recordType `data` interfaces and typed row aliases; full per-target promoted row shapes still future. |
| Pagination | 7/10 | Envelope `next.page` promoted from backing pagination; binary scan cursor typed (`next.artifactStrings`). PR-detail sub-cursors still ride params. |
| Minification/content views | 8/10 | File content is strong; a `symbols` view on PR/commit/diff content now emits `signatureUnsupported` instead of silently degrading. |
| Structural/AST search | 7.5/10 | Local/materialized AST search is strong; remote AST must always materialize bounded code first. |
| LSP semantics | 6/10 | All 9 raw LSP types exist; quick-command and remote-materialized semantics need clearer parity and tests. |
| Fetch-to-local | 7.5/10 | `target:"materialize"` gives explicit `localPath`/`repoRoot` provenance and executable `next.structure`/`next.files` follow-ups; extracted artifacts continue locally too. |
| Diagnostics/evidence | 7.5/10 | Proof/candidate/partial model is right; capability diagnostics now fire (`signatureUnsupported`, `partialResult` on metavars, `staleCache`, `materializationNotAllowed`, diff repair). Sanitizer/rate-limit paths still need stricter tests. |
| Human renderer | 5/10 | V1 row types render acceptably; V2 `record` rows must become visible and useful outside `--json`. |
| Parity tests | 7.5/10 | Local raw-tool-vs-OQL golden tests landed (code/content/files); V2 targets mocked. Live-network goldens still future. |

Rating rule for future audits:

- `9-10`: Replace by default.
- `7-8`: Use confidently with known caveats and `--json`/`--explain`.
- `5-6`: Adapter is present; require raw-schema fallback before relying on it.
- `<5`: Do not market or document as a replacement yet.

## Removed Quick Command Replacement Matrix

These command names were earlier quick-command surfaces. They are removed from
the current CLI; use `search` with the mapped target or flag shape instead.

| CLI command | Should `search` replace it? | OQL mapping | Current parity check |
|---|---|---|---|
| `grep` | Yes | `target:"code"` | Text/regex/AST, discovery/detailed views, context, include/exclude, match paging, count/onlyMatching controls, `--repo` materialization. |
| `cat` | Yes | `target:"content"` | Exact/standard/symbols views, line/char/match ranges, `--repo` materialization, raw vs minified rendering. |
| `ls` | Yes | `target:"structure"` or symbols | Tree browsing plus `--symbols`/file outline. Raw LSP `documentSymbols` remains the authoritative semantic outline path. |
| `find` | Yes | `target:"files"` | Field predicates, metadata, content-contained queries, negative queries, `--repo` materialization. |
| `lsp` | Yes | `target:"semantics"` | Local and remote-as-local LSP; use `search <file> --op documentSymbols` or `search <file> --symbols` for outline coverage, and raw `lspGetSemantics` for schema-exact parity probes. |
| `repo` | Yes | `target:"repositories"` | Repo discovery rows, sorting/filter params, pagination. |
| `pkg` | Yes | `target:"packages"` | Package metadata, repository handoff, npm fallback diagnostics. |
| `pr` | Yes | `target:"pullRequests"` or `diff` | PR list/detail modes, comments/reviews/patch selectors. |
| `history` | Yes | `target:"commits"` or `pullRequests` | Commit history, path/subtree, PR handoff, rename/diff behavior. |
| `binary` | Yes | `target:"artifacts"` | Inspect/list/extract/decompress/strings/unpack modes, string scan offsets, char windows, and output paths. |
| `unzip` | Yes | `target:"artifacts"` | Exposes extracted `localPath`; rows now emit `next.structure`/`next.files` follow-ups rooted at it. |
| `clone` | Yes | `target:"materialize"` | `target:"materialize"` returns a first-class checkpoint row + continuations; `npx octocode cache status` / `npx octocode cache clear` remain management. |
| `cache fetch` | Yes | `target:"materialize"` | Returns materialized `localPath`/`repoRoot`/`cache`/`complete` + `next.structure`/`next.files`; `npx octocode cache status` / `npx octocode cache clear` remain management. |
| `diff` | Yes | `target:"diff"` | Two typed lanes: PR patch (`{prNumber}`) and direct file (`{baseRef,headRef,path}`); neither → repair diagnostic. |
| `search` | Already | OQL runner | Current read-only quick surface. Must stay a thin wrapper over tools-core. |
| `tools` | No | Raw tool runner | Keep for schema-exact debug, parity probes, and compatibility. |
| `context` | No | Protocol/help surface | Keep for agent bootstrapping and schema discovery. |
| `install`, `auth login`, `auth logout`, `auth status` | No | Management | Keep outside OQL. |

## Minification And Content Views

The agent-facing names can differ between surfaces, so parity must compare
behavior, not just flag names.

| Surface | Control | Values | Audit note |
|---|---|---|---|
| Raw content tools | `minify` | `none`, `standard`, `symbols` | `none` is exact text for quotes/diffs; `standard` is the default compact read; `symbols` is the smallest orienting skeleton. |
| CLI `search` content | `--content-view` | `exact`, `compact`, `symbols` | Current quick flags are kebab-case; raw and OQL fields are camelCase. Read help before copying examples. |
| OQL `search` content | `fetch.content.contentView` / `contentView` | Exact/compact/symbol-oriented equivalent | Must report the view used and preserve exact text when proof requires it. |
| PR content | `minify` | `none`, `standard` | PR bodies/diffs do not expose `symbols`; keep this limitation visible. |
| Search snippets | `matchContentLength`, context, `onlyMatching` | Tool-specific | Snippets are discovery. Re-read content with exact mode before quoting or diffing. |

Special minified-file check: for minified one-line bundles, line snippets may
only prove that a line matched. `localSearchCode.onlyMatching:true` plus
`matchWindow` must still enumerate individual hits and give enough context to
follow up.

## Pagination And Continuations

Every pagination domain must keep its own cursor. Do not flatten these into one
generic `page` unless the continuation is still executable and unambiguous.

| Domain | Raw controls to preserve |
|---|---|
| Result rows | `page`, `itemsPerPage`, command `--page`, `--page-size`, `limit`. |
| Local search matches | `matchPage`, `maxMatchesPerFile`, `maxFiles`, `matchContentLength`; count/onlyMatching/unique modes can change row identity. |
| Content windows | `charOffset`, `charLength`, line ranges, `matchString` + `contextLines`. |
| GitHub PR/history | PR list `page/itemsPerPage`, selected PR `filePage`, `commentPage`, `commitPage`, per-content `charOffset/charLength`. |
| Repository/package search | provider `page`, provider limits, concise vs verbose rows. |
| Tree/file listings | `page`, `itemsPerPage`, `maxDepth`, `limit`, sort/filter controls. |
| Binary/archive | `entryPageNumber`, `entriesPerPage`, `maxEntries`, `scanOffset`, `nextScanOffset`, text char windows. |
| LSP | `page`, `itemsPerPage`, `depth`, grouped vs ungrouped rows. |

Continuation quality gate: top-level OQL `next` should be executable without
reading target-specific raw payload internals. If the only continuation is a
raw `data.next`, mark the parity grade `adapter-present` or `replace-json-only`.

## AST And Structural Coverage

Structural search parity is required for local/materialized code:

- CLI quick route: `search --pattern <code-shape> <path> --lang <lang>` or
  `search --rule <yaml-rule> <path> --lang <lang>`.
- Raw tool: `localSearchCode mode:"structural"` with exactly one of `pattern` or
  `rule`.
- Pattern metavariables: `$X` matches one node; `$$$` or named variadic forms
  match node lists.
- YAML rules must preserve relational operators such as `inside`, `has`, `not`,
  `all`, and `any`; relational sub-rules need `stopBy: end` when scope matters.
- Comments and strings must not false-positive as code nodes.
- GitHub provider search is not AST search. For remote AST proof, `search` must
  materialize a bounded repo/subtree first and then run the local structural
  runner.

Structural edge-case probes:

| Probe | Expected parity |
|---|---|
| Code pattern, e.g. `eval($X)` | Metavars survive in row data and renderer. |
| YAML relational rule | Rule reaches the same local runner without CLI-only parsing. |
| Remote repo with structural predicate | Plan shows materialization or returns a repair diagnostic. |
| Negative structural query | Requires a complete local/materialized universe before proof. |

## LSP Coverage

Raw `lspGetSemantics` supports all of these `type` values:

`definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`,
`documentSymbols`, `typeDefinition`, `implementation`.

Parity checks:

- `documentSymbols` exists in the raw LSP tool. Current CLI outline coverage is
  through `search <file> --op documentSymbols` or `search <file> --symbols`.
- `symbolName` and `lineHint` are identity anchors, not optional decoration, for
  most semantic operations.
- `workspaceRoot` must point at the real local or materialized project root, not
  just the single file cache path.
- Remote LSP requires materialization; preserve clone/cache provenance and any
  `serverUnavailable` or language-server diagnostics.
- Pagination and grouping must preserve `page`, `itemsPerPage`, `groupByFile`,
  `format:structured|compact`, `includeDeclaration`, `depth`, and context lines.

## Fetch-To-Local Coverage

`search` can replace remote workflows only when it can either prove through the
remote provider or materialize code and continue locally.

| Surface | Fetch behavior to cover |
|---|---|
| `ghCloneRepo` / `clone` | Clone repo or sparse subtree, honor `branch`/`sparsePath`, `forceRefresh`, clone enablement, and return absolute local path. |
| `cache fetch` | Materialize file/tree/clone into Octocode cache, return `localPath`, `repoRoot`, `source`, `complete`, `cached`. |
| `search --repo <owner/repo>` with code flags | Materialize remote repo/subtree, then run local text/regex/structural search. |
| `search <file> --repo <owner/repo>` with content flags | Materialize or fetch remote file, then read with local content controls. |
| `search <path> --repo <owner/repo> --tree` / `--search path` | Materialize remote scope, then run local structure/file tools. |
| `search <file> --repo <owner/repo> --op ...` | Materialize remote file/project before semantic navigation. |
| `unzip` / binary `unpack` | Produce a new local directory and continue with local `search`, `--tree`, `--search path`, content reads, or LSP operations. |

Required diagnostics: stale cache, force refresh, clone disabled, sparse path not
found, full-repo materialization risk, auth/rate failure, and any mismatch
between requested ref/path and returned local path.

## Agent Parity Procedure

Use this procedure before declaring `npx octocode search` a replacement for any
tool or command.

1. Read the live schema:

```bash
npx octocode search --scheme
```

Check that `activeTargets`, `query.target`, `from`, `params`, and target-specific
examples agree. Drift here is a blocker for agent trust.

2. Run a dry plan:

```bash
npx octocode search --explain --dry-run --query '<json>'
```

Check:

- `plan.normalized` has no CLI sugar.
- `plan.backendCalls[*].backend` is the same raw tool the old workflow used.
- `plan.backendCalls[*].exact` is honest.
- `plan.materialization` is explicit and bounded when GitHub source needs local
  proof.
- `plan.diagnostics` explains unsupported/residual/approximate behavior.

3. Run the OQL query with `--json`.

Check:

- `results[*].kind` is useful for the target.
- `diagnostics` do not block the answer.
- `provenance[*].backend` names the backing runner.
- `evidence.kind` is `proof` for answer-ready claims.
- `pagination.hasMore` and `next` are carried forward when more data exists.

4. Run the old command/tool for the same question.

Compare:

- row count and identity keys;
- paths, repo/ref, package, PR, commit, artifact identity;
- lines/ranges/snippets or payload bodies;
- pagination and continuation shape;
- diagnostics and failure mode;
- token density and human rendering.

5. Grade the replacement:

| Grade | Meaning |
|---|---|
| `replace` | OQL output is at least as complete, typed, and followable as old output. |
| `replace-json-only` | JSON is good, but human rendering hides important rows. |
| `adapter-present` | Backend call exists, but row/params/next parity is incomplete. |
| `not-yet` | OQL cannot express the workflow honestly. |

## Returned Data Requirements

All targets must return the common envelope:

- `results`
- `pagination`
- `next`
- `diagnostics`
- `provenance`
- `evidence`
- `plan` when `explain:true`

Target-specific minimums:

| Target | Minimum useful row data |
|---|---|
| `code` | source, path, line, column/endLine when known, snippet, metavars for structural search, row-level fetch/semantics continuation. |
| `content` | source, path, content, contentView, line/char range, char continuation. |
| `structure` | source, path, entryType, depth, size when available, drill/fetch continuation. |
| `files` | source, path, entryType, size/modified when requested, fetch/search continuation. |
| `semantics` | location/symbol/call/hover payload, uri, range, symbol identity, operation type. |
| `repositories` | owner, repo, stars/forks/language/topics/pushedAt, structure/search/clone continuations. |
| `packages` | name, version, description, downloads, repository, package subdir, repo/search/clone continuations. |
| `pullRequests` | number, title, state, author, dates, changed files, selected content, detail continuation. |
| `commits` | sha, title/message, author/date, touched path/ref, PR handoff when available. |
| `artifacts` | mode, format, entries/strings/symbols, extracted/decompressed localPath when created. |
| `diff` | file path, hunks/patches, additions/deletions, PR/file provenance. |

Generic `kind:"record"` rows are acceptable only as a transitional layer. Before
full replacement, agents need either typed row sub-shapes or documented
`recordType` payload contracts.

Human rendering is part of returned-data parity. If `--json` exposes useful
`kind:"record"` rows but plain text hides them, grade that surface
`replace-json-only`, not `replace`.

## Research Quality Gates

An answer is research-quality only when:

- `evidence.answerReady === true`;
- `evidence.kind === "proof"`;
- `evidence.complete === true`;
- every required predicate is represented in `plan.nodes` or target params;
- no blocking diagnostic is present;
- provider approximations are either acceptable to the task or followed by
  materialized proof;
- the agent can cite a stable identity: file:line, repo/ref/path, package
  name/version, PR/commit id, artifact path, or LSP location.

Do not treat these as proof:

- provider zero matches without complete predicate evaluation;
- `candidate` evidence;
- `partial` evidence caused by pagination/truncation;
- generic package/repo/history rows without checking the requested params;
- snippets when exact file content is required;
- stale clone/cache diagnostics without deciding whether freshness matters.

## Advanced Edge Cases To Check

| Area | Edge case | Required behavior |
|---|---|---|
| Schema drift | `activeTargets` differs from `query.target` help or diagnostics repair text | Fix source of truth before relying on agent instructions. |
| CLI/help drift | Quick command flags differ from raw schema fields | Keep both names documented; never copy raw camelCase fields into quick examples. |
| Opaque V2 params | `params` accepts anything but docs do not name fields | Read raw tool schema; add typed target docs before replacement. |
| Human rendering | `kind:"record"` rows are invisible or too terse | Use `--json`; renderer must support record rows before human parity. |
| V2 continuations | backing tool returns `data.next` instead of OQL `next` | Promote to OQL continuations so agents can follow them uniformly. |
| Pagination | result pages, per-file match pages, char offsets, archive entries, semantic rows | Preserve the exact pagination domain and expose executable continuation. |
| Minification | `none`/`standard`/`symbols` support differs by tool | Preserve exact text for proof; expose unsupported views as diagnostics. |
| Batch merge | incompatible row kinds or pagination domains | Reject with repair diagnostic; do not silently merge. |
| GitHub regex | provider search cannot prove local regex semantics | Mark candidate/approximate or materialize bounded scope. |
| Structural search | GitHub cannot run AST search directly | Require bounded materialization. |
| Negation | provider source lacks complete universe | Require materialization or return `negativeUniverseRequired`. |
| Remote LSP | repo/file must materialize before LSP | Show clone provenance, local URI, and LSP diagnostics. |
| Binary/archive | extraction/decompression writes derived files | Return localPath and follow-up local `search`/`structure` continuations. |
| PR/history | broad list mode vs detail mode have different payloads | Preserve mode and content selectors; avoid pretending list rows include full detail. |
| Diff | direct file diff and PR patch diff are different workflows | Represent both explicitly or mark unsupported. |
| Auth/rate | GitHub/npm calls can fail externally | Return `rateLimited`, auth/error diagnostics, and non-proof evidence. |
| Secrets | output may be sanitized | Preserve `sanitized` diagnostic so agents know content changed. |
| Build freshness | built CLI may be older than source | Rebuild before scoring search behavior. |

## Current Gap Log

Status legend: ✅ closed · 🟡 partial · ⬜ open. Last updated 2026-06-22.

1. ✅ `npx octocode search --scheme` now lists every active target in `query.target`
   (derived from `ACTIVE_TARGETS`).
2. ✅ `unsupportedTarget` (and "could not determine target") repair text now
   names the current active targets.
3. ✅ V2 target `params` now have **typed Zod input schemas** per target
   (`v2params.ts`, validated in normalize): a type mistake on a known field
   (e.g. `prNumber:"abc"`) fails with `invalidQuery params.prNumber: …` instead
   of failing opaquely at the tool. `.passthrough()` keeps the backing tool the
   exhaustive validator for the rest. `--scheme` `params.*` hints still apply.
4. ✅ `kind:"record"` rows carry a stable `id` + `recordType` **and** documented
   per-recordType `data` interfaces with typed row aliases (`OqlRepositoryRow`,
   `OqlPackageRow`, `OqlPullRequestRow`, `OqlCommitRow`, `OqlArtifactRow`,
   `OqlDiffRow`, `OqlSemanticsRow`, `OqlMaterializedRow`) in `types.ts`.
5. ✅ Human rendering now handles `kind:"record"` — renders `recordType` + `id`
   + key fields (stars/lang/desc, PR state/title, commit title, etc.).
6. ✅ Per-domain continuations: envelope `next.page` is promoted from the backing
   tool's pagination, and the binary `strings` scan cursor now emits a typed
   `next.artifactStrings` (`scanOffset`). Remaining sliver: PR *detail* sub-page
   char-offsets (file/comment/commit) still ride `params` — narrow and
   documented.
7. ✅ `clone`/`cache fetch` first-class materialization checkpoint — done.
   `target:"materialize"` clones/caches a bounded corpus and returns a
   `recordType:"materialized"` checkpoint row (`localPath`/`repoRoot`/`ref`/
   `cache`/`complete`) with `next.structure`/`next.files`; unbounded clone is
   refused with `materializationNotAllowed`. Live-verified
   (`adapters/materialize.ts`, `run.ts`).
8. ✅ `diff` direct-file vs PR-patch distinction — done. `executeDiff`
   discriminates `{prNumber}` (PR patch) vs `{baseRef,headRef,path}` (direct file
   via `ghGetFileContent` ×2 + pure `computeLineDiff`); neither → `invalidQuery`
   repair (no silent PR call). (`adapters/v2.ts`.)
9. ✅ `unzip`/`unpack` extracted-localPath follow-up continuations — done.
   `record:artifact` rows with a derived `localPath` emit `next.structure`/
   `next.files` rooted at it (`run.ts` continuation registry).
10. ✅ LSP outline vs raw `documentSymbols` parity narrative — done. Local code rows
    emit `next.semantic` (`type:"documentSymbols"`) + `next.fetch`; semantics rows
    emit `next.fetch`. Parity: `documentSymbols` is reachable via
    `target:"semantics",params:{type:"documentSymbols"}`, raw
    `lspGetSemantics`, `search <file> --op documentSymbols`, or
    `search <file> --symbols`.
11. ✅ PR content `none|standard`-only (no `symbols`) visibility — done.
    `checkOutputFeatures` emits `signatureUnsupported` + repair when a `symbols`
    view is requested for `pullRequests`/`commits`/`diff` (`features.ts`).
12. ✅ Golden parity tests landed: `tests/oql/golden-parity.test.ts` compares the
    raw tool output to OQL output for the local targets (code path:line set,
    content byte-identical, files basename set), plus the typed-shape/identity/
    continuation suites (parity-gaps, review-fixes, v2-targets, open-gaps-fixes,
    open-gaps-materialize). GitHub/npm targets are covered via mocked runners;
    live network goldens remain future.
13. ✅ Gate 15 (negation/xor proof over a provider) fixed: `not`/`xor` over a
    GitHub source no longer claims provider `proof`. Negation parity is threaded
    into leaf routing, so a negated predicate ROUTEs to bounded materialization
    (local proof) under `materialize.mode:"auto"/"required"`, returns
    `negativeUniverseRequired` + `UNSUPPORTED` under `"never"`, and double
    negation correctly collapses to a positive provider search. Tests:
    `tests/oql/provider-negation.test.ts`.
14. ✅ `content`/`structure` over a GitHub source now require a concrete
    repository (`owner/name`): a provider-wide (`{kind:"github"}`) or owner-only
    source is rejected with `invalidQuery` + repair instead of silently planning
    an executable read that fails opaquely at the tool layer (contract
    §source-and-scope). Tests: `tests/oql/github-corpus.test.ts`.
15. ✅ Content view labelling fixed: a content row now reports the view it was
    *asked* for (`exact`/`compact`/`symbols`) instead of the tool's unreliable
    echo (a `symbols` read previously mislabelled itself `compact`). Local +
    GitHub content paths. Tests: `tests/oql/content-views.test.ts`.
16. ✅ Content char-window pagination fixed: a windowed content read now carries
    `range.charOffset`/`charLength` and emits a first-class `next.charRange`
    continuation; `target:"content"` no longer emits a misleading `next.page`
    (wrong pagination domain). Local + GitHub content paths. Tests:
    `tests/oql/content-views.test.ts`.
17. 🟡 Capability regression coverage added for match-string anchored reads and
    match-anchored `contextLines` (verified `contextLines` applies to match
    anchors, not explicit line ranges). Tests: `tests/oql/content-views.test.ts`.
18. ✅ Structural `metavars` — **fully closed**. The earlier premise ("engine
    returns no captures") was wrong: the structural engine DOES return
    `metavars` + `metavarRanges` (verified via raw `localSearchCode`), they
    survive the tool result, and `mapCodeResult` now forwards both into
    `row.metavars`/`row.metavarRanges`. The core type `LocalSearchCodeMatch` was
    the only gap — now declares the fields (octocode-core source + dist sync).
    No diagnostic needed (captures are present, not absent). Tests:
    `tests/oql/open-gaps-fixes.test.ts`.

> **2026-06-22 — all coverage gaps closed.** Gaps 7–12 (open-gaps doc) and
> checklist items 3, 4, 6, 12, 18 closed from root cause: continuation registry
> + `checkOutputFeatures` (`run.ts`/`features.ts`), diff lane split,
> `target:"materialize"` checkpoint, typed V2 params (`v2params.ts`), typed
> record-row contracts (`types.ts`), binary scan cursor, golden parity tests, and
> structural captures (engine already produced them; core type + forwarding
> fixed). Live-verified through the rebuilt CLI; tools-core suite **977 passing**
> (+20). Remaining future work is breadth, not blockers: live-network golden
> matrix, PR-detail sub-page char-offset cursors, and richer human rendering.

## Minimal Parity Test Suite

For each row in the raw tool matrix, keep one golden test:

1. Raw tool input.
2. Equivalent OQL input.
3. `--explain --dry-run` backend call assertion.
4. OQL execution JSON shape assertion.
5. Old-vs-OQL row identity comparison.
6. Diagnostics/evidence assertion.
7. Continuation assertion.
8. Human renderer assertion when the command is user-facing.

Recommended probe order:

```bash
npx octocode search --scheme
npx octocode search --explain --dry-run --query '<oql>'
npx octocode search --json --query '<oql>'
npx octocode tools <rawTool> --scheme
npx octocode tools <rawTool> --json --queries '<raw>'
```

## Replacement Readiness Summary

Current direction is correct: `search` delegates to tools-core, V2 adapters call
existing tool runners, and CLI shorthand lowering has moved into tools-core.

The remaining risk is agent trust, not basic dispatch. To fully replace all
research tools, make target-specific params, record payloads, continuations,
renderer output, and old-vs-new parity tests as strong as the existing V1
code/content/structure/files path.
