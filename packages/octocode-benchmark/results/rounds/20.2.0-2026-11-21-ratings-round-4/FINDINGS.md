# Findings: rating round 4, 2026-11-21

Every new finding from [rating round 4](SCORECARD.md) on `octocode 20.2.0+56f67471`, as the raters reported them.
"Confirmed" means the coordinator re-ran it on the pinned binary. Each lane's full report, with every exact input and
its ground truth, is in [lanes/](lanes/). This copy does not change.

Severity: **high** wrong evidence, lost evidence or money spent without consent; **med** wrong lead, slow, or
costly; **low** wording, size or polish. Owner: **core** (agent text and schemas), **native** (Rust behavior).

## Code intelligence (astSearch 8, gated 4; lspSearch 8) ([lane](lanes/code-intelligence.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-K1 | astSearch | A private-key block left open to the end of a file hides every later match: `$X.clone()`, `hide_key_line($X)` and `$X.unwrap_or_default()` on `crates/runtime-base/src/security/content/tests/key_blocks.rs` return `empty` with no warning or partial reason. Same class as R3-K2, new shape: the test at :136-139 builds an unterminated `BEGIN … KEY` block, and redaction swallows the rest of the file before parsing (syntax tree `ERROR 136:1-489:2`; the raw file has none). | ast-grep 0.45.0 finds 5 / 2 / 4 (:308, :337, :339-341 for `clone`). **Confirmed**: status empty, 0 rows, only an info `structural.pattern.shape` diagnostic. | high (gate) | native |
| R4-K2 | lspSearch | `workspaceSymbol` with only `workspaceRoot:"packages/octocode-config"` is empty: the server picks the project of `scripts/check-core-contract-sync.cjs`, outside the tsconfig project. Disclosed, but the default is a dead end. | The same query with `path:".../src/dotenv.ts"` returns 9+ symbols | med | native |
| R4-K3 | lspSearch | Rust `references` of `ErrorCode` at `crates/engine/src/error.rs:114`: 106.5 s cold, 19.5 s warm. The answer is right. | Timed on the pinned binary under machine load 3+ per CPU | low | native |
| R4-K4 | lspSearch | An anchor at line 20 resolves to line 501 (`lineDeviation:481`) with no warning. | Report input K4 | low | native |
| R4-K5 | lspSearch | The notCallable reason reads "a enum". | Report input K5 | low | native |
| R4-K6 | astSearch | `--help` lists a redundant required-field row, "match (with view)". | `octocode astSearch --help` | low | core |

## Graph (graphBuild 8.5; graphQuery 8, gated 4) ([lane](lanes/graph.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-G1 | graphQuery | Symbol `impact` is wrong both ways: `{"operation":"impact","path":"packages/octocode-native","target":"crates/engine/src/num.rs#saturating_u32","kind":"symbol","depth":1}` lists files that use only the sibling `saturating_usize` (e.g. `runtime-base/src/response/pages/publish.rs`) and omits `tool-local/src/structure_search/listing.rs`, whose call at :176 sits inside `macro_rules! listing_fields` (:111). The miss is in neither callers nor `unresolved`; no warning names it. | rg: publish.rs has only `saturating_usize` (:220); listing.rs calls `saturating_u32` at :176. **Confirmed**: 3 pages, 104 unique rows, publish.rs listed, listing.rs absent; coverage says `callExtraction: supported`. | high (gate) | native |
| R4-G2 | graphQuery | `testsToRun.functions` names module-level helpers (`FAKE`, `OPEN`, `root` in `attach-gate.test.mjs`) instead of the anonymous `test(...)` calls at :46, :68, :128; `testFunctions` (177) is inflated. | Read of the test file | med | native |
| R4-G3 | graphQuery | A Rust call through an inline `#[path]` module (`inl::deep::deepf()`) stays unresolved with the wrong reason `member-unknown-receiver-in-repo` (the site is disclosed). | Fixture with known edges | low | native |
| R4-G4 | graphQuery | `sites:1` where `alpha` calls `helper` twice (by import and through the namespace); `witnesses` shows 2. | Fixture | low | native |
| R4-G5 | graphQuery | On a stale snapshot 11 of 592 `mentionedIn` lines were wrong (`freshness:false` was flagged); after a rebuild 575 of 575 were right. | Stale vs rebuilt snapshot | low | native |
| R4-G6 | graphQuery | Impact rows omit `from` at depth 1 and carry it at depth 2. | Same target at both depths | low | native |

## clasify (clasify 8; search `classification` option 6) ([lane](lanes/clasify.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-C1 | localSearch, ghSearchCode (`classification`) | The search `classification` option bills without a cap or consent: `{"path":"packages/octocode-native/crates","matchString":["Mutex::new"],"regexEngine":"literal","classification":{"predicates":["The hit guards process-wide state."]}}` billed 131 requests (74,799 input tokens) in 9.7 s, with no price or confirm first. clasify itself prices for free and needs `next.confirm`. | **Confirmed** without billing: `octocode schema localSearch` / `ghSearchCode` give `classification` only `context`, `longContext`, `predicates`; the only bound is `SelectionLimits { max_chars: 131_072 }` (`crates/runtime-base/src/execution/limits.rs:57`), one request per file | high | core + native |
| R4-C2 | clasify | R3-C1 partly fixed: a 170 KB source with `maxRequests:1` now returns a price (7 sections, ceiling 12) and `next.confirm`, but spends 1 request first; window sizes are remembered only in process memory (`judgment/windows.rs`). | Report input C1 | med | native |
| R4-C3 | classification | Overlapping hit windows blur single-hit judgments: `process.rs:148` `Err(error)` judged quiet while :147 got 0.31; the helper-built error at :403 missed; `arr[i]` 0.48. | Hand labels read in source | med | native |
| R4-C4 | clasify | Cost grows per section and per item: a 62 KB file is 29 sections of about 40 lines, one request each; `perItem` sends one request per line. | Plans read for free | low | native |
| R4-C5 | clasify | Replayed pages still show `usage.calls:1` beside `replayed:true` (a plain sum double-counts); a details replay of a quiet-only page shows `sections: []`. | Replay of sections details | low | native |
| R4-C6 | clasify | Guidance slips: the score-question error example shows a yes/no question; the `maxRequests:0` example omits `maxRequests`; `score:1.97` appears without its scale; a `cap.rs` citation spans lines 1-113. | Error rows and a cited read | low | core + native |

## Local (structureSearch 9, localSearch 9, localFetch 9) ([lane](lanes/local.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-L1 | localFetch | After a case-sensitive miss the warning says "3 lines match ignoring case": it is 3 matches on 2 lines (`har-ingest.test.mjs`, `REDACTED`). | `rg -i` finds 2 lines | low | native |
| R4-L2 | localFetch | `view:"symbols"` shows the macro call in `tree.rs` as its first line and `…`, without a span, and lists none of its 6 inner functions. | Read of the file | low | native |
| R4-L3 | structureSearch | The refusal for `modifiedAt:"P1D"` gives an example without `modifiedAt` (it should show `{"from":"-P1D"}`). | Refusal row | low | native |
| R4-L4 | localSearch | With 2+ terms in a count view, per-term counts are given per file only; a total per term needs every page summed. | Count view walk | low | core + native |

## GitHub read (ghSearchRepo 9, ghSearchCode 8.5, ghStructure 9, ghGetFileContent 8.5) ([lane](lanes/github-read.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-GR1 | ghGetFileContent | `block:true` on a PHP method whose `{` sits on its own line returns only the signature (marked an estimate) and no `matchedLines`: laravel `Str.php` `slug(` gives 1665-1665 (real 1665-1688); symfony `Application.php` `run(` gives 196-196 (real 196-285). | Raw files via `gh api` | med | native |
| R4-GR2 | ghSearchCode, ghSearchRepo | Qualifier-like keywords (`language:rust`, `"ripgrep stars:>1000"`) are searched as text and come back empty; the hint says to drop a term but never names `extensions`, `filters.language` or `filters.stars`. | `gh api search` with the qualifier | med | core + native |
| R4-GR3 | ghGetFileContent | A paged minified read switches unit on its last page (pages 1-9 bytes, page 10 lines with a `7\t` prefix); page 1 reports `returnedLines:6` for 17 KB of 140 KB. | 10-page walk rebuilt the file | low | native |
| R4-GR4 | ghSearchCode | A `ref` code search (R3-GR1) warns correctly but gives no runnable call to search at that ref. | Warning row | low | native |
| R4-GR5 | ghSearchRepo | The output reference lacks `defaultBranch`; a last page carries `expiresAt` with no next call. | `--view output` | low | core |
| R4-GR6 | ghStructure | `operation:"tags"` is refused plainly instead of being repaired to `refs`. | Refusal row | low | native |

## GitHub history (ghSearchHistory 9, ghGetHistoryItem 8.5, ghCloneRepo 9) ([lane](lanes/github-history.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-H1 | ghGetHistoryItem | Files whose patch GitHub withholds get no read continuation unless they fall on the first response page, and are marked `terminalLimit` ("unreachable by any call") although ghGetFileContent reads them at head or base: kubernetes#141224 (22 files, pages 6, 12, 20 of 27); TypeScript#51387 with `include:["src/**"]` (196 of 238 files without a read). Rest of R3-H1. | `gh api .../pulls/141224/files` `patch==null` set | med | native |
| R4-H2 | ghSearchHistory | Search rows can't carry `mergedBy` (`fields:["mergedBy"]` warns "No row has mergedBy"); ghGetHistoryItem now returns it. Rest of R3-H2. | fd#1571 `.merged_by` | med | core + native |
| R4-H3 | ghCloneRepo | `historyDepth` is described as "Commits of history"; fd@v10.2.0 with `historyDepth:5` holds 13 commits (git depth counts generations). | `git rev-list --count` | low | core |
| R4-H4 | ghGetHistoryItem | The commit patches view drops the author's name and email that the summary view shows (commit 9e79f8c2). | Both views | low | native |
| R4-H5 | ghCloneRepo | A one-path sparse clone warns "only src/compiler/checker.ts are on disk". | Warning text | low | native |

## Packages (artifactSearch 8.5, securityCheck 8.5) ([lane](lanes/packages.md))

| # | Tool | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-P1 | securityCheck | Go `github.com/gin-gonic/gin/binding` v1.9.0: the summary says `github.com/gin-gonic/gin@v1.9.0: none known to OSV` and warns OSV doesn't list GHSA-2c4m-59x9-fr2g, while OSV lists it (and GO-2023-1737) for the gin module; no lead to the `moduleOnly` list the details view has. | api.osv.dev query for the module | med | native |
| R4-P2 | securityCheck | The details view repeats each advisory once per database with the same id and no field naming the database: rack has 35 advisories in 98 elements (251 KB), with differing severities. | OSV ids for rack | med | native |
| R4-P3 | securityCheck | `@Angular/core` gets no lowercase lead (artifactSearch now gives one). | Error row | low | native |
| R4-P4 | artifactSearch | Go paths inside a module drop `publishedAt` (testify/assert, gin/binding). | proxy.golang.org | low | native |
| R4-P5 | artifactSearch | No lookalike lead for PyPI `reqeusts` (deferred in round 5: one provider request per candidate). | pypi.org | low | native |
| R4-P6 | artifactSearch | npm searches with 2+ keywords: 19 s cold, short pages, npm 429 after about 10 pages. | Timed walk | low | native |

## End to end and guidance (3 of 3 answers right; guidance 9) ([lane](lanes/end-to-end.md))

| # | Area | Finding | Evidence | Sev | Owner |
|---|---|---|---|---|---|
| R4-F1 | Validation errors | The `Example` of a validation error drops the bad field (`contextLines`, `createdAt`) instead of showing its valid shape. | Three refusals in the lane log | med | native |
| R4-F2 | lspSearch | An empty definition on generated code (`buildScriptsOff`) suggests a localFetch of the same file, which only uses the symbol; a workspace text search is the useful lead. | Lane log | low | native |
| R4-F3 | Guidance | No route to find an old PR or commit (keyword search returns the newest by date; a date window was needed), and no line on reading a compare as "this release contains it". | Lane log | low | core |
